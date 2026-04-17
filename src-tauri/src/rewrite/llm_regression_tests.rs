use std::future::Future;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::json;

use crate::models::{AppSettings, DocumentFormat};
use crate::rewrite_unit::{
    parse_rewrite_batch_response, parse_rewrite_unit_response, RewriteBatchRequest,
    RewriteUnitRequest, RewriteUnitSlot, WritebackSlotRole,
};

use super::llm::{
    build_client, rewrite_batch_with_client, rewrite_selection_text_with_client,
    rewrite_unit_with_client,
};

#[test]
fn parse_rewrite_batch_response_rejects_mismatched_batch_id() {
    let request = RewriteBatchRequest::new(
        "batch-1",
        "docx",
        vec![RewriteUnitRequest::new(
            "unit-1",
            "docx",
            vec![RewriteUnitSlot::editable("slot-1", "甲")],
        )],
    );

    let error = parse_rewrite_batch_response(
        &request,
        r#"{"batchId":"batch-x","results":[{"rewriteUnitId":"unit-1","updates":[{"slotId":"slot-1","text":"乙"}]}]}"#,
    )
    .expect_err("expected invalid batch id");

    assert!(error.contains("batchId"));
}

#[test]
fn parse_rewrite_unit_response_rejects_unknown_slot_id() {
    let request = RewriteUnitRequest::new(
        "unit-1",
        "docx",
        vec![RewriteUnitSlot::editable("slot-1", "甲")],
    );

    let error = parse_rewrite_unit_response(
        &request,
        r#"{"rewriteUnitId":"unit-1","updates":[{"slotId":"slot-x","text":"乙"}]}"#,
    )
    .expect_err("expected validation error");

    assert!(error.contains("未知 slot_id"));
}

#[test]
fn rewrite_unit_with_client_returns_structured_updates() {
    let server = TestServer::start(vec![json_http_response(
        r#"{"rewriteUnitId":"unit-1","updates":[{"slotId":"slot-1","text":"改写后正文"}]}"#,
    )]);
    let settings = test_settings(&server.base_url);
    let client = build_client(&settings).unwrap();
    let request = RewriteUnitRequest::new(
        "unit-1",
        "docx",
        vec![
            RewriteUnitSlot::editable("slot-1", "原正文"),
            RewriteUnitSlot::locked("slot-locked", "[公式]", WritebackSlotRole::LockedText),
        ],
    );

    let result = run_async(rewrite_unit_with_client(&client, &settings, &request))
        .expect("unit rewrite should succeed");

    assert_eq!(result.rewrite_unit_id, "unit-1");
    assert_eq!(result.updates.len(), 1);
    assert_eq!(result.updates[0].slot_id, "slot-1");
    assert_eq!(result.updates[0].text, "改写后正文");
    assert_eq!(server.request_count(), 1);
}

#[test]
fn rewrite_batch_with_client_sends_single_http_request() {
    let server = TestServer::start(vec![json_http_response(
        r#"{"batchId":"batch-1","results":[{"rewriteUnitId":"unit-1","updates":[{"slotId":"slot-1","text":"改写1"}]},{"rewriteUnitId":"unit-2","updates":[{"slotId":"slot-2","text":"改写2"}]}]}"#,
    )]);
    let settings = test_settings(&server.base_url);
    let client = build_client(&settings).unwrap();
    let request = RewriteBatchRequest::new(
        "batch-1",
        "docx",
        vec![
            RewriteUnitRequest::new(
                "unit-1",
                "docx",
                vec![RewriteUnitSlot::editable("slot-1", "甲")],
            ),
            RewriteUnitRequest::new(
                "unit-2",
                "docx",
                vec![RewriteUnitSlot::editable("slot-2", "乙")],
            ),
        ],
    );

    let result = run_async(rewrite_batch_with_client(&client, &settings, &request))
        .expect("batch rewrite should succeed");

    assert_eq!(result.results.len(), 2);
    assert_eq!(server.request_count(), 1);
}

#[test]
fn rewrite_unit_builds_client_and_parses_structured_response() {
    let server = TestServer::start(vec![json_http_response(
        r#"{"rewriteUnitId":"unit-2","updates":[{"slotId":"slot-2","text":"第二段改写"}]}"#,
    )]);
    let settings = test_settings(&server.base_url);
    let client = build_client(&settings).unwrap();
    let request = RewriteUnitRequest::new(
        "unit-2",
        "markdown",
        vec![RewriteUnitSlot::editable("slot-2", "第二段原文")],
    );

    let result = run_async(rewrite_unit_with_client(&client, &settings, &request))
        .expect("unit rewrite should succeed");

    assert_eq!(result.rewrite_unit_id, "unit-2");
    assert_eq!(result.updates.len(), 1);
    assert_eq!(result.updates[0].slot_id, "slot-2");
    assert_eq!(result.updates[0].text, "第二段改写");
    assert_eq!(server.request_count(), 1);
}

#[test]
fn plain_single_unit_does_not_retry_after_validation_failure() {
    let server = TestServer::start(vec![
        json_http_response(
            r#"{"rewriteUnitId":"selection","updates":[{"slotId":"slot-0","text":"I am Claude."}]}"#,
        ),
        json_http_response(
            r#"{"rewriteUnitId":"selection","updates":[{"slotId":"slot-0","text":"自然改写后的正文。"}]}"#,
        ),
    ]);
    let settings = test_settings(&server.base_url);
    let client = build_client(&settings).unwrap();

    let result = run_async(rewrite_selection_text_with_client(
        &client,
        &settings,
        "这是一段正文。",
        DocumentFormat::PlainText,
        false,
    ));

    assert!(result.is_err());
    assert_eq!(server.request_count(), 1);
}

#[test]
fn plain_multiline_unit_does_not_fallback_per_line() {
    let server = TestServer::start(vec![json_http_response(
        r#"{"rewriteUnitId":"selection","updates":[{"slotId":"slot-0","text":"@@@1@@@第一行改写\n@@@2@@@不该出现的内容\n@@@3@@@第二行改写"}]}"#,
    )]);
    let settings = test_settings(&server.base_url);
    let client = build_client(&settings).unwrap();

    let result = run_async(rewrite_selection_text_with_client(
        &client,
        &settings,
        "第一行\n\n第二行",
        DocumentFormat::PlainText,
        false,
    ));

    assert!(result.is_err());
    assert_eq!(server.request_count(), 1);
}

#[test]
fn transport_does_not_retry_with_stream_after_stream_required_error() {
    let server = TestServer::start(vec![
        http_response(
            "400 Bad Request",
            "application/json; charset=utf-8",
            r#"{"error":{"message":"Stream must be set to true","param":"stream"}}"#,
        ),
        http_response(
            "200 OK",
            "text/event-stream; charset=utf-8",
            "data: {\"choices\":[{\"delta\":{\"content\":\"改写成功\"}}]}\n\ndata: [DONE]\n",
        ),
    ]);
    let settings = test_settings(&server.base_url);
    let client = build_client(&settings).unwrap();

    let result = run_async(rewrite_selection_text_with_client(
        &client,
        &settings,
        "需要改写的正文。",
        DocumentFormat::PlainText,
        false,
    ));

    assert!(result.is_err());
    assert_eq!(server.request_count(), 1);
}

fn run_async<F>(future: F) -> F::Output
where
    F: Future,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

fn test_settings(base_url: &str) -> AppSettings {
    AppSettings {
        base_url: base_url.to_string(),
        api_key: "test-key".to_string(),
        model: "test-model".to_string(),
        ..AppSettings::default()
    }
}

fn json_http_response(content: &str) -> String {
    http_response(
        "200 OK",
        "application/json; charset=utf-8",
        &json!({
            "choices": [
                {
                    "message": {
                        "content": content
                    }
                }
            ]
        })
        .to_string(),
    )
}

fn http_response(status: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.as_bytes().len()
    )
}

struct TestServer {
    addr: SocketAddr,
    base_url: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl TestServer {
    fn start(responses: Vec<String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_requests = Arc::clone(&requests);
        let thread_stop = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            let mut next = 0usize;

            loop {
                if thread_stop.load(Ordering::SeqCst) {
                    break;
                }

                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let request = read_http_request(&mut stream).unwrap_or_default();
                        thread_requests.lock().unwrap().push(request);

                        let response = responses.get(next).cloned().unwrap_or_else(|| {
                            http_response(
                                "500 Internal Server Error",
                                "text/plain; charset=utf-8",
                                "unexpected request",
                            )
                        });
                        next = next.saturating_add(1);

                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.flush();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            addr,
            base_url: format!("http://{addr}"),
            requests,
            stop,
            handle: Some(handle),
        }
    }

    fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn read_http_request(stream: &mut TcpStream) -> std::io::Result<String> {
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];

    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                if request_complete(&buffer) {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(error) => return Err(error),
        }
    }

    Ok(String::from_utf8_lossy(&buffer).to_string())
}

fn request_complete(buffer: &[u8]) -> bool {
    let Some(header_end) = find_header_end(buffer) else {
        return false;
    };
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let body_len = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if !name.eq_ignore_ascii_case("content-length") {
                return None;
            }
            value.trim().parse::<usize>().ok()
        })
        .unwrap_or(0);

    buffer.len() >= header_end + 4 + body_len
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}
