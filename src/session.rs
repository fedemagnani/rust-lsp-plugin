//! Session runtime for stdio-backed JSON-RPC servers such as `rust-analyzer`.

use lsp_server::{
    Message, Notification, Request, RequestId as LspRequestId, Response,
    ResponseError as LspResponseError,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::{self, BufRead, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvError, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

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

    fn into_lsp(self) -> Result<LspRequestId, SessionError> {
        match self {
            Self::Integer(id) => {
                let id = i32::try_from(id)
                    .map_err(|_| SessionError::Protocol("request id overflow".into()))?;
                Ok(LspRequestId::from(id))
            }
            Self::String(id) => Ok(LspRequestId::from(id)),
        }
    }

    fn from_lsp(id: LspRequestId) -> Result<Self, SessionError> {
        serde_json::from_value(serde_json::to_value(id)?).map_err(SessionError::Json)
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
    /// A request exceeded its configured response deadline.
    #[error("request timed out after {timeout:?}: {method}")]
    RequestTimeout {
        /// The JSON-RPC method that timed out.
        method: String,
        /// The configured timeout that elapsed.
        timeout: Duration,
    },
    /// The child process did not exit within the configured shutdown timeout.
    #[error("child process did not exit within {timeout:?}")]
    ProcessExitTimeout {
        /// The configured shutdown timeout that elapsed.
        timeout: Duration,
    },
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
pub struct SessionBuilder {
    program: OsString,
    args: Vec<OsString>,
    current_dir: Option<PathBuf>,
    envs: Vec<(OsString, OsString)>,
    request_timeout: Option<Duration>,
    shutdown_timeout: Duration,
}

impl SessionBuilder {
    /// Creates a builder for the given program.
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            program: program.as_ref().to_os_string(),
            args: Vec::new(),
            current_dir: None,
            envs: Vec::new(),
            request_timeout: None,
            shutdown_timeout: DEFAULT_SHUTDOWN_TIMEOUT,
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

    /// Sets the default timeout applied to `request`.
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Sets the maximum time spent waiting for the child process to exit.
    pub fn shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.shutdown_timeout = timeout;
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
            request_timeout: self.request_timeout,
            shutdown_timeout: self.shutdown_timeout,
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
    request_timeout: Option<Duration>,
    shutdown_timeout: Duration,
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
        self.request_inner(method, params, self.request_timeout)
    }

    /// Sends a JSON-RPC request using a per-call timeout override.
    pub fn request_with_timeout<P>(
        &self,
        method: &str,
        params: P,
        timeout: Duration,
    ) -> Result<Value, SessionError>
    where
        P: Serialize,
    {
        self.request_inner(method, params, Some(timeout))
    }

    fn request_inner<P>(
        &self,
        method: &str,
        params: P,
        timeout: Option<Duration>,
    ) -> Result<Value, SessionError>
    where
        P: Serialize,
    {
        let request_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let message = Message::Request(Request::new(
            JsonRpcId::from_outgoing(request_id)?.into_lsp()?,
            method.to_owned(),
            params,
        ));

        let (tx, rx) = mpsc::channel();
        {
            let mut pending = self.pending.lock().expect("pending map poisoned");
            if self.terminated.load(Ordering::SeqCst) {
                return Err(SessionError::Disconnected);
            }
            pending.insert(request_id, tx);
        }

        if let Err(error) = self.write_message(&message) {
            self.pending
                .lock()
                .expect("pending map poisoned")
                .remove(&request_id);
            return Err(error);
        }

        match timeout {
            Some(timeout) => match rx.recv_timeout(timeout) {
                Ok(result) => result,
                Err(RecvTimeoutError::Timeout) => {
                    self.pending
                        .lock()
                        .expect("pending map poisoned")
                        .remove(&request_id);
                    Err(SessionError::RequestTimeout {
                        method: method.to_owned(),
                        timeout,
                    })
                }
                Err(RecvTimeoutError::Disconnected) => Err(SessionError::Disconnected),
            },
            None => rx.recv()?,
        }
    }

    /// Sends a JSON-RPC notification.
    pub fn notify<P>(&self, method: &str, params: P) -> Result<(), SessionError>
    where
        P: Serialize,
    {
        let message = Message::Notification(Notification::new(method.to_owned(), params));
        self.write_message(&message)
    }

    /// Sends a JSON-RPC success response to a server-originated request.
    pub fn respond<R>(&self, id: JsonRpcId, result: R) -> Result<(), SessionError>
    where
        R: Serialize,
    {
        let message = Message::Response(Response::new_ok(id.into_lsp()?, result));
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
        let mut response = Response::new_err(
            id.into_lsp()?,
            i32::try_from(code)
                .map_err(|_| SessionError::Protocol("response error code overflow".into()))?,
            message.into(),
        );
        if let Some(data) = data {
            response.error = Some(LspResponseError {
                data: Some(data),
                ..response.error.expect("new_err sets error")
            });
        }
        let message = Message::Response(response);
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

        let result = (|| {
            self.request("shutdown", Value::Null)?;
            self.notify("exit", Value::Null)?;
            self.finish_process()
        })();

        if result.is_err() && !matches!(result, Err(SessionError::ProcessExitTimeout { .. })) {
            self.shutdown_sent.store(false, Ordering::SeqCst);
        }

        result
    }

    fn write_message(&self, message: &Message) -> Result<(), SessionError> {
        if self.terminated.load(Ordering::SeqCst) {
            return Err(SessionError::Disconnected);
        }

        let mut writer = self.writer.lock().expect("writer poisoned");
        message.write(&mut *writer)?;
        Ok(())
    }

    fn finish_process(&self) -> Result<(), SessionError> {
        self.terminated.store(true, Ordering::SeqCst);
        close_pending(&self.pending, SessionError::Disconnected);

        let (status, forced_shutdown) = {
            let mut child = self.child.lock().expect("child poisoned");
            wait_for_child_exit_or_kill(&mut child, self.shutdown_timeout)?
        };
        join_transport_threads(&self.reader_handle, &self.stderr_handle);

        if forced_shutdown {
            Err(SessionError::ProcessExitTimeout {
                timeout: self.shutdown_timeout,
            })
        } else if status.success() {
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

        join_transport_threads(&self.reader_handle, &self.stderr_handle);
    }
}

fn join_transport_threads(
    reader_handle: &Mutex<Option<JoinHandle<()>>>,
    stderr_handle: &Mutex<Option<JoinHandle<()>>>,
) {
    if let Ok(mut handle) = reader_handle.lock() {
        if let Some(handle) = handle.take() {
            let _ = handle.join();
        }
    }

    if let Ok(mut handle) = stderr_handle.lock() {
        if let Some(handle) = handle.take() {
            let _ = handle.join();
        }
    }
}

fn wait_for_child_exit_or_kill(
    child: &mut Child,
    timeout: Duration,
) -> Result<(ExitStatus, bool), SessionError> {
    if let Some(status) = wait_for_child_exit(child, timeout)? {
        return Ok((status, false));
    }

    let _ = child.kill();
    let status = child.wait()?;
    Ok((status, true))
}

fn wait_for_child_exit(child: &mut Child, timeout: Duration) -> io::Result<Option<ExitStatus>> {
    let start = Instant::now();

    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }

        if start.elapsed() >= timeout {
            return Ok(None);
        }

        thread::sleep(Duration::from_millis(10));
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
            match Message::read(&mut reader) {
                Ok(Some(message)) => {
                    if let Err(error) = handle_incoming_message(message, &pending, &event_tx) {
                        terminated.store(true, Ordering::SeqCst);
                        close_pending(&pending, error);
                        break;
                    }
                }
                Ok(None) => {
                    terminated.store(true, Ordering::SeqCst);
                    close_pending(&pending, SessionError::Disconnected);
                    break;
                }
                Err(error) => {
                    terminated.store(true, Ordering::SeqCst);
                    close_pending(&pending, SessionError::Io(error));
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
            SessionError::RequestTimeout { method, timeout } => SessionError::RequestTimeout {
                method: method.clone(),
                timeout: *timeout,
            },
            SessionError::ProcessExitTimeout { timeout } => {
                SessionError::ProcessExitTimeout { timeout: *timeout }
            }
            SessionError::Disconnected => SessionError::Disconnected,
        }));
    }
}

fn handle_incoming_message(
    message: Message,
    pending: &PendingMap,
    event_tx: &Sender<SessionEvent>,
) -> Result<(), SessionError> {
    match message {
        Message::Request(request) => {
            let request = ServerRequest {
                id: JsonRpcId::from_lsp(request.id)?,
                method: request.method,
                params: Some(request.params),
            };
            event_tx
                .send(SessionEvent::ServerRequest(request))
                .map_err(|_| SessionError::Disconnected)
        }
        Message::Notification(notification) => {
            let method = notification.method;
            let params = Some(notification.params);
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
        Message::Response(response) => {
            let id = parse_outgoing_response_id(&response.id)?;
            let result = if let Some(error) = response.error {
                Err(SessionError::ServerError(parse_response_error(error)))
            } else {
                Ok(response.result.unwrap_or(Value::Null))
            };

            if let Some(sender) = pending.lock().expect("pending map poisoned").remove(&id) {
                let _ = sender.send(result);
            }
            Ok(())
        }
    }
}

fn parse_response_error(error: LspResponseError) -> ResponseError {
    ResponseError {
        code: i64::from(error.code),
        message: error.message,
        data: error.data,
    }
}

fn parse_outgoing_response_id(id: &LspRequestId) -> Result<u64, SessionError> {
    let value = serde_json::to_value(id)?;
    let number = value
        .as_i64()
        .ok_or_else(|| SessionError::Protocol("response id must be an integer".into()))?;
    u64::try_from(number)
        .map_err(|_| SessionError::Protocol("response id must be non-negative".into()))
}
