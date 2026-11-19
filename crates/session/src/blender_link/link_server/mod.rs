//! The link's listener on 127.0.0.1, answered by the game's main loop.
//!
//! `POST /` carries JSON-RPC 2.0 as `application/json`. `POST /model/<model
//! path>` carries an edited model as `model/gltf-binary`, replacing that model
//! while the game runs.
//!
//! Requests must use one of those types and carry no `Origin`, so web pages in
//! a browser cannot reach the link: their cross-origin requests of these types
//! need a preflight that the link never grants.

use std::{
    io::{BufRead, BufReader, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};

use serde_json::{json, Value};

/// The port the Blender add-on connects to.
pub(super) const BLENDER_LINK_PORT: u16 = 37_002;

const MAXIMUM_REQUEST_HEADER_BYTES: usize = 16 * 1024;
const MAXIMUM_JSON_REQUEST_BYTES: usize = 64 * 1024;
const MAXIMUM_MODEL_REQUEST_BYTES: usize = 256 * 1024 * 1024;
const REPLY_TIMEOUT: Duration = Duration::from_secs(60);
const MODEL_TARGET_PREFIX: &str = "/model/";

pub(super) struct LinkRequest {
    pub(super) method: String,
    pub(super) params: Value,
    /// The edited model of a `replace_model` request.
    pub(super) model: Vec<u8>,
    reply: Sender<Result<Value, String>>,
}

impl LinkRequest {
    pub(super) fn answer(self, result: Result<Value, String>) {
        // The client may have disconnected; nothing waits for the answer then.
        let _ = self.reply.send(result);
    }
}

pub(super) struct LinkServer {
    requests: Option<Mutex<Receiver<LinkRequest>>>,
    stopping: Arc<AtomicBool>,
    address: SocketAddr,
    listener_thread: Option<JoinHandle<()>>,
}

impl LinkServer {
    pub(super) fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, BLENDER_LINK_PORT))?;
        let address = listener.local_addr()?;
        let (sender, requests) = mpsc::channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let listener_stopping = Arc::clone(&stopping);
        let listener_thread = std::thread::Builder::new()
            .name("blender link".to_owned())
            .spawn(move || {
                for stream in listener.incoming() {
                    if listener_stopping.load(Ordering::Acquire) {
                        break;
                    }
                    let Ok(stream) = stream else {
                        continue;
                    };
                    // One connection at a time is enough for one editor.
                    let _ = answer_connection(stream, &sender);
                }
            })?;
        Ok(Self {
            requests: Some(Mutex::new(requests)),
            stopping,
            address,
            listener_thread: Some(listener_thread),
        })
    }

    pub(super) fn try_next_request(&self) -> Option<LinkRequest> {
        self.requests.as_ref()?.lock().ok()?.try_recv().ok()
    }
}

impl Drop for LinkServer {
    fn drop(&mut self) {
        // Dropping the queue releases a connection waiting on an answer.
        self.requests = None;
        self.stopping.store(true, Ordering::Release);
        // Wakes the listener so it sees that it is stopping.
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_secs(1));
        if let Some(thread) = self.listener_thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum HttpRequest {
    JsonRpc(Vec<u8>),
    Model { model: String, glb: Vec<u8> },
}

fn answer_connection(stream: TcpStream, requests: &Sender<LinkRequest>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let response = match read_http_request(&mut reader) {
        Ok(HttpRequest::JsonRpc(body)) => answer_json_rpc(&body, requests),
        Ok(HttpRequest::Model { model, glb }) => forward(
            requests,
            &Value::Null,
            "replace_model",
            json!({ "model": model }),
            glb,
        ),
        Err(status) => return write_http_response(stream, status, ""),
    };
    write_http_response(stream, "200 OK", &response.to_string())
}

/// Reads one HTTP POST, returning it or the status to refuse it with.
fn read_http_request(reader: &mut impl BufRead) -> Result<HttpRequest, &'static str> {
    let mut header_bytes = 0;
    let mut line = String::new();
    let mut read_line = |line: &mut String| -> Result<(), &'static str> {
        line.clear();
        let count = reader.read_line(line).map_err(|_| "400 Bad Request")?;
        header_bytes += count;
        if count == 0 || header_bytes > MAXIMUM_REQUEST_HEADER_BYTES {
            return Err("400 Bad Request");
        }
        Ok(())
    };
    read_line(&mut line)?;
    let Some(target) = line
        .strip_prefix("POST ")
        .and_then(|rest| rest.split_whitespace().next())
        .map(str::to_owned)
    else {
        return Err("405 Method Not Allowed");
    };
    let model = match target.as_str() {
        "/" => None,
        _ => Some(
            target
                .strip_prefix(MODEL_TARGET_PREFIX)
                .and_then(percent_decode)
                .filter(|model| !model.is_empty())
                .ok_or("404 Not Found")?,
        ),
    };
    let expected_type = if model.is_some() {
        "model/gltf-binary"
    } else {
        "application/json"
    };
    let mut content_length = None;
    let mut has_expected_type = false;
    loop {
        read_line(&mut line)?;
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else {
            return Err("400 Bad Request");
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => content_length = value.parse::<usize>().ok(),
            "content-type" => {
                has_expected_type = value
                    .split(';')
                    .next()
                    .is_some_and(|media_type| media_type.trim() == expected_type);
            }
            "origin" => return Err("403 Forbidden"),
            _ => {}
        }
    }
    if !has_expected_type {
        return Err("415 Unsupported Media Type");
    }
    let Some(content_length) = content_length else {
        return Err("411 Length Required");
    };
    let maximum = if model.is_some() {
        MAXIMUM_MODEL_REQUEST_BYTES
    } else {
        MAXIMUM_JSON_REQUEST_BYTES
    };
    if content_length > maximum {
        return Err("413 Content Too Large");
    }
    let mut body = vec![0; content_length];
    reader
        .read_exact(&mut body)
        .map_err(|_| "400 Bad Request")?;
    Ok(match model {
        Some(model) => HttpRequest::Model { model, glb: body },
        None => HttpRequest::JsonRpc(body),
    })
}

fn percent_decode(encoded: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut rest = encoded.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        if byte == b'%' {
            let digits = std::str::from_utf8(tail.get(..2)?).ok()?;
            bytes.push(u8::from_str_radix(digits, 16).ok()?);
            rest = &tail[2..];
        } else {
            bytes.push(byte);
            rest = tail;
        }
    }
    String::from_utf8(bytes).ok()
}

fn answer_json_rpc(body: &[u8], requests: &Sender<LinkRequest>) -> Value {
    let Ok(request) = serde_json::from_slice::<Value>(body) else {
        return json_rpc_error(&Value::Null, -32_700, "the request is not JSON");
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return json_rpc_error(&id, -32_600, "the request names no method");
    };
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    forward(requests, &id, method, params, Vec::new())
}

/// Hands a request to the game's main loop and waits for its answer.
fn forward(
    requests: &Sender<LinkRequest>,
    id: &Value,
    method: &str,
    params: Value,
    model: Vec<u8>,
) -> Value {
    let (reply, answer) = mpsc::channel();
    if requests
        .send(LinkRequest {
            method: method.to_owned(),
            params,
            model,
            reply,
        })
        .is_err()
    {
        return json_rpc_error(id, -32_000, "the Blender link stopped");
    }
    match answer.recv_timeout(REPLY_TIMEOUT) {
        Ok(Ok(result)) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Ok(Err(message)) => json_rpc_error(id, -32_000, &message),
        Err(_) => json_rpc_error(id, -32_000, "the game did not answer"),
    }
}

fn json_rpc_error(id: &Value, code: i32, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn write_http_response(mut stream: TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(request: &str) -> Result<HttpRequest, &'static str> {
        read_http_request(&mut request.as_bytes())
    }

    #[test]
    fn accepts_json_and_models_and_refuses_browser_requests() {
        assert_eq!(
            read(
                "POST / HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}"
            ),
            Ok(HttpRequest::JsonRpc(b"{}".to_vec()))
        );
        assert_eq!(
            read("POST /model/ui/a%20b.bfb HTTP/1.1\r\nContent-Type: model/gltf-binary\r\nContent-Length: 4\r\n\r\nglTF"),
            Ok(HttpRequest::Model {
                model: "ui/a b.bfb".to_owned(),
                glb: b"glTF".to_vec()
            })
        );
        assert_eq!(
            read("POST / HTTP/1.1\r\nOrigin: https://example.com\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}"),
            Err("403 Forbidden")
        );
        assert_eq!(
            read("POST / HTTP/1.1\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\n{}"),
            Err("415 Unsupported Media Type")
        );
        assert_eq!(
            read("POST /model/a.bfb HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}"),
            Err("415 Unsupported Media Type")
        );
        assert_eq!(
            read("GET / HTTP/1.1\r\n\r\n"),
            Err("405 Method Not Allowed")
        );
    }
}
