#![forbid(unsafe_code)]
use secs_rew::*;
use serde_json::json;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};
#[test]
fn test_http_transport_sends_json_and_query_and_preserves_error_status() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        for (expected, body, status, response) in [
            (
                "GET /measurements/abc/impulse-response?normalised=false HTTP/1.1",
                None,
                200,
                "{\"sampleRate\":48000.0}",
            ),
            (
                "POST /measure/command HTTP/1.1",
                Some(json!({"command":"SPL"})),
                403,
                "Pro required",
            ),
        ] {
            let (stream, _) = listener.accept().unwrap();
            let mut r = BufReader::new(stream);
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            assert_eq!(line.trim(), expected);
            let mut length = 0;
            loop {
                line.clear();
                r.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                let (key, value) = line.split_once(':').unwrap();
                if key.eq_ignore_ascii_case("content-length") {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut raw = vec![0; length];
            r.read_exact(&mut raw).unwrap();
            if let Some(body) = body {
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&raw).unwrap(),
                    body
                );
            } else {
                assert!(raw.is_empty());
            }
            write!(r.get_mut(),"HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
        }
    });
    let http = HttpTransport::new(&base).unwrap();
    let mut request = Request::new(
        "GET",
        "/measurements/abc/impulse-response",
        None,
        READ_TIMEOUT,
    );
    request.query = vec![("normalised".into(), "false".into())];
    assert_eq!(
        http.request(&request).unwrap().body,
        json!({"sampleRate":48000.0})
    );
    let result = http
        .request(&Request::new(
            "POST",
            "/measure/command",
            Some(json!({"command":"SPL"})),
            MEASURE_TIMEOUT,
        ))
        .unwrap();
    assert_eq!(result.status, 403);
    assert_eq!(result.body, json!("Pro required"));
    server.join().unwrap();
    assert!(HttpTransport::new("https://127.0.0.1:4735").is_err());
}
#[test]
fn test_http_transport_read_timeout_is_enforced() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (_stream, _) = listener.accept().unwrap();
        thread::sleep(Duration::from_millis(150));
    });
    let result = HttpTransport::new(&base).unwrap().request(&Request::new(
        "GET",
        "/version",
        None,
        Duration::from_millis(20),
    ));
    assert!(matches!(result, Err(RewError::Connection { .. })));
    server.join().unwrap();
}
