#![forbid(unsafe_code)]
use secs_rew::*;
use serde_json::json;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    sync::{Arc, mpsc},
    time::Duration,
};
fn path(url: &str) -> &str {
    url[url.find("127.0.0.1:").unwrap() + 10..]
        .split_once('/')
        .unwrap()
        .1
}
fn send(stream: &mut TcpStream, method: &str, path: &str, body: &[u8]) {
    write!(
        stream,
        "{method} /{} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
        path.trim_start_matches('/'),
        body.len()
    )
    .unwrap();
    stream.write_all(body).unwrap();
    stream.flush().unwrap();
}
fn response(reader: &mut BufReader<TcpStream>) -> (u16, String) {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let code = line.split_whitespace().nth(1).unwrap().parse().unwrap();
    let mut length = 0;
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" {
            break;
        }
        if let Some(n) = line.strip_prefix("Content-Length: ") {
            length = n.trim().parse().unwrap();
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    (code, String::from_utf8(body).unwrap())
}
fn connect(s: &CallbackServer) -> BufReader<TcpStream> {
    let stream = TcpStream::connect(("127.0.0.1", s.port())).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    BufReader::new(stream)
}
#[test]
fn test_server_starts_on_free_port_and_builds_urls() {
    let mut s = CallbackServer::default();
    s.start().unwrap();
    assert!(s.is_running());
    assert!(s.port() > 0);
    assert_eq!(s.url_for("levels"), s.url_for("/levels"));
    let p = s.port();
    s.start().unwrap();
    assert_eq!(s.port(), p);
    s.stop();
    assert!(!s.is_running());
}
#[test]
fn test_route_receives_json_body() {
    let mut s = CallbackServer::new(10);
    s.start().unwrap();
    let (tx, rx) = mpsc::channel();
    let url = s.route("/levels", Arc::new(move |v| tx.send(v).unwrap()));
    let mut r = connect(&s);
    send(
        r.get_mut(),
        "POST",
        path(&url),
        br#"{"rms":-21.5,"peak":-6.0}"#,
    );
    assert_eq!(response(&mut r), (200, "OK".into()));
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        json!({"rms":-21.5,"peak":-6.0})
    );
    assert_eq!(s.history()[0].0, "/levels");
    drop(r);
}
#[test]
fn test_query_string_and_unknown_path() {
    let mut s = CallbackServer::default();
    s.start().unwrap();
    let (tx, rx) = mpsc::channel();
    let url = s.route("/levels", Arc::new(move |v| tx.send(v).unwrap()));
    let mut r = connect(&s);
    send(
        r.get_mut(),
        "PUT",
        &format!("{}?unit=dBFS", path(&url)),
        b"1",
    );
    assert_eq!(response(&mut r).0, 200);
    assert_eq!(rx.recv_timeout(Duration::from_secs(1)).unwrap(), json!(1));
    send(r.get_mut(), "POST", "unknown-path", b"1");
    assert_eq!(response(&mut r).0, 404);
    assert!(rx.try_recv().is_err());
    drop(r);
}
#[test]
fn test_handler_exception_does_not_kill_server() {
    let mut s = CallbackServer::default();
    s.start().unwrap();
    let url = s.route("boom", Arc::new(|_| panic!("handler failed")));
    let mut r = connect(&s);
    for _ in 0..2 {
        send(r.get_mut(), "POST", path(&url), b"{}");
        assert_eq!(response(&mut r).0, 200);
    }
    drop(r);
}
#[test]
fn test_history_and_get_health() {
    let mut s = CallbackServer::new(1);
    s.start().unwrap();
    let (tx, rx) = mpsc::channel();
    let url = s.route("levels", Arc::new(move |v| tx.send(v).unwrap()));
    let mut r = connect(&s);
    for n in [1, 2] {
        send(r.get_mut(), "POST", path(&url), n.to_string().as_bytes());
        assert_eq!(response(&mut r).0, 200);
        rx.recv_timeout(Duration::from_secs(1)).unwrap();
    }
    assert_eq!(s.history(), vec![("/levels".into(), json!(2))]);
    send(r.get_mut(), "GET", path(&url), b"");
    assert_eq!(response(&mut r), (200, "{\"status\":\"ok\"}".into()));
    drop(r);
}
#[test]
fn test_stop_closes_port_and_drop_closes_port() {
    let port = {
        let mut s = CallbackServer::default();
        s.start().unwrap();
        let p = s.port();
        s.stop();
        assert!(TcpStream::connect(("127.0.0.1", p)).is_err());
        s.start().unwrap();
        s.port()
    };
    assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
}
#[test]
fn test_r3_random_token_wrong_path_404_and_oversized_body_drained() {
    let mut a = CallbackServer::default();
    let mut b = CallbackServer::default();
    a.start().unwrap();
    b.start().unwrap();
    let (tx, rx) = mpsc::channel();
    let url = a.route("levels", Arc::new(move |v| tx.send(v).unwrap()));
    assert_ne!(path(&url), path(&b.url_for("levels")));
    assert_eq!(path(&url).split('/').next().unwrap().len(), 32);
    let mut r = connect(&a);
    send(r.get_mut(), "POST", "levels", b"{}");
    assert_eq!(response(&mut r).0, 404);
    let big = vec![b'x'; MAX_CALLBACK_BODY + 8192];
    send(r.get_mut(), "POST", path(&url), &big);
    assert_eq!(response(&mut r).0, 413);
    send(r.get_mut(), "POST", path(&url), b"42");
    assert_eq!(response(&mut r).0, 200);
    assert_eq!(rx.recv_timeout(Duration::from_secs(1)).unwrap(), json!(42));
    assert!(rx.try_recv().is_err());
    drop(r);
}
#[test]
fn test_reply_is_sent_before_dispatch() {
    let mut s = CallbackServer::default();
    s.start().unwrap();
    let (tx, rx) = mpsc::channel::<()>();
    let rx = std::sync::Mutex::new(rx);
    let url = s.route(
        "levels",
        Arc::new(move |_| {
            rx.lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
        }),
    );
    let mut r = connect(&s);
    send(r.get_mut(), "POST", path(&url), b"{}");
    assert_eq!(response(&mut r).0, 200);
    tx.send(()).unwrap();
    drop(r);
}
#[test]
fn test_level_monitor_subscribes_with_callback_url_and_unsubscribes_on_close() {
    let t = RecordedTransport::new([]);
    let mut c = RewClient::new(t.clone());
    let url = c.callback_url("levels").unwrap();
    let progress = c.callback_url("measure-progress").unwrap();
    let post = |p: &str, b| {
        RecordedExchange::new(
            Request::new("POST", p, Some(b), READ_TIMEOUT),
            json!({"message":"ok"}),
        )
    };
    for e in [
        post(
            "/input-levels/subscribe",
            json!({"url":url,"parameters":{"unit":"dBFS","timeSpanSeconds":0.25}}),
        ),
        post("/input-levels/command", json!({"command":"Start"})),
        post(
            "/measure/subscribe",
            json!({"url":progress,"parameters":{}}),
        ),
        post("/generator/command", json!({"command":"Stop"})),
        post("/measure/unsubscribe", json!({"url":progress})),
        post("/input-levels/command", json!({"command":"Stop"})),
        post("/input-levels/unsubscribe", json!({"url":url})),
    ] {
        t.push(e);
    }
    let (tx, rx) = mpsc::channel();
    c.start(Arc::new(move |l| tx.send(l).unwrap())).unwrap();
    c.subscribe_progress(Arc::new(|_| {})).unwrap();
    let address = url
        .strip_prefix("http://")
        .unwrap()
        .split('/')
        .next()
        .unwrap();
    let stream = TcpStream::connect(address).unwrap();
    let mut r = BufReader::new(stream);
    send(
        r.get_mut(),
        "POST",
        path(&url),
        br#"{"unit":"dBFS","rms":[-22.0],"peak":[-4.0]}"#,
    );
    assert_eq!(response(&mut r).0, 200);
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        Level {
            input_dbfs: -22.0,
            peak_dbfs: -4.0,
            spl_db: None,
            clip: false
        }
    );
    drop(r);
    c.close();
    t.assert_finished();
}
