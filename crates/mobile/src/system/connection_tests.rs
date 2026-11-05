use super::*;
use crate::MobilePlatform;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

fn serve(status: u16, body: String) -> (Connection, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            socket.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        write!(socket, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        String::from_utf8(request).unwrap()
    });
    let connection = Connection::new(MobileConfig {
        platform: MobilePlatform::Android,
        server_url: format!("http://{address}/wd/hub/"),
        session_id: "session-1".into(),
    })
    .unwrap();
    (connection, worker)
}

#[test]
fn preserves_base_path_and_unwraps_w3c_value() {
    let (connection, worker) = serve(200, json!({"value": "xml"}).to_string());
    assert_eq!(
        connection
            .request("GET", "/source", None, Deadline::standard().unwrap())
            .unwrap(),
        "xml"
    );
    assert!(
        worker
            .join()
            .unwrap()
            .starts_with("GET /wd/hub/session/session-1/source HTTP/1.1")
    );
}

#[test]
fn rejected_element_action_is_safe_to_retry() {
    for (remote, code) in [
        ("stale element reference", ErrorCode::StaleRef),
        ("no such element", ErrorCode::StaleRef),
        ("unsupported operation", ErrorCode::ActionNotSupported),
    ] {
        let (connection, worker) = serve(
            404,
            json!({"value": {"error": remote, "message": "private UI text"}}).to_string(),
        );
        let error = connection
            .request(
                "POST",
                "/element/id/click",
                None,
                Deadline::standard().unwrap(),
            )
            .unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(error.disposition, DeliverySemantics::not_delivered());
        assert!(!error.message.contains("private UI text"));
        worker.join().unwrap();
    }
}

#[test]
fn malformed_mutation_response_never_promises_safe_retry() {
    for (status, body) in [(200, "not-json"), (200, "{}"), (500, "{\"value\":null}")] {
        let (connection, worker) = serve(status, body.into());
        let error = connection
            .request("POST", "/actions", None, Deadline::standard().unwrap())
            .unwrap_err();
        assert_eq!(error.disposition, DeliverySemantics::uncertain());
        worker.join().unwrap();
    }
}

#[test]
fn ambiguous_remote_failure_preserves_delivery_uncertainty() {
    let (connection, worker) = serve(500, json!({"value": {"error":"unknown error"}}).to_string());
    let error = connection
        .request("POST", "/actions", None, Deadline::standard().unwrap())
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(error.disposition, DeliverySemantics::uncertain());
    worker.join().unwrap();
}

#[test]
fn rejects_wrong_session_platform_before_reading_context() {
    let (connection, worker) = serve(200, json!({"value": {"platformName":"iOS"}}).to_string());
    assert_eq!(
        connection
            .validate_session(Deadline::standard().unwrap())
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgs
    );
    assert!(
        worker
            .join()
            .unwrap()
            .starts_with("GET /wd/hub/session/session-1 HTTP/1.1")
    );
}

#[test]
fn rejects_oversized_response_without_trusting_truncated_json() {
    let (connection, worker) = serve(200, " ".repeat(MAX_RESPONSE as usize + 1));
    let error = connection
        .request("GET", "/source", None, Deadline::standard().unwrap())
        .unwrap_err();
    assert!(error.message.contains("24 MiB"));
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    worker.join().unwrap();
}
