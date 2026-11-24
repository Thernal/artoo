//! The one HTTP seam: every call the app makes to a host goes through `send`, which runs `curl` (so the VPN's routes,
//! the system's proxies and its certificates apply as they do for the developer's shell) and nothing else does.
//!
//! The request is a config on curl's stdin (`-K -`), so a token never shows in `ps`; the answer's headers come back
//! with `-D -` and are read here, one block per redirect hop. Tests replace the whole transport with
//! `with_transport`: they record the requests and answer from canned JSON, no process and no network.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

/// What to ask.
#[derive(Clone)]
pub struct Request {
    pub method: String,
    pub url: String,
    /// `(name, value)`; a token goes here, and reaches curl on stdin only.
    pub headers: Vec<(String, String)>,
    /// The body as text (JSON), sent as given.
    pub body: Option<String>,
    /// Fields sent form-urlencoded.
    pub form: Vec<(String, String)>,
    /// A file sent as a multipart field: `(field, path)`.
    pub upload: Option<(String, PathBuf)>,
    /// The answer's body written to this file, not kept in memory (`Response::body` is then empty).
    pub output: Option<PathBuf>,
    /// Follow redirects (curl keeps `Authorization` off another host).
    pub follow: bool,
    /// Give up on a body larger than this many bytes.
    pub max_filesize: Option<u64>,
    /// The whole call, connecting included.
    pub timeout: Duration,
}

impl Request {
    pub fn get(url: impl Into<String>) -> Self {
        Request {
            method: "GET".to_owned(),
            url: url.into(),
            headers: Vec::new(),
            body: None,
            form: Vec::new(),
            upload: None,
            output: None,
            follow: false,
            max_filesize: None,
            timeout: Duration::from_secs(25),
        }
    }

    pub fn method(mut self, method: &str) -> Self {
        self.method = method.to_owned();
        self
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_owned(), value.into()));
        self
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn form(mut self, fields: &[(&str, String)]) -> Self {
        self.form
            .extend(fields.iter().map(|(n, v)| ((*n).to_owned(), v.clone())));
        self
    }

    pub fn upload(mut self, field: &str, file: PathBuf) -> Self {
        self.upload = Some((field.to_owned(), file));
        self
    }

    pub fn output(mut self, file: PathBuf) -> Self {
        self.output = Some(file);
        self
    }

    pub fn follow(mut self) -> Self {
        self.follow = true;
        self
    }

    pub fn max_filesize(mut self, bytes: u64) -> Self {
        self.max_filesize = Some(bytes);
        self
    }

    pub fn timeout(mut self, seconds: u64) -> Self {
        self.timeout = Duration::from_secs(seconds);
        self
    }

    /// The host the request goes to, for the words of an error.
    pub fn host(&self) -> String {
        host_of(&self.url)
    }
}

/// What came back (the last hop, when redirects were followed).
#[derive(Debug, Clone, Default)]
pub struct Response {
    pub status: u16,
    /// Names in lower case, in the order sent.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The first header called `name` (any case).
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        (self.headers.iter())
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }

    /// 200–299.
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// What kind of thing went wrong before any answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The name did not resolve, or the connection was refused (curl 6, 7): the VPN is off, or the network.
    Offline,
    /// The host did not answer in time (curl 28).
    Timeout,
    /// The secure connection could not be made (curl 35, 51, 58, 60).
    Tls,
    /// The host asked to be left alone until `HttpError::until`; the call was not made, or was refused for it.
    Limited,
    /// `curl` is not on this machine.
    NoCurl,
    /// Anything else curl said, or a file it could not be given.
    Failed,
}

/// A call that got no answer.
#[derive(Debug, Clone)]
pub struct HttpError {
    pub kind: Kind,
    pub host: String,
    /// curl's exit code, when it ran.
    pub code: Option<i32>,
    /// What curl said on its stderr, trimmed.
    pub detail: String,
    /// For `Kind::Limited`: the Unix second the host may be called again.
    pub until: i64,
}

impl HttpError {
    /// An error of `kind` for `host`, for a transport that fakes one.
    pub fn new(kind: Kind, host: &str) -> Self {
        HttpError {
            kind,
            host: host.to_owned(),
            code: None,
            detail: String::new(),
            until: 0,
        }
    }

    /// The host asked to wait until the Unix second `until`.
    pub fn limited(host: &str, until: i64) -> Self {
        HttpError {
            until,
            ..HttpError::new(Kind::Limited, host)
        }
    }

    /// The error curl's exit `code` stands for.
    pub fn from_exit(code: i32, host: &str, detail: &str) -> Self {
        let kind = match code {
            6 | 7 => Kind::Offline,
            28 => Kind::Timeout,
            35 | 51 | 58 | 60 => Kind::Tls,
            _ => Kind::Failed,
        };
        HttpError {
            kind,
            host: host.to_owned(),
            code: Some(code),
            detail: detail.trim().to_owned(),
            until: 0,
        }
    }

    /// One line for the app's log: curl's exit and its words.
    pub fn log(&self) -> String {
        format!(
            "curl {}: {}",
            self.code.unwrap_or(-1),
            if self.detail.is_empty() {
                "(nothing said)"
            } else {
                &self.detail
            }
        )
    }
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let host = &self.host;
        match self.kind {
            Kind::Offline => write!(f, "Can't reach {host} — is the VPN on?"),
            Kind::Timeout => write!(f, "{host} did not answer in time"),
            Kind::Tls => write!(f, "Can't make a secure connection to {host}"),
            Kind::Limited => write!(f, "{host} asks to wait until {}", clock_time(self.until)),
            Kind::NoCurl => write!(f, "curl is not installed"),
            Kind::Failed if self.detail.is_empty() => write!(f, "The call to {host} failed"),
            Kind::Failed => write!(f, "{}", self.detail),
        }
    }
}

impl std::error::Error for HttpError {}

/// What answers a request in place of curl.
pub type Transport = dyn Fn(&Request) -> Result<Response, HttpError> + Send + Sync;

static TRANSPORT: Mutex<Option<Arc<Transport>>> = Mutex::new(None);
/// Under a test transport: the clock (`None` is the real one) and the pauses taken.
static NOW: Mutex<Option<i64>> = Mutex::new(None);
static SLEPT: Mutex<Vec<Duration>> = Mutex::new(Vec::new());
/// Per host, the Unix second before which it asked not to be called.
static CLOSED: Mutex<Vec<(String, i64)>> = Mutex::new(Vec::new());
/// One test at a time owns the transport.
static GATE: Mutex<()> = Mutex::new(());

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Holds the replaced transport until it is dropped.
pub struct TransportGuard(#[allow(dead_code)] MutexGuard<'static, ()>);

impl Drop for TransportGuard {
    fn drop(&mut self) {
        *lock(&TRANSPORT) = None;
        *lock(&NOW) = None;
        lock(&SLEPT).clear();
        lock(&CLOSED).clear();
    }
}

impl TransportGuard {
    /// Fixes the clock at the Unix second `now`; a pause then moves it on instead of sleeping.
    pub fn at(&self, now: i64) {
        *lock(&NOW) = Some(now);
    }

    /// The pauses the seam took (retries, a `Retry-After`), none of them slept for real under a test transport.
    pub fn slept(&self) -> Vec<Duration> {
        lock(&SLEPT).clone()
    }
}

/// Makes `f` answer every `send` — on any thread — until the guard drops; a test records the requests in `f` and
/// answers from canned JSON. Tests that install one run one after another.
pub fn with_transport(
    f: impl Fn(&Request) -> Result<Response, HttpError> + Send + Sync + 'static,
) -> TransportGuard {
    let gate = lock(&GATE);
    lock(&CLOSED).clear();
    lock(&SLEPT).clear();
    *lock(&NOW) = None;
    *lock(&TRANSPORT) = Some(Arc::new(f));
    TransportGuard(gate)
}

/// A canned answer: `status` and `body`.
pub fn reply(status: u16, body: &str) -> Result<Response, HttpError> {
    Ok(Response {
        status,
        headers: Vec::new(),
        body: body.as_bytes().to_vec(),
    })
}

/// Sends `request`: the answer whatever its status, or the reason there was none. A GET is tried again (at most
/// twice, after 0.5 s then 1.5 s, jittered) when the host was unreachable, did not answer in time, or said 429, 502,
/// 503 or 504; a `Retry-After` of up to 30 s is waited out instead. A host that asked for more, or whose rate limit
/// is used up, is not called until then: the call fails at once with `Kind::Limited`. Nothing but a GET is retried.
pub fn send(request: &Request) -> Result<Response, HttpError> {
    guarded(request, None)
}

/// `send`, with `progress` told how far a body going out is (0.0–1.0, as curl's bar says).
pub fn send_with_progress(
    request: &Request,
    progress: &dyn Fn(f32),
) -> Result<Response, HttpError> {
    guarded(request, Some(progress))
}

const BACKOFF: [f64; 2] = [0.5, 1.5];
/// The longest `Retry-After` waited out in place; a longer one closes the host instead.
const LONGEST_WAIT: i64 = 30;
/// How long a limit without a date closes the host.
const DEFAULT_WAIT: i64 = 60;

fn guarded(request: &Request, progress: Option<&dyn Fn(f32)>) -> Result<Response, HttpError> {
    let host = request.host();
    if let Some(until) = closed_until(&host) {
        return Err(HttpError::limited(&host, until));
    }
    let again = request.method.eq_ignore_ascii_case("GET");
    let mut tried = 0;
    loop {
        let answer = run(request, progress);
        let can_retry = again && tried < BACKOFF.len();
        let pause = match &answer {
            Err(e) if matches!(e.kind, Kind::Offline | Kind::Timeout) => {
                if !can_retry {
                    return answer;
                }
                backoff(tried)
            }
            Err(_) => return answer,
            Ok(r) => {
                let now = now();
                if let Some(until) = used_up(r, now) {
                    close(&host, until);
                }
                if let Some(until) = limit(r, now) {
                    let wait = until - now;
                    if can_retry && wait <= LONGEST_WAIT {
                        // Waiting it out: the host stays open.
                        unclose(&host);
                        Duration::from_secs(wait.max(0) as u64)
                    } else {
                        close(&host, until);
                        return Err(HttpError::limited(&host, until));
                    }
                } else if can_retry && matches!(r.status, 502..=504) {
                    backoff(tried)
                } else {
                    return answer;
                }
            }
        };
        sleep(pause);
        tried += 1;
    }
}

/// The wait before retry `tried` (0-based): 0.5 s, then 1.5 s, each ±25 %.
fn backoff(tried: usize) -> Duration {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let jitter = 0.75 + f64::from(nanos % 1000) / 2000.0;
    Duration::from_secs_f64(BACKOFF[tried] * jitter)
}

fn sleep(pause: Duration) {
    if lock(&TRANSPORT).is_some() {
        lock(&SLEPT).push(pause);
        if let Some(now) = lock(&NOW).as_mut() {
            *now += pause.as_secs() as i64;
        }
    } else {
        std::thread::sleep(pause);
    }
}

/// The Unix second now.
fn now() -> i64 {
    (*lock(&NOW)).unwrap_or_else(|| chrono::Utc::now().timestamp())
}

fn closed_until(host: &str) -> Option<i64> {
    let now = now();
    let mut closed = lock(&CLOSED);
    closed.retain(|(_, until)| *until > now);
    closed.iter().find(|(h, _)| h == host).map(|(_, u)| *u)
}

fn close(host: &str, until: i64) {
    let mut closed = lock(&CLOSED);
    closed.retain(|(h, _)| h != host);
    closed.push((host.to_owned(), until));
}

fn unclose(host: &str) {
    lock(&CLOSED).retain(|(h, _)| h != host);
}

fn clock_time(unix: i64) -> String {
    chrono::DateTime::from_timestamp(unix, 0)
        .map(|d| d.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default()
}

/// The Unix second a `Retry-After` names: seconds from `now`, or an HTTP date.
fn retry_after(r: &Response, now: i64) -> Option<i64> {
    let v = r.header("retry-after")?.trim();
    if let Ok(seconds) = v.parse::<i64>() {
        return Some(now + seconds.max(0));
    }
    chrono::DateTime::parse_from_rfc2822(v)
        .ok()
        .map(|d| d.timestamp())
}

/// The reset second of a rate limit: GitHub's `x-ratelimit-reset`, GitLab's `ratelimit-reset` (both Unix seconds).
fn reset(r: &Response) -> Option<i64> {
    r.header("x-ratelimit-reset")
        .or_else(|| r.header("ratelimit-reset"))?
        .trim()
        .parse()
        .ok()
}

fn remaining(r: &Response) -> Option<i64> {
    r.header("x-ratelimit-remaining")
        .or_else(|| r.header("ratelimit-remaining"))?
        .trim()
        .parse()
        .ok()
}

/// The answer says the host's allowance is spent: the second it comes back, when that is ahead.
fn used_up(r: &Response, now: i64) -> Option<i64> {
    (remaining(r)? == 0).then_some(())?;
    reset(r).filter(|until| *until > now)
}

/// The answer is a rate limit (429; a 403 that says so; a 503 that names a `Retry-After`): the second to come
/// back. A GitHub 403 is a limit, not a refusal, when its allowance is spent or it names the secondary limit.
fn limit(r: &Response, now: i64) -> Option<i64> {
    let asked = retry_after(r, now);
    let spent = used_up(r, now);
    let limited = match r.status {
        429 => true,
        403 => {
            asked.is_some()
                || spent.is_some()
                || (remaining(r) == Some(0))
                || r.text().to_ascii_lowercase().contains("rate limit")
        }
        503 => asked.is_some(),
        _ => false,
    };
    limited.then(|| asked.or(spent).unwrap_or(now + DEFAULT_WAIT))
}

fn run(request: &Request, progress: Option<&dyn Fn(f32)>) -> Result<Response, HttpError> {
    let fake = lock(&TRANSPORT).clone();
    if let Some(transport) = fake {
        return transport(request);
    }
    let host = request.host();
    let mut child = Command::new("curl")
        .args(arguments(request, progress.is_some()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| HttpError::new(Kind::NoCurl, &host))?;
    // The token goes in on stdin, not on the command line, where `ps` would show it.
    let config = config(request);
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(config.as_bytes())
            .map_err(|e| HttpError::from_exit(-1, &host, &e.to_string()))?;
    }
    let mut out = child.stdout.take();
    let answer = std::thread::spawn(move || {
        let mut raw = Vec::new();
        if let Some(out) = out.as_mut() {
            let _ = out.read_to_end(&mut raw);
        }
        raw
    });
    let said = read_stderr(child.stderr.take(), progress);
    let status = child
        .wait()
        .map_err(|e| HttpError::from_exit(-1, &host, &e.to_string()))?;
    let raw = answer.join().unwrap_or_default();
    if !status.success() {
        return Err(HttpError::from_exit(
            status.code().unwrap_or(-1),
            &host,
            &said,
        ));
    }
    parse(&raw, request.follow)
        .ok_or_else(|| HttpError::from_exit(-1, &host, "the answer had no status line"))
}

/// curl's arguments: what is not secret. Everything else is the config on stdin.
fn arguments(request: &Request, bar: bool) -> Vec<String> {
    let mut args: Vec<String> = [
        if bar { "-S" } else { "-sS" },
        "--connect-timeout",
        "5",
        "--max-time",
    ]
    .iter()
    .map(|a| (*a).to_owned())
    .collect();
    args.push(request.timeout.as_secs().max(1).to_string());
    args.extend(["--compressed", "-D", "-"].map(str::to_owned));
    if bar {
        args.push("-#".to_owned());
    }
    if let Some(bytes) = request.max_filesize {
        args.extend(["--max-filesize".to_owned(), bytes.to_string()]);
    }
    args.extend(["-K", "-"].map(str::to_owned));
    args
}

/// The request as curl's config file.
fn config(request: &Request) -> String {
    let mut c = format!("url = \"{}\"\n", quote(&request.url));
    if request.method != "GET" {
        c.push_str(&format!("request = \"{}\"\n", quote(&request.method)));
    }
    for (name, value) in &request.headers {
        c.push_str(&format!(
            "header = \"{}\"\n",
            quote(&format!("{name}: {value}"))
        ));
    }
    for (name, value) in &request.form {
        c.push_str(&format!(
            "data-urlencode = \"{}={}\"\n",
            quote(name),
            quote(value)
        ));
    }
    if let Some(body) = &request.body {
        c.push_str(&format!("data-raw = \"{}\"\n", quote(body)));
    }
    if let Some((field, file)) = &request.upload {
        c.push_str(&format!(
            "form = \"{}=@\\\"{}\\\"\"\n",
            quote(field),
            quote(&file.to_string_lossy())
        ));
    }
    if request.follow {
        c.push_str("location\n");
    }
    if let Some(file) = &request.output {
        c.push_str(&format!(
            "output = \"{}\"\n",
            quote(&file.to_string_lossy())
        ));
    }
    c
}

/// A value inside a curl config file's double quotes.
pub fn quote(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// curl's stderr to its end: its words, and the bar's percentages told to `progress`.
fn read_stderr(err: Option<std::process::ChildStderr>, progress: Option<&dyn Fn(f32)>) -> String {
    let Some(mut err) = err else {
        return String::new();
    };
    let mut said = String::new();
    let (mut buf, mut line) = ([0u8; 256], String::new());
    while let Ok(n) = err.read(&mut buf) {
        if n == 0 {
            break;
        }
        for c in String::from_utf8_lossy(&buf[..n]).chars() {
            if c != '\r' && c != '\n' {
                line.push(c);
                continue;
            }
            match percent(&line) {
                Some(p) => {
                    if let Some(progress) = progress {
                        progress(p / 100.);
                    }
                }
                None if !line.trim().is_empty() => {
                    if !said.is_empty() {
                        said.push(' ');
                    }
                    said.push_str(line.trim());
                }
                None => {}
            }
            line.clear();
        }
    }
    said
}

/// How far curl's bar says it is, `45.6` of `###   45.6%`.
fn percent(line: &str) -> Option<f32> {
    let token = line.split_whitespace().last()?;
    token.strip_suffix('%')?.parse().ok()
}

/// curl's `-D -` output followed by the body: the header blocks (one for each redirect hop that was followed, and
/// any `100 Continue`), then the last block's body. A body that itself starts with `HTTP/` is safe: a block is
/// only skipped while its status says another follows.
fn parse(raw: &[u8], follow: bool) -> Option<Response> {
    let mut at = 0;
    loop {
        let rest = raw.get(at..)?;
        let end = find(rest, b"\r\n\r\n")
            .map(|i| (i, 4))
            .or_else(|| find(rest, b"\n\n").map(|i| (i, 2)))?;
        let head = String::from_utf8_lossy(&rest[..end.0]).into_owned();
        let mut lines = head.lines();
        let status: u16 = lines
            .next()?
            .strip_prefix("HTTP/")?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()?;
        let headers: Vec<(String, String)> = lines
            .filter_map(|l| l.split_once(':'))
            .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_owned()))
            .collect();
        at += end.0 + end.1;
        let hop = status / 100 == 1
            || (follow && status / 100 == 3 && headers.iter().any(|(n, _)| n == "location"));
        if !hop {
            return Some(Response {
                status,
                headers,
                body: raw[at..].to_vec(),
            });
        }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// `host` of `https://user@host:port/path`.
fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    authority.split(':').next().unwrap_or(authority).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_redirect_has_two_blocks_and_the_last_one_answers() {
        let raw = b"HTTP/2 301\r\nlocation: https://b.example/x\r\n\r\nHTTP/2 200\r\nContent-Type: application/json\r\nETag: \"a\"\r\n\r\n{\"ok\":true}";
        let r = parse(raw, true).unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.header("Content-Type"), Some("application/json"));
        assert_eq!(r.header("etag"), Some("\"a\""));
        assert_eq!(r.text(), "{\"ok\":true}");
    }

    #[test]
    fn a_redirect_not_followed_is_the_answer() {
        let raw = b"HTTP/1.1 302 Found\r\nLocation: /elsewhere\r\n\r\n";
        let r = parse(raw, false).unwrap();
        assert_eq!((r.status, r.header("location")), (302, Some("/elsewhere")));
    }

    #[test]
    fn a_continue_block_is_skipped_and_a_body_that_looks_like_a_head_is_kept() {
        let raw =
            b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\n\r\nHTTP/1.1 500 in the body\r\n\r\n";
        let r = parse(raw, true).unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.text(), "HTTP/1.1 500 in the body\r\n\r\n");
    }

    #[test]
    fn no_status_line_is_no_answer() {
        assert!(parse(b"", true).is_none());
        assert!(parse(b"not http\r\n\r\n", true).is_none());
    }

    #[test]
    fn each_curl_exit_is_a_sentence() {
        let said = |code| HttpError::from_exit(code, "git.example.com", "").to_string();
        assert_eq!(said(6), "Can't reach git.example.com — is the VPN on?");
        assert_eq!(said(7), "Can't reach git.example.com — is the VPN on?");
        assert_eq!(said(28), "git.example.com did not answer in time");
        for code in [35, 51, 58, 60] {
            assert_eq!(
                said(code),
                "Can't make a secure connection to git.example.com"
            );
        }
        assert_eq!(said(56), "The call to git.example.com failed");
        assert_eq!(
            HttpError::from_exit(63, "h", " Maximum file size exceeded ").to_string(),
            "Maximum file size exceeded"
        );
        assert_eq!(HttpError::from_exit(28, "h", "").kind, Kind::Timeout);
    }

    #[test]
    fn a_host_is_read_off_the_address() {
        assert_eq!(
            host_of("https://git.example.com/api/v4/x"),
            "git.example.com"
        );
        assert_eq!(host_of("https://me@host.io:8443/a?b=c"), "host.io");
        assert_eq!(host_of("http://localhost:3000"), "localhost");
    }

    #[test]
    fn a_value_is_quoted_for_a_curl_config() {
        assert_eq!(quote("a\"b\\c\nd\te"), "a\\\"b\\\\c\\nd\\te");
    }

    #[test]
    fn the_config_carries_the_token_and_the_arguments_do_not() {
        let request = Request::get("https://h.example/api")
            .method("POST")
            .header("PRIVATE-TOKEN", "sekrit")
            .body("{\"a\":1}")
            .follow();
        let config = config(&request);
        assert!(config.contains("header = \"PRIVATE-TOKEN: sekrit\""));
        assert!(config.contains("request = \"POST\""));
        assert!(config.contains("data-raw = \"{\\\"a\\\":1}\""));
        assert!(config.contains("location"));
        let args = arguments(&request, false).join(" ");
        assert!(!args.contains("sekrit"));
        assert!(args.contains("--connect-timeout 5"));
        assert!(args.contains("--compressed"));
        assert!(args.contains("-D -"));
        assert!(args.ends_with("-K -"));
    }

    fn answer(status: u16, headers: &[(&str, &str)], body: &str) -> Result<Response, HttpError> {
        Ok(Response {
            status,
            headers: headers
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            body: body.as_bytes().to_vec(),
        })
    }

    /// A transport that answers from `script` in turn and counts the calls.
    fn scripted(script: Vec<Result<Response, HttpError>>) -> (TransportGuard, Arc<Mutex<usize>>) {
        let calls = Arc::new(Mutex::new(0));
        let seen = calls.clone();
        let script = Mutex::new(script.into_iter());
        let guard = with_transport(move |_| {
            *seen.lock().unwrap() += 1;
            script
                .lock()
                .unwrap()
                .next()
                .unwrap_or_else(|| reply(200, "late"))
        });
        guard.at(1_000_000);
        (guard, calls)
    }

    #[test]
    fn a_503_then_a_200_is_one_retry() {
        let (guard, calls) = scripted(vec![reply(503, ""), reply(200, "ok")]);
        let r = send(&Request::get("https://h.test/a")).unwrap();
        assert_eq!((r.status, *calls.lock().unwrap()), (200, 2));
        let pauses = guard.slept();
        assert_eq!(pauses.len(), 1);
        let s = pauses[0].as_secs_f64();
        assert!((0.37..=0.63).contains(&s), "{s}");
    }

    #[test]
    fn a_get_is_tried_three_times_at_most() {
        let (guard, calls) = scripted(vec![
            reply(502, ""),
            reply(504, ""),
            reply(503, ""),
            reply(200, ""),
        ]);
        let r = send(&Request::get("https://h.test/a")).unwrap();
        assert_eq!((r.status, *calls.lock().unwrap()), (503, 3));
        let pauses = guard.slept();
        assert!(pauses[1] > pauses[0]);
    }

    #[test]
    fn offline_and_timeout_are_retried_too() {
        let (_g, calls) = scripted(vec![
            Err(HttpError::new(Kind::Offline, "h.test")),
            Err(HttpError::new(Kind::Timeout, "h.test")),
            reply(200, "ok"),
        ]);
        assert_eq!(send(&Request::get("https://h.test/a")).unwrap().status, 200);
        assert_eq!(*calls.lock().unwrap(), 3);
    }

    #[test]
    fn a_post_on_503_is_not_retried() {
        let (guard, calls) = scripted(vec![reply(503, ""), reply(200, "")]);
        let r = send(&Request::get("https://h.test/a").method("POST")).unwrap();
        assert_eq!((r.status, *calls.lock().unwrap()), (503, 1));
        assert!(guard.slept().is_empty());
        drop(guard);
        let (_g, calls) = scripted(vec![
            Err(HttpError::new(Kind::Offline, "h.test")),
            reply(200, ""),
        ]);
        assert!(send(&Request::get("https://h.test/a").method("DELETE")).is_err());
        assert_eq!(*calls.lock().unwrap(), 1);
    }

    #[test]
    fn retry_after_is_waited_out_on_the_injected_clock() {
        let (guard, calls) = scripted(vec![
            answer(429, &[("retry-after", "2")], ""),
            reply(200, "ok"),
        ]);
        let r = send(&Request::get("https://h.test/a")).unwrap();
        assert_eq!((r.status, *calls.lock().unwrap()), (200, 2));
        assert_eq!(guard.slept(), vec![Duration::from_secs(2)]);
        // The host stayed open.
        assert!(send(&Request::get("https://h.test/b")).is_ok());
    }

    #[test]
    fn a_long_retry_after_closes_the_host_and_the_gate_blocks_the_next_call() {
        let (guard, calls) = scripted(vec![
            answer(429, &[("retry-after", "120")], ""),
            reply(200, "ok"),
        ]);
        let e = send(&Request::get("https://h.test/a")).unwrap_err();
        assert_eq!((e.kind, e.until), (Kind::Limited, 1_000_120));
        assert!(e.to_string().starts_with("h.test asks to wait until "));
        // The second call is not made.
        let e = send(&Request::get("https://h.test/b")).unwrap_err();
        assert_eq!(e.kind, Kind::Limited);
        assert_eq!(*calls.lock().unwrap(), 1);
        // Another host is not held; the same one opens when the time comes.
        assert!(send(&Request::get("https://other.test/b")).is_ok());
        guard.at(1_000_121);
        assert!(send(&Request::get("https://h.test/b")).is_ok());
    }

    #[test]
    fn a_github_403_with_no_allowance_left_is_a_limit_not_a_refusal() {
        let spent = [
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", "1003600"),
        ];
        let (_g, _) = scripted(vec![answer(403, &spent, "API rate limit exceeded")]);
        let e = send(&Request::get("https://api.github.com/x")).unwrap_err();
        assert_eq!((e.kind, e.until), (Kind::Limited, 1_003_600));
        // A plain 403 stays a 403.
        drop(_g);
        let (_g, _) = scripted(vec![answer(
            403,
            &[("x-ratelimit-remaining", "4000")],
            "Forbidden",
        )]);
        assert_eq!(
            send(&Request::get("https://api.github.com/x"))
                .unwrap()
                .status,
            403
        );
    }

    #[test]
    fn the_secondary_limit_message_counts_and_waits_a_minute() {
        let (_g, _) = scripted(vec![answer(
            403,
            &[],
            "You have exceeded a secondary rate limit.",
        )]);
        let e = send(&Request::get("https://api.github.com/x")).unwrap_err();
        assert_eq!((e.kind, e.until), (Kind::Limited, 1_000_060));
    }

    #[test]
    fn a_spent_allowance_on_a_good_answer_closes_the_host_after_it() {
        let spent = [("ratelimit-remaining", "0"), ("ratelimit-reset", "1000500")];
        let (_g, calls) = scripted(vec![answer(200, &spent, "[]"), reply(200, "[]")]);
        assert_eq!(
            send(&Request::get("https://gl.test/a")).unwrap().status,
            200
        );
        assert_eq!(
            send(&Request::get("https://gl.test/b")).unwrap_err().kind,
            Kind::Limited
        );
        assert_eq!(*calls.lock().unwrap(), 1);
    }

    #[test]
    fn retry_after_may_be_a_date() {
        let r = Response {
            status: 503,
            headers: vec![("retry-after".into(), "Wed, 21 Oct 2015 07:28:00 GMT".into())],
            body: Vec::new(),
        };
        assert_eq!(retry_after(&r, 0), Some(1_445_412_480));
    }

    #[test]
    fn a_transport_answers_in_place_of_curl_and_records() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        let _t = with_transport(move |r| {
            lock(&log).push(format!("{} {}", r.method, r.url));
            reply(200, "[1,2]")
        });
        let r = send(&Request::get("https://h.example/list")).unwrap();
        assert_eq!(r.text(), "[1,2]");
        assert_eq!(*lock(&seen), ["GET https://h.example/list"]);
    }

    /// Real curl: `cargo test -p artoo-core live_get -- --ignored`.
    #[test]
    #[ignore]
    fn live_get_follows_a_redirect_and_reads_headers() {
        let r = send(
            &Request::get("https://github.com/Thernal/nothing-here-xyz")
                .header("User-Agent", "Artoo")
                .follow()
                .timeout(15),
        )
        .unwrap();
        assert_eq!(r.status, 404);
        let r = send(&Request::get("https://api.github.com/zen").header("User-Agent", "Artoo"))
            .unwrap();
        assert_eq!(r.status, 200);
        assert!(r.header("x-ratelimit-limit").is_some());
        let e = send(&Request::get("https://no-such-host.invalid/").timeout(5)).unwrap_err();
        assert_eq!(e.kind, Kind::Offline);
    }
}
