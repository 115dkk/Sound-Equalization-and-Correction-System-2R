use crate::{CancelToken, RewError};
use serde_json::Value;
use std::{
    collections::{HashMap, VecDeque, hash_map::RandomState},
    hash::{BuildHasher, Hasher},
    io::{BufRead, BufReader, ErrorKind, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const MAX_CALLBACK_BODY: usize = 4 * 1024 * 1024;
/// A request's line, headers and body must arrive within this time.
pub const CALLBACK_REQUEST_DEADLINE: Duration = Duration::from_secs(5);
/// Largest read between two stop and deadline checks.
const CHUNK: usize = 64 * 1024;
type Handler = Arc<dyn Fn(Value) + Send + Sync>;
struct State {
    routes: HashMap<String, Handler>,
    history: VecDeque<(String, Value)>,
    keep_history: usize,
}
/// Loopback-only HTTP/1.1 subscriber, with a new unguessable path namespace per
/// instance. Routes must be registered; unknown paths (including health) are 404.
pub struct CallbackServer {
    token: String,
    port: u16,
    state: Arc<Mutex<State>>,
    stop: CancelToken,
    thread: Option<JoinHandle<()>>,
}
impl std::fmt::Debug for CallbackServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallbackServer")
            .field("port", &self.port)
            .field("running", &self.is_running())
            .finish_non_exhaustive()
    }
}
impl Default for CallbackServer {
    fn default() -> Self {
        Self::new(0)
    }
}
impl CallbackServer {
    pub fn new(keep_history: usize) -> Self {
        // RandomState is seeded by the standard library's OS-backed randomness;
        // two independent seeds avoid a predictable timestamp/counter token.
        let token = (0..2)
            .map(|_| {
                let mut h = RandomState::new().build_hasher();
                h.write(b"secs-rew callback");
                format!("{:016x}", h.finish())
            })
            .collect::<String>();
        Self {
            token,
            port: 0,
            state: Arc::new(Mutex::new(State {
                routes: HashMap::new(),
                history: VecDeque::new(),
                keep_history,
            })),
            stop: CancelToken::default(),
            thread: None,
        }
    }
    pub fn is_running(&self) -> bool {
        self.thread.is_some()
    }
    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
    pub fn url_for(&self, path: &str) -> String {
        format!(
            "{}/{}/{}",
            self.url(),
            self.token,
            path.trim_start_matches('/')
        )
    }
    pub fn route(&self, path: &str, handler: Handler) -> String {
        let path = format!("/{}", path.trim_start_matches('/'));
        self.state
            .lock()
            .unwrap()
            .routes
            .insert(format!("/{}{path}", self.token), handler);
        self.url_for(&path)
    }
    pub fn unroute(&self, path: &str) {
        self.state.lock().unwrap().routes.remove(&format!(
            "/{}/{}",
            self.token,
            path.trim_start_matches('/')
        ));
    }
    pub fn history(&self) -> Vec<(String, Value)> {
        self.state.lock().unwrap().history.iter().cloned().collect()
    }
    pub fn start(&mut self) -> Result<String, RewError> {
        if self.is_running() {
            return Ok(self.url());
        }
        let listener =
            TcpListener::bind(("127.0.0.1", 0)).map_err(|e| RewError::connection("callback", e))?;
        self.port = listener
            .local_addr()
            .map_err(|e| RewError::connection("callback", e))?
            .port();
        listener
            .set_nonblocking(true)
            .map_err(|e| RewError::connection("callback", e))?;
        self.stop = CancelToken::default();
        let stop = self.stop.clone();
        let state = self.state.clone();
        self.thread = Some(thread::spawn(move || {
            let mut workers: Vec<JoinHandle<()>> = vec![];
            while !stop.is_cancelled() {
                let mut i = 0;
                while i < workers.len() {
                    if workers[i].is_finished() {
                        let _ = workers.swap_remove(i).join();
                    } else {
                        i += 1;
                    }
                }
                match listener.accept() {
                    Ok((stream, _)) if workers.len() < 16 => {
                        let state = state.clone();
                        let stop = stop.clone();
                        workers.push(thread::spawn(move || {
                            let _ = serve(stream, state, stop);
                        }));
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        stop.wait(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
            for worker in workers {
                let _ = worker.join();
            }
        }));
        Ok(self.url())
    }
    pub fn stop(&mut self) {
        self.stop.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for CallbackServer {
    fn drop(&mut self) {
        self.stop();
    }
}
fn reply(stream: &mut TcpStream, code: u16, body: &str) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {code} {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{body}",
        match code {
            200 => "OK",
            404 => "Not Found",
            413 => "Payload Too Large",
            _ => "Bad Request",
        },
        body.len()
    )?;
    stream.flush()
}
/// The server's stop flag and one request's deadline, checked between reads.
/// Each read waits at most the 1 s socket timeout, so a client that keeps
/// sending cannot hold a worker (and `stop()`) past either.
struct Guard<'a> {
    stop: &'a CancelToken,
    deadline: Instant,
}
impl Guard<'_> {
    fn check(&self) -> std::io::Result<()> {
        if self.stop.is_cancelled() {
            return Err(std::io::Error::new(
                ErrorKind::ConnectionAborted,
                "callback server stopped",
            ));
        }
        if Instant::now() >= self.deadline {
            return Err(std::io::Error::new(
                ErrorKind::TimedOut,
                "callback request deadline passed",
            ));
        }
        Ok(())
    }
}
/// Buffer at most one socket read, retried on EINTR as `read_until` and
/// `read_to_end` do; the bytes are then in `reader.buffer()`. 0 is end of stream.
fn fill(reader: &mut BufReader<TcpStream>) -> std::io::Result<usize> {
    loop {
        match reader.fill_buf() {
            Ok(available) => return Ok(available.len()),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}
fn line(
    reader: &mut BufReader<TcpStream>,
    budget: &mut usize,
    guard: &Guard,
) -> std::io::Result<String> {
    // Same bound as `take(budget + 1).read_until(b'\n')`, checked per read.
    let mut raw = Vec::new();
    while raw.len() <= *budget && !raw.ends_with(b"\n") {
        guard.check()?;
        let n = fill(reader)?;
        if n == 0 {
            break;
        }
        let window = &reader.buffer()[..n.min(*budget + 1 - raw.len())];
        let used = window
            .iter()
            .position(|&b| b == b'\n')
            .map_or(window.len(), |i| i + 1);
        raw.extend_from_slice(&window[..used]);
        reader.consume(used);
    }
    if raw.len() > *budget || (!raw.is_empty() && !raw.ends_with(b"\n")) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "header too long",
        ));
    }
    *budget -= raw.len();
    String::from_utf8(raw).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}
/// Read `length` body bytes into `keep`, or discard them, in chunks of at most
/// [`CHUNK`]. Ok(false) when the peer closed before the whole body arrived.
fn body(
    reader: &mut BufReader<TcpStream>,
    length: u64,
    mut keep: Option<&mut Vec<u8>>,
    guard: &Guard,
) -> std::io::Result<bool> {
    let mut left = length;
    while left > 0 {
        guard.check()?;
        let n = fill(reader)?
            .min(CHUNK)
            .min(usize::try_from(left).unwrap_or(usize::MAX));
        if n == 0 {
            return Ok(false);
        }
        if let Some(raw) = keep.as_deref_mut() {
            raw.extend_from_slice(&reader.buffer()[..n]);
        }
        reader.consume(n);
        left -= n as u64;
    }
    Ok(true)
}
fn serve(stream: TcpStream, state: Arc<Mutex<State>>, stop: CancelToken) -> std::io::Result<()> {
    // The listener is non-blocking so `stop` is seen between accepts. On macOS
    // and other BSDs an accepted socket inherits that flag, and a non-blocking
    // socket ignores the read timeout below; make the connection blocking.
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let mut reader = BufReader::with_capacity(CHUNK, stream);
    while !stop.is_cancelled() {
        let guard = Guard {
            stop: &stop,
            deadline: Instant::now() + CALLBACK_REQUEST_DEADLINE,
        };
        let mut budget = 16 * 1024;
        let request = line(&mut reader, &mut budget, &guard)?;
        if request.is_empty() {
            break;
        }
        let parts = request.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 3 {
            reply(reader.get_mut(), 400, "Bad Request")?;
            break;
        }
        let method = parts[0];
        let path = parts[1].split('?').next().unwrap_or("");
        let mut length = None;
        let mut bad = false;
        let mut close = false;
        loop {
            let h = line(&mut reader, &mut budget, &guard)?;
            if h == "\r\n" || h == "\n" {
                break;
            }
            let Some((key, value)) = h.split_once(':') else {
                bad = true;
                break;
            };
            if key.eq_ignore_ascii_case("content-length") {
                if length.is_some() {
                    bad = true;
                }
                length = value.trim().parse::<u64>().ok();
                if length.is_none() {
                    bad = true;
                }
            }
            if key.eq_ignore_ascii_case("transfer-encoding") {
                bad = true;
            }
            if key.eq_ignore_ascii_case("connection") && value.trim().eq_ignore_ascii_case("close")
            {
                close = true;
            }
        }
        // REW uses Content-Length, not chunked. Reject ambiguous framing and close.
        if bad {
            reply(reader.get_mut(), 400, "Bad Request")?;
            break;
        }
        let length = length.unwrap_or(0);
        // Routes include the server's token: an unknown path is refused before
        // its body is read. A small body is then discarded to keep the
        // connection; a larger one closes it.
        let handler = state.lock().unwrap().routes.get(path).cloned();
        let Some(handler) = handler else {
            reply(reader.get_mut(), 404, "Not Found")?;
            if close
                || length > MAX_CALLBACK_BODY as u64
                || !body(&mut reader, length, None, &guard)?
            {
                break;
            }
            continue;
        };
        if length > MAX_CALLBACK_BODY as u64 {
            if !body(&mut reader, length, None, &guard)? {
                break;
            }
            reply(reader.get_mut(), 413, "Payload Too Large")?;
            if close {
                break;
            } else {
                continue;
            }
        }
        let mut raw = Vec::new();
        if !body(&mut reader, length, Some(&mut raw), &guard)? {
            break;
        }
        if method == "GET" {
            reply(reader.get_mut(), 200, "{\"status\":\"ok\"}")?;
        } else if method == "POST" || method == "PUT" {
            let payload = if raw.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&raw)
                    .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&raw).into_owned()))
            };
            reply(reader.get_mut(), 200, "OK")?;
            {
                let mut s = state.lock().unwrap();
                if s.keep_history > 0 {
                    let logical = path
                        .split_once('/')
                        .and_then(|(_, p)| p.split_once('/'))
                        .map(|(_, p)| format!("/{p}"))
                        .unwrap_or_default();
                    s.history.push_back((logical, payload.clone()));
                    while s.history.len() > s.keep_history {
                        s.history.pop_front();
                    }
                }
            }
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(payload)));
        } else {
            reply(reader.get_mut(), 400, "Bad Request")?;
        }
        if close {
            break;
        }
    }
    Ok(())
}
