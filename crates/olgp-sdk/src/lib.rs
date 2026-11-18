//! Rust core for the OLGP HTTP API.
//!
//! Discovery and pairing go through `olgpctl`; API calls use a plain TCP
//! HTTP/1.1 client so the crate stays usable from thin FFI wrappers.

use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const DEFAULT_BASE_URL: &str = "http://olgp.local:8787";
const API_PORT: u16 = 8787;
const READ_TIMEOUT: Duration = Duration::from_secs(5);
const JOB_WAIT_TIMEOUT: Duration = Duration::from_secs(60);
const JOB_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Caller-owned search credentials forwarded on API requests.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchOptions {
    pub search_provider: Option<String>,
    pub tavily_key: Option<String>,
}

impl SearchOptions {
    /// Fill missing values from the environment.
    pub fn resolve(search_provider: Option<String>, tavily_key: Option<String>) -> Self {
        let env_value = |name: &str| env::var(name).ok().filter(|value| !value.is_empty());
        Self {
            search_provider: search_provider.or_else(|| env_value("OLGP_SEARCH_PROVIDER")),
            tavily_key: tavily_key
                .or_else(|| env_value("OLGP_TAVILY_KEY"))
                .or_else(|| env_value("TAVILY_API_KEY")),
        }
    }
}

fn validate_header_value(name: &str, value: &str) -> Result<(), String> {
    if value.chars().any(|char| char == '\r' || char == '\n') {
        return Err(format!("invalid {name}: control characters"));
    }
    Ok(())
}

/// A box found by `olgpctl discover`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoxInfo {
    pub box_id: String,
    /// Short fingerprint as printed by `olgpctl discover`.
    pub fingerprint: String,
    pub address: String,
    pub gpu: String,
}

/// The credential written by `olgpctl pair`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Credential {
    pub box_id: String,
    pub token: String,
    pub host_key: String,
    pub host_signature: String,
    pub box_spki_sha256: String,
    pub capabilities: Vec<String>,
    pub paired_at: String,
}

fn validate_box_id(box_id: &str) -> Result<(), String> {
    if box_id.len() == 32 && box_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(format!("invalid box id: {box_id}"))
    }
}

fn config_dir() -> PathBuf {
    PathBuf::from(env::var("HOME").unwrap_or_else(|_| "/tmp".into())).join(".config/olgp")
}

fn credential_path(box_id: &str) -> Result<PathBuf, String> {
    validate_box_id(box_id)?;
    Ok(config_dir().join("boxes").join(format!("{box_id}.json")))
}

fn parse_credential(text: &str) -> Result<Credential, String> {
    serde_json::from_str(text).map_err(|error| format!("credential parse: {error}"))
}

/// Parse the four whitespace-separated columns of `olgpctl discover`.
///
/// The GPU name is the last column and may contain spaces.
fn parse_discover_output(output: &str) -> Vec<BoxInfo> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let box_id = fields.next()?;
            let fingerprint = fields.next()?;
            let address = fields.next()?;
            Some(BoxInfo {
                box_id: box_id.to_owned(),
                fingerprint: fingerprint.to_owned(),
                address: address.to_owned(),
                gpu: fields.collect::<Vec<_>>().join(" "),
            })
        })
        .collect()
}

/// List boxes on the LAN by invoking `olgpctl discover`.
pub fn discover(timeout_ms: u64) -> Result<Vec<BoxInfo>, String> {
    let mut child = Command::new("olgpctl")
        .arg("discover")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("olgpctl discover: {error}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("olgpctl discover: no stdout pipe")?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut output = String::new();
        let _ = stdout.read_to_string(&mut output);
        let _ = sender.send(output);
    });
    let output = match receiver.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(output) => output,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("olgpctl discover timed out after {timeout_ms}ms"));
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            return Err("olgpctl discover: output reader failed".into());
        }
    };
    let status = child
        .wait()
        .map_err(|error| format!("olgpctl discover: {error}"))?;
    if !status.success() {
        return Err(format!("olgpctl discover exited with {status}"));
    }
    Ok(parse_discover_output(&output))
}

/// Pair with a box and return the credential stored by `olgpctl`.
pub fn pair(box_id: &str, code: &str) -> Result<Credential, String> {
    validate_box_id(box_id)?;
    let status = Command::new("olgpctl")
        .args(["pair", box_id, "--code", code])
        .status()
        .map_err(|error| format!("olgpctl pair: {error}"))?;
    if !status.success() {
        return Err(format!("olgpctl pair exited with {status}"));
    }
    let path = credential_path(box_id)?;
    let text = fs::read_to_string(path).map_err(|error| format!("credential read: {error}"))?;
    parse_credential(&text)
}

/// Split an `http://host[:port]` base URL into host, port and Host header.
fn split_base_url(base_url: &str) -> Result<(String, u16, String), String> {
    let rest = base_url
        .strip_prefix("http://")
        .ok_or("base URL must start with http:// (the API port is plain HTTP)")?;
    let authority = rest.split('/').next().unwrap_or(rest);
    if authority.is_empty() {
        return Err("base URL has no host".into());
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && port.parse::<u16>().is_ok() => {
            (host.to_owned(), port.parse::<u16>().unwrap_or(80))
        }
        _ => (authority.to_owned(), 80),
    };
    let host = host.trim_start_matches('[').trim_end_matches(']').to_owned();
    Ok((host, port, authority.to_owned()))
}

fn http_base(address: &str, port: u16) -> String {
    if address.contains(':') {
        format!("http://[{address}]:{port}")
    } else {
        format!("http://{address}:{port}")
    }
}

/// Percent-encode one path segment so ids cannot alter the request path.
fn path_segment(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Build an HTTP/1.1 request with a Bearer token and optional JSON body.
fn build_request(
    method: &str,
    authority: &str,
    path: &str,
    token: &str,
    body: Option<&str>,
) -> String {
    let payload = body.unwrap_or("");
    let mut request =
        format!("{method} {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n");
    if !token.is_empty() {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    if body.is_some() {
        request.push_str("Content-Type: application/json\r\n");
    }
    request.push_str(&format!(
        "Content-Length: {}\r\n\r\n{payload}",
        payload.len()
    ));
    request
}

fn with_search_headers(request: String, search: &SearchOptions) -> String {
    let mut headers = String::new();
    if let Some(provider) = &search.search_provider {
        headers.push_str(&format!("X-Wici-Search-Provider: {provider}\r\n"));
    }
    if let Some(key) = &search.tavily_key {
        headers.push_str(&format!("X-Wici-Tavily-Key: {key}\r\n"));
    }
    if headers.is_empty() {
        return request;
    }
    let anchor = request
        .find("Content-Type: application/json\r\n")
        .or_else(|| request.find("Content-Length: "))
        .unwrap_or(request.len());
    let mut result = request;
    result.insert_str(anchor, &headers);
    result
}

fn decode_chunked(body: &str) -> Result<String, String> {
    let mut rest = body;
    let mut out = String::new();
    loop {
        let (size_line, tail) = rest
            .split_once("\r\n")
            .ok_or("malformed chunked response")?;
        let size_text = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|error| format!("malformed chunk size: {error}"))?;
        if size == 0 {
            return Ok(out);
        }
        if tail.len() < size {
            return Err("truncated chunked response".into());
        }
        let (chunk, tail) = tail.split_at(size);
        out.push_str(chunk);
        rest = tail
            .strip_prefix("\r\n")
            .ok_or("malformed chunk terminator")?;
    }
}

struct HttpResponse {
    status: u16,
    body: String,
}

fn parse_response(raw: &str) -> Result<HttpResponse, String> {
    let (head, raw_body) = raw
        .split_once("\r\n\r\n")
        .ok_or("malformed HTTP response")?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or("malformed HTTP status line")?;
    let chunked = head.lines().skip(1).any(|line| {
        let lower = line.to_ascii_lowercase();
        lower.starts_with("transfer-encoding:") && lower.contains("chunked")
    });
    let body = if chunked {
        decode_chunked(raw_body)?
    } else {
        raw_body.to_owned()
    };
    Ok(HttpResponse { status, body })
}

#[derive(Clone, Debug)]
struct Runtime {
    host: String,
    port: u16,
    authority: String,
    token: String,
    search: SearchOptions,
}

impl Runtime {
    fn send(&self, method: &str, path: &str, body: Option<&str>) -> Result<HttpResponse, String> {
        let request = with_search_headers(
            build_request(method, &self.authority, path, &self.token, body),
            &self.search,
        );
        let mut stream = TcpStream::connect((self.host.as_str(), self.port))
            .map_err(|error| format!("connect {}:{}: {error}", self.host, self.port))?;
        stream
            .set_read_timeout(Some(READ_TIMEOUT))
            .map_err(|error| format!("read timeout: {error}"))?;
        stream
            .write_all(request.as_bytes())
            .map_err(|error| format!("HTTP write: {error}"))?;
        let mut raw = String::new();
        stream
            .read_to_string(&mut raw)
            .map_err(|error| format!("HTTP read: {error}"))?;
        parse_response(&raw)
    }

    fn request(&self, method: &str, path: &str, body: Option<&str>) -> Result<String, String> {
        let response = self.send(method, path, body)?;
        if response.status >= 400 {
            return Err(http_error(&response));
        }
        Ok(response.body)
    }

    fn json(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let payload = body.map(Value::to_string);
        let text = self.request(method, path, payload.as_deref())?;
        parse_json_body(&text)
    }
}

fn http_error(response: &HttpResponse) -> String {
    let detail: String = response.body.chars().take(200).collect();
    format!("olgp: HTTP {} {}", response.status, detail)
}

fn parse_json_body(text: &str) -> Result<Value, String> {
    if text.trim().is_empty() {
        Ok(Value::Null)
    } else {
        serde_json::from_str(text).map_err(|error| format!("olgp: invalid JSON: {error}"))
    }
}

/// A client bound to one paired box.
#[derive(Clone, Debug)]
pub struct Client {
    pub chat: Chat,
    pub engine: Engine,
    pub speakers: Speakers,
    pub timers: Timers,
    pub memos: Memos,
    pub knowledge_bases: KnowledgeBases,
    pub images: Images,
}

impl Client {
    /// Build a client for a base URL and Device Token, with search headers from the environment.
    pub fn new(base_url: &str, token: &str) -> Result<Self, String> {
        Self::new_with_options(base_url, token, SearchOptions::default())
    }

    /// Build a client with explicit search credentials; missing values fall back to the environment.
    pub fn new_with_options(
        base_url: &str,
        token: &str,
        options: SearchOptions,
    ) -> Result<Self, String> {
        let (host, port, authority) = split_base_url(base_url)?;
        validate_header_value("token", token)?;
        let search = SearchOptions::resolve(options.search_provider, options.tavily_key);
        if let Some(provider) = &search.search_provider {
            validate_header_value("search provider", provider)?;
        }
        if let Some(key) = &search.tavily_key {
            validate_header_value("Tavily key", key)?;
        }
        let inner = Arc::new(Runtime {
            host,
            port,
            authority,
            token: token.to_owned(),
            search,
        });
        Ok(Self {
            chat: Chat {
                completions: Completions {
                    inner: inner.clone(),
                },
            },
            engine: Engine {
                inner: inner.clone(),
            },
            speakers: Speakers {
                inner: inner.clone(),
            },
            timers: Timers {
                inner: inner.clone(),
            },
            memos: Memos {
                inner: inner.clone(),
            },
            knowledge_bases: KnowledgeBases {
                inner: inner.clone(),
            },
            images: Images {
                jobs: ImageJobs { inner },
            },
        })
    }
}

/// Connect using the paired credential for `box`, or environment defaults.
///
/// `box` is a 32-hex box id or an `http://host:port` base URL. With `None`,
/// `OLGP_BASE_URL` and `OLGP_DEVICE_TOKEN` are used.
pub fn connect(r#box: Option<&str>) -> Result<Client, String> {
    match r#box {
        None => {
            let base = env::var("OLGP_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.into());
            let token = env::var("OLGP_DEVICE_TOKEN").unwrap_or_default();
            Client::new(&base, &token)
        }
        Some(base_url) if base_url.starts_with("http://") => {
            let token = env::var("OLGP_DEVICE_TOKEN").unwrap_or_default();
            Client::new(base_url, &token)
        }
        Some(box_id) => {
            validate_box_id(box_id)?;
            let path = credential_path(box_id)?;
            let text =
                fs::read_to_string(path).map_err(|error| format!("credential read: {error}"))?;
            let credential = parse_credential(&text)?;
            let base = if let Ok(url) = env::var("OLGP_BASE_URL") {
                url
            } else {
                let boxes = discover(2_000)?;
                let found = boxes
                    .into_iter()
                    .find(|found| found.box_id == box_id)
                    .ok_or_else(|| format!("box {box_id} not found; set OLGP_BASE_URL"))?;
                http_base(&found.address, API_PORT)
            };
            Client::new(&base, &credential.token)
        }
    }
}

/// Scheduler lease attached to a request.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Lease {
    Interactive,
    Background,
}

/// One chat message.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }
}

fn chat_request(model: &str, messages: &[Message], stream: bool, lease: Lease) -> Value {
    json!({
        "model": model,
        "messages": messages,
        "stream": stream,
        "lease": lease,
    })
}

/// Parsed reply from a chat completion (JSON or `text/event-stream`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatCompletion {
    pub content: String,
}

impl ChatCompletion {
    fn from_json(text: &str) -> Result<Self, String> {
        let reply: Value =
            serde_json::from_str(text).map_err(|error| format!("chat reply JSON: {error}"))?;
        let content = reply
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or("chat reply has no choices[0].message.content")?;
        Ok(Self {
            content: content.to_owned(),
        })
    }

    fn from_stream(text: &str) -> Result<Self, String> {
        let mut content = String::new();
        let mut done = false;
        for line in text.lines() {
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                done = true;
                break;
            }
            let chunk: Value =
                serde_json::from_str(data).map_err(|error| format!("SSE chunk JSON: {error}"))?;
            let delta = chunk
                .pointer("/choices/0/delta/content")
                .or_else(|| chunk.pointer("/choices/0/message/content"))
                .and_then(Value::as_str);
            if let Some(delta) = delta {
                content.push_str(delta);
            }
        }
        if !done {
            return Err("stream ended without [DONE]".into());
        }
        Ok(Self { content })
    }

    /// Parse one API reply body.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.trim_start().starts_with('{') {
            Self::from_json(text)
        } else {
            Self::from_stream(text)
        }
    }
}

#[derive(Clone, Debug)]
pub struct Chat {
    pub completions: Completions,
}

#[derive(Clone, Debug)]
pub struct Completions {
    inner: Arc<Runtime>,
}

impl Completions {
    pub fn create(
        &self,
        model: &str,
        messages: &[Message],
        stream: bool,
        lease: Lease,
    ) -> Result<ChatCompletion, String> {
        let payload = chat_request(model, messages, stream, lease).to_string();
        let body = self
            .inner
            .request("POST", "/v1/chat/completions", Some(&payload))?;
        ChatCompletion::parse(&body)
    }
}

#[derive(Clone, Debug)]
pub struct Engine {
    inner: Arc<Runtime>,
}

impl Engine {
    pub fn status(&self) -> Result<Value, String> {
        self.inner.json("GET", "/engine/status", None)
    }

    pub fn capabilities(&self) -> Result<Value, String> {
        self.inner.json("GET", "/engine/capabilities", None)
    }

    pub fn sessions(&self) -> Result<Value, String> {
        self.inner.json("GET", "/engine/sessions", None)
    }
}

#[derive(Clone, Debug)]
pub struct Speakers {
    inner: Arc<Runtime>,
}

impl Speakers {
    pub fn list(&self) -> Result<Value, String> {
        self.inner.json("GET", "/v1/speakers", None)
    }

    pub fn retrieve(&self, speaker_id: &str) -> Result<Value, String> {
        self.inner
            .json("GET", &format!("/v1/speakers/{}", path_segment(speaker_id)), None)
    }

    pub fn update(&self, speaker_id: &str, patch: &Value) -> Result<Value, String> {
        self.inner.json(
            "PATCH",
            &format!("/v1/speakers/{}", path_segment(speaker_id)),
            Some(patch),
        )
    }

    pub fn play(&self, speaker_id: &str) -> Result<Value, String> {
        self.inner.json(
            "POST",
            &format!("/v1/speakers/{}:play", path_segment(speaker_id)),
            None,
        )
    }

    pub fn stop(&self, speaker_id: &str) -> Result<Value, String> {
        self.inner.json(
            "POST",
            &format!("/v1/speakers/{}:stop", path_segment(speaker_id)),
            None,
        )
    }

    pub fn bluetooth(&self, speaker_id: &str) -> Result<Value, String> {
        self.inner.json(
            "GET",
            &format!("/v1/speakers/{}/bluetooth", path_segment(speaker_id)),
            None,
        )
    }

    pub fn start_bluetooth_pairing(&self, speaker_id: &str) -> Result<Value, String> {
        self.inner.json(
            "POST",
            &format!(
                "/v1/speakers/{}:startBluetoothPairing",
                path_segment(speaker_id)
            ),
            None,
        )
    }
}

#[derive(Clone, Debug)]
pub struct Timers {
    inner: Arc<Runtime>,
}

impl Timers {
    pub fn list(&self) -> Result<Value, String> {
        self.inner.json("GET", "/v1/timers", None)
    }

    pub fn create(&self, timer: &Value) -> Result<Value, String> {
        self.inner.json("POST", "/v1/timers", Some(timer))
    }

    pub fn cancel(&self, timer_id: &str) -> Result<Value, String> {
        self.inner.json(
            "POST",
            &format!("/v1/timers/{}:cancel", path_segment(timer_id)),
            None,
        )
    }

    pub fn snooze(&self, timer_id: &str) -> Result<Value, String> {
        self.inner.json(
            "POST",
            &format!("/v1/timers/{}:snooze", path_segment(timer_id)),
            None,
        )
    }
}

#[derive(Clone, Debug)]
pub struct Memos {
    inner: Arc<Runtime>,
}

impl Memos {
    pub fn list(&self) -> Result<Value, String> {
        self.inner.json("GET", "/v1/memos", None)
    }

    pub fn create(&self, memo: &Value) -> Result<Value, String> {
        self.inner.json("POST", "/v1/memos", Some(memo))
    }

    pub fn dismiss(&self, memo_id: &str) -> Result<Value, String> {
        self.inner.json(
            "POST",
            &format!("/v1/memos/{}:dismiss", path_segment(memo_id)),
            None,
        )
    }
}

#[derive(Clone, Debug)]
pub struct KnowledgeBases {
    inner: Arc<Runtime>,
}

impl KnowledgeBases {
    pub fn create(&self, body: &Value) -> Result<Value, String> {
        self.inner.json("POST", "/v1/knowledge-bases", Some(body))
    }

    pub fn retrieve(&self, kb_id: &str) -> Result<Value, String> {
        self.inner
            .json("GET", &format!("/v1/knowledge-bases/{}", path_segment(kb_id)), None)
    }

    pub fn delete(&self, kb_id: &str) -> Result<Value, String> {
        self.inner
            .json("DELETE", &format!("/v1/knowledge-bases/{}", path_segment(kb_id)), None)
    }

    pub fn documents(&self, kb_id: &str) -> Result<Value, String> {
        self.inner.json(
            "GET",
            &format!("/v1/knowledge-bases/{}/documents", path_segment(kb_id)),
            None,
        )
    }

    pub fn query(&self, kb_id: &str, query: &Value) -> Result<Value, String> {
        self.inner.json(
            "POST",
            &format!("/v1/knowledge-bases/{}:query", path_segment(kb_id)),
            Some(query),
        )
    }
}

fn image_job_terminal(job: &Value) -> bool {
    let status = job
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        status.as_str(),
        "succeeded"
            | "completed"
            | "complete"
            | "failed"
            | "error"
            | "cancelled"
            | "canceled"
            | "model_missing"
    ) || job.get("phase").and_then(Value::as_str) == Some("done")
}

fn frame_stream_request<T: Serialize, U: Serialize>(frames: &T, options: &U) -> Value {
    json!({"frames": frames, "options": options})
}

#[derive(Clone, Debug)]
pub struct Images {
    pub jobs: ImageJobs,
}

impl Images {
    pub fn stream<T: Serialize, U: Serialize>(
        &self,
        frames: &T,
        options: &U,
    ) -> Result<Value, String> {
        let payload = frame_stream_request(frames, options);
        self.jobs
            .inner
            .json("POST", "/v2/images/frames:process", Some(&payload))
    }
}

#[derive(Clone, Debug)]
pub struct ImageJobs {
    inner: Arc<Runtime>,
}

impl ImageJobs {
    pub fn create(&self, job: &Value) -> Result<Value, String> {
        self.inner.json("POST", "/v2/images/jobs", Some(job))
    }

    pub fn retrieve(&self, job_id: &str) -> Result<Value, String> {
        self.inner
            .json("GET", &format!("/v2/images/jobs/{}", path_segment(job_id)), None)
    }

    pub fn observe(&self, job_id: &str) -> Result<Value, String> {
        self.retrieve(job_id)
    }

    pub fn wait(&self, job_id: &str) -> Result<Value, String> {
        let deadline = Instant::now() + JOB_WAIT_TIMEOUT;
        loop {
            let job = self.retrieve(job_id)?;
            if image_job_terminal(&job) {
                return Ok(job);
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "olgp: image job {job_id} timed out after {}ms",
                    JOB_WAIT_TIMEOUT.as_millis()
                ));
            }
            thread::sleep(JOB_POLL_INTERVAL);
        }
    }

    pub fn result(&self, job_id: &str) -> Result<Value, String> {
        let response = self.inner.send(
            "GET",
            &format!("/v2/images/jobs/{}/result", path_segment(job_id)),
            None,
        )?;
        if response.status >= 400 && response.status != 409 {
            return Err(http_error(&response));
        }
        parse_json_body(&response.body)
    }

    pub fn cancel(&self, job_id: &str) -> Result<Value, String> {
        self.inner.json(
            "POST",
            &format!("/v2/images/jobs/{}:cancel", path_segment(job_id)),
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX_ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn builds_request_with_token_and_json_body() {
        let request = build_request(
            "POST",
            "olgp.local:8787",
            "/v1/chat/completions",
            "token123",
            Some("{\"a\":1}"),
        );
        assert_eq!(
            request,
            "POST /v1/chat/completions HTTP/1.1\r\n\
             Host: olgp.local:8787\r\n\
             Connection: close\r\n\
             Authorization: Bearer token123\r\n\
             Content-Type: application/json\r\n\
             Content-Length: 7\r\n\
             \r\n\
             {\"a\":1}"
        );
    }

    #[test]
    fn adds_search_headers_when_configured() {
        let request = with_search_headers(
            build_request(
                "POST",
                "olgp.local:8787",
                "/v1/chat/completions",
                "token123",
                Some("{\"a\":1}"),
            ),
            &SearchOptions {
                search_provider: Some("tavily".into()),
                tavily_key: Some("key".into()),
            },
        );
        assert_eq!(
            request,
            "POST /v1/chat/completions HTTP/1.1\r\n\
             Host: olgp.local:8787\r\n\
             Connection: close\r\n\
             Authorization: Bearer token123\r\n\
             X-Wici-Search-Provider: tavily\r\n\
             X-Wici-Tavily-Key: key\r\n\
             Content-Type: application/json\r\n\
             Content-Length: 7\r\n\
             \r\n\
             {\"a\":1}"
        );
    }

    #[test]
    fn search_headers_are_optional_and_reject_injection() {
        let plain = with_search_headers(
            build_request("GET", "olgp.local:8787", "/v1/models", "", None),
            &SearchOptions::default(),
        );
        assert!(!plain.contains("X-Wici-"));
        assert!(Client::new_with_options(
            "http://olgp.local:8787",
            "ok",
            SearchOptions {
                search_provider: Some("tavily\r\nX-Injected: 1".into()),
                tavily_key: None,
            },
        )
        .is_err());
        let client = Client::new_with_options(
            "http://olgp.local:8787",
            "ok",
            SearchOptions {
                search_provider: Some("tavily".into()),
                tavily_key: Some("key".into()),
            },
        )
        .unwrap();
        assert_eq!(
            client
                .chat
                .completions
                .inner
                .search
                .search_provider
                .as_deref(),
            Some("tavily")
        );
    }

    #[test]
    fn builds_get_request_without_token() {
        let request = build_request("GET", "127.0.0.1:8787", "/v1/models", "", None);
        assert_eq!(
            request,
            "GET /v1/models HTTP/1.1\r\nHost: 127.0.0.1:8787\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
        );
    }

    #[test]
    fn parses_status_and_body() {
        let raw = "HTTP/1.1 401 Unauthorized\r\nContent-Length: 2\r\n\r\n{}";
        let response = parse_response(raw).unwrap();
        assert_eq!(response.status, 401);
        assert_eq!(response.body, "{}");
    }

    #[test]
    fn parses_chunked_body() {
        let raw = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n";
        let response = parse_response(raw).unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "{\"a\":1}");
        assert!(decode_chunked("4\r\nabcd\r\n0\r\n\r\n").unwrap() == "abcd");
    }

    #[test]
    fn rejects_malformed_responses() {
        assert!(parse_response("not-http").is_err());
        assert!(parse_response("HTTP/1.1\r\n\r\nbody").is_err());
        assert!(decode_chunked("zz\r\nbody").is_err());
    }

    #[test]
    fn parses_discover_columns_including_gpu_with_spaces() {
        let output = format!(
            "{BOX_ID} aaaaaaaaaaaabbbbbbbbbbbb 192.168.1.9 NVIDIA GeForce GTX 1650\n\
             ffffffffffffffffffffffffffffffff cccccccccccdddddddddddd 10.0.0.2 Test GPU\nbroken\n"
        );
        let boxes = parse_discover_output(&output);
        assert_eq!(boxes.len(), 2);
        assert_eq!(boxes[0].box_id, BOX_ID);
        assert_eq!(boxes[0].fingerprint, "aaaaaaaaaaaabbbbbbbbbbbb");
        assert_eq!(boxes[0].address, "192.168.1.9");
        assert_eq!(boxes[0].gpu, "NVIDIA GeForce GTX 1650");
        assert_eq!(boxes[1].gpu, "Test GPU");
    }

    #[test]
    fn parses_credential_file() {
        let text = json!({
            "box_id": BOX_ID,
            "token": "tok",
            "host_key": "aa",
            "host_signature": "bb",
            "box_spki_sha256": "cc",
            "capabilities": ["cuda"],
            "paired_at": "2026-01-01T00:00:00Z",
        })
        .to_string();
        let credential = parse_credential(&text).unwrap();
        assert_eq!(credential.box_id, BOX_ID);
        assert_eq!(credential.token, "tok");
        assert_eq!(credential.capabilities, vec!["cuda".to_owned()]);
        assert!(parse_credential("{}").is_err());
    }

    #[test]
    fn builds_chat_request() {
        let messages = [Message::system("be brief"), Message::user("hello")];
        let request = chat_request("qwen25-0p5", &messages, true, Lease::Background);
        assert_eq!(
            request,
            json!({
                "model": "qwen25-0p5",
                "messages": [
                    {"role": "system", "content": "be brief"},
                    {"role": "user", "content": "hello"}
                ],
                "stream": true,
                "lease": "background",
            })
        );
    }

    #[test]
    fn parses_json_chat_reply() {
        let reply = "{\"choices\":[{\"message\":{\"content\":\"hi\"}}]}";
        assert_eq!(
            ChatCompletion::parse(reply).unwrap(),
            ChatCompletion {
                content: "hi".into()
            }
        );
        assert!(ChatCompletion::parse("{\"choices\":[]}").is_err());
    }

    #[test]
    fn parses_sse_chat_reply() {
        let reply = "data: {\"choices\":[{\"delta\":{\"content\":\"he\"}}]}\n\n\
                     data: {\"choices\":[{\"delta\":{\"content\":\"llo\"}}]}\n\n\
                     data: [DONE]\n\n";
        assert_eq!(
            ChatCompletion::parse(reply).unwrap().content,
            "hello"
        );
        assert!(ChatCompletion::parse("data: {\"choices\":[]}\n\n").is_err());
    }

    #[test]
    fn splits_base_urls() {
        assert_eq!(
            split_base_url("http://olgp.local:8787/v1").unwrap(),
            ("olgp.local".to_owned(), 8787_u16, "olgp.local:8787".to_owned())
        );
        assert_eq!(
            split_base_url("http://127.0.0.1").unwrap(),
            ("127.0.0.1".to_owned(), 80_u16, "127.0.0.1".to_owned())
        );
        assert!(split_base_url("https://olgp.local:8787").is_err());
        assert!(split_base_url("http://").is_err());
        assert_eq!(http_base("::1", 8787), "http://[::1]:8787");
    }

    #[test]
    fn validates_ids_and_encodes_path_segments() {
        assert!(validate_box_id(BOX_ID).is_ok());
        assert!(validate_box_id("../../etc/passwd").is_err());
        assert_eq!(path_segment("a/b c"), "a%2Fb%20c");
        assert_eq!(path_segment("abc-1.2"), "abc-1.2");
    }

    #[test]
    fn rejects_header_injection_and_accepts_client() {
        assert!(Client::new("http://olgp.local:8787", "ok").is_ok());
        assert!(Client::new("http://olgp.local:8787", "bad\r\nX: 1").is_err());
    }

    #[test]
    fn recognizes_terminal_image_jobs() {
        assert!(image_job_terminal(&json!({"status": "succeeded"})));
        assert!(image_job_terminal(&json!({"status": "MODEL_MISSING"})));
        assert!(image_job_terminal(&json!({"status": "queued", "phase": "done"})));
        assert!(!image_job_terminal(&json!({"status": "running", "phase": "generating"})));
        assert!(!image_job_terminal(&Value::Null));
    }

    #[test]
    fn builds_frame_stream_payload() {
        let frames = vec!["frame-one"];
        let options = json!({"operation": "detect"});
        let payload = frame_stream_request(&frames, &options);
        assert_eq!(payload["frames"][0], "frame-one");
        assert_eq!(payload["options"]["operation"], "detect");
    }

    #[test]
    fn parses_job_error_body() {
        let body = "{\"error\":{\"code\":\"result_unavailable\"}}";
        let parsed = parse_json_body(body).unwrap();
        assert_eq!(parsed["error"]["code"], "result_unavailable");
    }
}
