//! Session runtime for stdio-backed JSON-RPC servers such as `rust-analyzer`.

use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// JSON-RPC request identifier.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum JsonRpcId {
    /// Numeric request identifier.
    Integer(i64),
    /// String request identifier.
    String(String),
}

impl JsonRpcId {
    fn from_outgoing(id: u64) -> Result<Self, SessionError> {
        let id =
            i64::try_from(id).map_err(|_| SessionError::Protocol("request id overflow".into()))?;
        Ok(Self::Integer(id))
    }
}

/// JSON-RPC error returned by a remote server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseError {
    /// Protocol-defined numeric error code.
    pub code: i64,
    /// Human-readable error message.
    pub message: String,
    /// Optional structured error payload.
    pub data: Option<Value>,
}

/// A server-originated request surfaced to higher layers.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerRequest {
    /// Server-generated request id.
    pub id: JsonRpcId,
    /// Requested method name.
    pub method: String,
    /// Optional request parameters.
    pub params: Option<Value>,
}

/// Transport-level events emitted by a running session.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    /// A server notification.
    Notification {
        /// Notification method name.
        method: String,
        /// Optional notification params.
        params: Option<Value>,
    },
    /// A normalized progress notification extracted from `$/progress`.
    Progress {
        /// Progress token.
        token: Value,
        /// Progress payload.
        value: Value,
    },
    /// A server-originated request that higher layers may answer.
    ServerRequest(ServerRequest),
    /// A line observed on the child process stderr stream.
    Stderr(String),
}

/// Errors produced by session setup, transport, or JSON-RPC handling.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The child process could not be started.
    #[error("failed to spawn process: {0}")]
    Spawn(#[source] io::Error),
    /// A required stdio pipe was not available on the spawned process.
    #[error("child process missing {0} pipe")]
    MissingPipe(&'static str),
    /// Transport I/O failed while reading or writing frames.
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    /// A frame body could not be decoded as JSON.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// The peer sent an invalid or unsupported JSON-RPC payload.
    #[error("json-rpc protocol error: {0}")]
    Protocol(String),
    /// The peer answered a request with a JSON-RPC error object.
    #[error("json-rpc request failed: {0:?}")]
    ServerError(ResponseError),
    /// The child process or transport disconnected before completion.
    #[error("session disconnected")]
    Disconnected,
}

impl From<RecvError> for SessionError {
    fn from(_: RecvError) -> Self {
        Self::Disconnected
    }
}

/// Builder for a stdio-backed JSON-RPC session.
#[derive(Default)]
pub struct SessionBuilder {
    program: OsString,
    args: Vec<OsString>,
    current_dir: Option<PathBuf>,
    envs: Vec<(OsString, OsString)>,
}

impl SessionBuilder {
    /// Creates a builder for the given program.
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            program: program.as_ref().to_os_string(),
            ..Self::default()
        }
    }

    /// Adds a single command-line argument.
    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    /// Adds multiple command-line arguments.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|arg| arg.as_ref().to_os_string()));
        self
    }

    /// Sets the child process working directory.
    pub fn current_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.current_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Adds an environment variable for the child process.
    pub fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.envs
            .push((key.as_ref().to_os_string(), value.as_ref().to_os_string()));
        self
    }

    /// Spawns the configured child process and starts the session runtime.
    pub fn spawn(self) -> Result<Session, SessionError> {
        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(current_dir) = self.current_dir {
            command.current_dir(current_dir);
        }

        for (key, value) in self.envs {
            command.env(key, value);
        }

        let mut child = command.spawn().map_err(SessionError::Spawn)?;
        let stdin = child
            .stdin
            .take()
            .ok_or(SessionError::MissingPipe("stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(SessionError::MissingPipe("stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(SessionError::MissingPipe("stderr"))?;

        let pending = Arc::new(Mutex::new(HashMap::new()));
        let (event_tx, event_rx) = mpsc::channel();
        let terminated = Arc::new(AtomicBool::new(false));
        let reader_handle = spawn_stdout_thread(
            stdout,
            Arc::clone(&pending),
            event_tx.clone(),
            Arc::clone(&terminated),
        );
        let stderr_handle = spawn_stderr_thread(stderr, event_tx);

        Ok(Session {
            writer: Mutex::new(BufWriter::new(stdin)),
            child: Mutex::new(child),
            pending,
            next_id: AtomicU64::new(1),
            event_rx: Mutex::new(Some(event_rx)),
            reader_handle: Mutex::new(Some(reader_handle)),
            stderr_handle: Mutex::new(Some(stderr_handle)),
            terminated,
            shutdown_sent: AtomicBool::new(false),
        })
    }
}

type PendingMap = Arc<Mutex<HashMap<u64, Sender<Result<Value, SessionError>>>>>;

/// Reusable transport session around a stdio-speaking JSON-RPC server.
pub struct Session {
    writer: Mutex<BufWriter<std::process::ChildStdin>>,
    child: Mutex<Child>,
    pending: PendingMap,
    next_id: AtomicU64,
    event_rx: Mutex<Option<Receiver<SessionEvent>>>,
    reader_handle: Mutex<Option<JoinHandle<()>>>,
    stderr_handle: Mutex<Option<JoinHandle<()>>>,
    terminated: Arc<AtomicBool>,
    shutdown_sent: AtomicBool,
}

impl Session {
    /// Takes ownership of the event receiver, if it has not already been taken.
    pub fn take_event_receiver(&self) -> Option<Receiver<SessionEvent>> {
        self.event_rx
            .lock()
            .expect("event receiver poisoned")
            .take()
    }

    /// Sends a JSON-RPC request and waits for the correlated response.
    pub fn request<P>(&self, method: &str, params: P) -> Result<Value, SessionError>
    where
        P: Serialize,
    {
        let request_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let jsonrpc_id = JsonRpcId::from_outgoing(request_id)?;
        let params = serde_json::to_value(params)?;
        let message = json!({
            "jsonrpc": "2.0",
            "id": jsonrpc_id,
            "method": method,
            "params": params,
        });

        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .expect("pending map poisoned")
            .insert(request_id, tx);

        if let Err(error) = self.write_message(&message) {
            self.pending
                .lock()
                .expect("pending map poisoned")
                .remove(&request_id);
            return Err(error);
        }

        rx.recv()?
    }

    /// Sends a JSON-RPC notification.
    pub fn notify<P>(&self, method: &str, params: P) -> Result<(), SessionError>
    where
        P: Serialize,
    {
        let params = serde_json::to_value(params)?;
        let message = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_message(&message)
    }

    /// Sends a JSON-RPC success response to a server-originated request.
    pub fn respond<R>(&self, id: JsonRpcId, result: R) -> Result<(), SessionError>
    where
        R: Serialize,
    {
        let result = serde_json::to_value(result)?;
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        });
        self.write_message(&message)
    }

    /// Sends a JSON-RPC error response to a server-originated request.
    pub fn respond_error(
        &self,
        id: JsonRpcId,
        code: i64,
        message: impl Into<String>,
        data: Option<Value>,
    ) -> Result<(), SessionError> {
        let mut error = json!({
            "code": code,
            "message": message.into(),
        });

        if let Some(data) = data {
            error["data"] = data;
        }

        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": error,
        });
        self.write_message(&message)
    }

    /// Requests cancellation for an in-flight request.
    pub fn cancel_request(&self, id: JsonRpcId) -> Result<(), SessionError> {
        self.notify("$/cancelRequest", json!({ "id": id }))
    }

    /// Performs the standard `shutdown` request followed by the `exit` notification.
    pub fn shutdown(&self) -> Result<(), SessionError> {
        if self.shutdown_sent.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        self.request("shutdown", Value::Null)?;
        self.notify("exit", Value::Null)?;
        self.finish_process()
    }

    fn write_message(&self, message: &Value) -> Result<(), SessionError> {
        if self.terminated.load(Ordering::SeqCst) {
            return Err(SessionError::Disconnected);
        }

        let body = serde_json::to_vec(message)?;
        let mut writer = self.writer.lock().expect("writer poisoned");
        write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
        writer.write_all(&body)?;
        writer.flush()?;
        Ok(())
    }

    fn finish_process(&self) -> Result<(), SessionError> {
        self.terminated.store(true, Ordering::SeqCst);
        close_pending(&self.pending, SessionError::Disconnected);

        let status = self.child.lock().expect("child poisoned").wait()?;
        if let Some(handle) = self
            .reader_handle
            .lock()
            .expect("reader handle poisoned")
            .take()
        {
            let _ = handle.join();
        }
        if let Some(handle) = self
            .stderr_handle
            .lock()
            .expect("stderr handle poisoned")
            .take()
        {
            let _ = handle.join();
        }

        if status.success() {
            Ok(())
        } else {
            Err(SessionError::Protocol(format!(
                "child process exited with status {status}"
            )))
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.shutdown_sent.load(Ordering::SeqCst) {
            return;
        }

        self.terminated.store(true, Ordering::SeqCst);
        close_pending(&self.pending, SessionError::Disconnected);

        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }

        if let Ok(mut handle) = self.reader_handle.lock() {
            if let Some(handle) = handle.take() {
                let _ = handle.join();
            }
        }

        if let Ok(mut handle) = self.stderr_handle.lock() {
            if let Some(handle) = handle.take() {
                let _ = handle.join();
            }
        }
    }
}

fn spawn_stdout_thread(
    stdout: ChildStdout,
    pending: PendingMap,
    event_tx: Sender<SessionEvent>,
    terminated: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);

        loop {
            match read_message(&mut reader) {
                Ok(Some(message)) => {
                    if let Err(error) = handle_incoming_message(message, &pending, &event_tx) {
                        close_pending(&pending, error);
                        terminated.store(true, Ordering::SeqCst);
                        break;
                    }
                }
                Ok(None) => {
                    close_pending(&pending, SessionError::Disconnected);
                    terminated.store(true, Ordering::SeqCst);
                    break;
                }
                Err(error) => {
                    close_pending(&pending, error);
                    terminated.store(true, Ordering::SeqCst);
                    break;
                }
            }
        }
    })
}

fn spawn_stderr_thread(stderr: ChildStderr, event_tx: Sender<SessionEvent>) -> JoinHandle<()> {
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if event_tx.send(SessionEvent::Stderr(line)).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    })
}

fn close_pending(pending: &PendingMap, error: SessionError) {
    let waiters = pending
        .lock()
        .expect("pending map poisoned")
        .drain()
        .map(|(_, sender)| sender)
        .collect::<Vec<_>>();

    for sender in waiters {
        let _ = sender.send(Err(match &error {
            SessionError::Spawn(inner) => SessionError::Protocol(inner.to_string()),
            SessionError::MissingPipe(pipe) => SessionError::Protocol(pipe.to_string()),
            SessionError::Io(inner) => {
                SessionError::Io(io::Error::new(inner.kind(), inner.to_string()))
            }
            SessionError::Json(inner) => SessionError::Protocol(inner.to_string()),
            SessionError::Protocol(message) => SessionError::Protocol(message.clone()),
            SessionError::ServerError(server) => SessionError::ServerError(server.clone()),
            SessionError::Disconnected => SessionError::Disconnected,
        }));
    }
}

fn handle_incoming_message(
    message: Value,
    pending: &PendingMap,
    event_tx: &Sender<SessionEvent>,
) -> Result<(), SessionError> {
    let object = message
        .as_object()
        .ok_or_else(|| SessionError::Protocol("incoming message must be a JSON object".into()))?;

    let method = object
        .get("method")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let id = object.get("id").cloned();

    match (method, id) {
        (Some(method), Some(id)) => {
            let id = parse_jsonrpc_id(&id)?;
            let request = ServerRequest {
                id,
                method,
                params: object.get("params").cloned(),
            };
            event_tx
                .send(SessionEvent::ServerRequest(request))
                .map_err(|_| SessionError::Disconnected)
        }
        (Some(method), None) => {
            let params = object.get("params").cloned();
            if method == "$/progress" {
                let params_value = params.clone().ok_or_else(|| {
                    SessionError::Protocol("progress notification missing params".into())
                })?;
                let token = params_value.get("token").cloned().ok_or_else(|| {
                    SessionError::Protocol("progress notification missing token".into())
                })?;
                let value = params_value.get("value").cloned().ok_or_else(|| {
                    SessionError::Protocol("progress notification missing value".into())
                })?;
                event_tx
                    .send(SessionEvent::Progress { token, value })
                    .map_err(|_| SessionError::Disconnected)?;
            }

            event_tx
                .send(SessionEvent::Notification { method, params })
                .map_err(|_| SessionError::Disconnected)
        }
        (None, Some(id)) => {
            let id = parse_outgoing_response_id(&id)?;
            let response = if let Some(result) = object.get("result") {
                Ok(result.clone())
            } else if let Some(error) = object.get("error") {
                Err(SessionError::ServerError(parse_response_error(error)?))
            } else {
                Err(SessionError::Protocol(
                    "response missing result or error".into(),
                ))
            };

            if let Some(sender) = pending.lock().expect("pending map poisoned").remove(&id) {
                let _ = sender.send(response);
            }
            Ok(())
        }
        (None, None) => Err(SessionError::Protocol(
            "incoming message missing both method and id".into(),
        )),
    }
}

fn parse_response_error(value: &Value) -> Result<ResponseError, SessionError> {
    let object = value
        .as_object()
        .ok_or_else(|| SessionError::Protocol("response error must be an object".into()))?;
    let code = object
        .get("code")
        .and_then(Value::as_i64)
        .ok_or_else(|| SessionError::Protocol("response error missing numeric code".into()))?;
    let message = object
        .get("message")
        .and_then(Value::as_str)
        .ok_or_else(|| SessionError::Protocol("response error missing message".into()))?
        .to_owned();
    let data = object.get("data").cloned();

    Ok(ResponseError {
        code,
        message,
        data,
    })
}

fn parse_outgoing_response_id(id: &Value) -> Result<u64, SessionError> {
    let number = id
        .as_i64()
        .ok_or_else(|| SessionError::Protocol("response id must be an integer".into()))?;
    u64::try_from(number)
        .map_err(|_| SessionError::Protocol("response id must be non-negative".into()))
}

fn parse_jsonrpc_id(id: &Value) -> Result<JsonRpcId, SessionError> {
    if let Some(number) = id.as_i64() {
        return Ok(JsonRpcId::Integer(number));
    }
    if let Some(string) = id.as_str() {
        return Ok(JsonRpcId::String(string.to_owned()));
    }
    Err(SessionError::Protocol(
        "json-rpc id must be an integer or string".into(),
    ))
}

fn read_message(reader: &mut impl BufRead) -> Result<Option<Value>, SessionError> {
    let mut content_length = None;

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;

        if bytes == 0 {
            return Ok(None);
        }

        if line == "\r\n" {
            break;
        }

        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| SessionError::Protocol(format!("malformed header line: {line:?}")))?;

        if name.eq_ignore_ascii_case("content-length") {
            let parsed = value
                .trim()
                .parse::<usize>()
                .map_err(|_| SessionError::Protocol("invalid content length header".into()))?;
            content_length = Some(parsed);
        }
    }

    let content_length = content_length
        .ok_or_else(|| SessionError::Protocol("missing Content-Length header".into()))?;
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;

    Ok(Some(serde_json::from_slice(&body)?))
}
