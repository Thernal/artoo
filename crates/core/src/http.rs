//! The one HTTP seam: every call the app makes to a host goes through `send`, and nothing else does. The calls are
//! made by one pooled in-process client (`client`): connections are kept and reused, answers are gzip-decoded, the
//! system's trust store and `HTTPS_PROXY` / `NO_PROXY` apply, so the VPN's routes and a keychain-trusted company
//! CA work as they do in the developer's shell. A token travels in the headers, in this process only. Tests replace
//! the whole transport with `with_transport`: they record the requests and answer from canned JSON, no network.

mod client;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

/// What to ask.
#[derive(Clone)]
pub struct Request {
    pub method: String,
    pub url: String,
    /// `(name, value)`; a token goes here, and goes out in this process only.
    pub headers: Vec<(String, String)>,
    /// The body as text (JSON), sent as given.
    pub body: Option<String>,
    /// Fields sent form-urlencoded.
    pub form: Vec<(String, String)>,
    /// A file sent as a multipart field: `(field, path)`.
    pub upload: Option<(String, PathBuf)>,
    /// The answer's body written to this file, not kept in memory (`Response::body` is then empty).
    pub output: Option<PathBuf>,
    /// Follow redirects (`Authorization` stays off another host).
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

    /// `X-Poll-Interval` (GitHub's notifications): the seconds the host asks to wait between polls.
    pub fn poll_interval(&self) -> Option<u64> {
        self.header("x-poll-interval")?.trim().parse().ok()
    }

    /// 200–299.
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// What kind of thing went wrong before any answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The name did not resolve, or the connection was refused: the VPN is off, or the network.
    Offline,
    /// The host did not answer in time .
    Timeout,
    /// The secure connection could not be made .
    Tls,
    /// The host asked to be left alone until `HttpError::until`; the call was not made, or was refused for it.
    Limited,
    /// Anything else the client said, or a file it could not be given.
    Failed,
}

/// A call that got no answer.
#[derive(Debug, Clone)]
pub struct HttpError {
    pub kind: Kind,
    pub host: String,
    /// The client's exit code, when there is one (none today).
    pub code: Option<i32>,
    /// What the client said, trimmed.
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

    /// One line for the app's log: the client's words.
    pub fn log(&self) -> String {
        format!(
            "http: {}",
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
            Kind::Failed if self.detail.is_empty() => write!(f, "The call to {host} failed"),
            Kind::Failed => write!(f, "{}", self.detail),
        }
    }
}

impl std::error::Error for HttpError {}

/// What answers a request in place of the network.
pub type Transport = dyn Fn(&Request) -> Result<Response, HttpError> + Send + Sync;

static TRANSPORT: Mutex<Option<Arc<Transport>>> = Mutex::new(None);
/// Under a test transport: the clock (`None` is the real one) and the pauses taken.
static NOW: Mutex<Option<i64>> = Mutex::new(None);
static SLEPT: Mutex<Vec<Duration>> = Mutex::new(Vec::new());
/// Per host, the Unix second before which it asked not to be called.
static CLOSED: Mutex<Vec<(String, i64)>> = Mutex::new(Vec::new());
/// One test at a time owns the transport.
static CACHE: Mutex<Cache> = Mutex::new(Cache::new());
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
    lock(&CACHE).clear();
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
    cached(request, None)
}

/// `send`, with `progress` told how far a body going out is (0.0–1.0, as the client counts the bytes).
pub fn send_with_progress(
    request: &Request,
    progress: &dyn Fn(f32),
) -> Result<Response, HttpError> {
    cached(request, Some(progress))
}

/// The most body bytes the response cache keeps.
const CACHE_BYTES: usize = 32 * 1024 * 1024;

/// What a GET's answer was, kept to ask "has it changed?" next time.
struct Entry {
    etag: Option<String>,
    modified: Option<String>,
    response: Response,
    used: u64,
}

/// `(url, hash of the request's headers — the token among them)` → the last full answer, in memory, bounded by the
/// bytes of the bodies kept; the least recently used go first.
struct Cache {
    entries: Vec<((String, [u8; 32]), Entry)>,
    bytes: usize,
    tick: u64,
}

impl Cache {
    const fn new() -> Self {
        Cache {
            entries: Vec::new(),
            bytes: 0,
            tick: 0,
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }

    /// The validators kept for `key`, `used` now.
    fn validators(&mut self, key: &(String, [u8; 32])) -> Option<(Option<String>, Option<String>)> {
        self.tick += 1;
        let tick = self.tick;
        let (_, e) = self.entries.iter_mut().find(|(k, _)| k == key)?;
        e.used = tick;
        Some((e.etag.clone(), e.modified.clone()))
    }

    fn get(&self, key: &(String, [u8; 32])) -> Option<&Response> {
        (self.entries.iter().find(|(k, _)| k == key)).map(|(_, e)| &e.response)
    }

    fn put(&mut self, key: (String, [u8; 32]), response: Response, limit: usize) {
        self.drop_key(&key);
        let size = response.body.len();
        if size > limit {
            return;
        }
        let etag = response.header("etag").map(str::to_owned);
        let modified = response.header("last-modified").map(str::to_owned);
        if etag.is_none() && modified.is_none() {
            return;
        }
        while self.bytes + size > limit {
            let Some(oldest) = (self.entries.iter().enumerate())
                .min_by_key(|(_, (_, e))| e.used)
                .map(|(i, _)| i)
            else {
                break;
            };
            let (_, gone) = self.entries.swap_remove(oldest);
            self.bytes -= gone.response.body.len();
        }
        self.tick += 1;
        self.bytes += size;
        self.entries.push((
            key,
            Entry {
                etag,
                modified,
                response,
                used: self.tick,
            },
        ));
    }

    fn drop_key(&mut self, key: &(String, [u8; 32])) {
        if let Some(i) = self.entries.iter().position(|(k, _)| k == key) {
            let (_, gone) = self.entries.swap_remove(i);
            self.bytes -= gone.response.body.len();
        }
    }
}

/// Whether `request` may be answered from the cache: a plain GET read into memory.
fn cacheable(request: &Request) -> bool {
    request.method.eq_ignore_ascii_case("GET") && request.output.is_none()
}

fn cache_key(request: &Request) -> (String, [u8; 32]) {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for (name, value) in &request.headers {
        h.update(name.to_ascii_lowercase().as_bytes());
        h.update([0]);
        h.update(value.as_bytes());
        h.update([1]);
    }
    (request.url.clone(), h.finalize().into())
}

/// `guarded`, with a GET's last answer kept: the next identical call asks `If-None-Match` / `If-Modified-Since`, and
/// a `304` comes back as the kept answer (status 200, the newer headers laid over the kept ones).
fn cached(request: &Request, progress: Option<&dyn Fn(f32)>) -> Result<Response, HttpError> {
    if !cacheable(request) {
        return guarded(request, progress);
    }
    let key = cache_key(request);
    let mut asked = request.clone();
    let known = lock(&CACHE).validators(&key);
    let own = |name: &str| (request.headers.iter()).any(|(n, _)| n.eq_ignore_ascii_case(name));
    let mut conditional = false;
    if let Some((etag, modified)) = known {
        if let (Some(etag), false) = (etag, own("if-none-match")) {
            asked = asked.header("If-None-Match", etag);
            conditional = true;
        }
        if let (Some(m), false) = (modified, own("if-modified-since")) {
            asked = asked.header("If-Modified-Since", m);
            conditional = true;
        }
    }
    let answer = guarded(&asked, progress)?;
    if answer.status == 304 && conditional {
        let kept = lock(&CACHE).get(&key).cloned();
        if let Some(mut kept) = kept {
            for (n, v) in &answer.headers {
                match kept.headers.iter_mut().find(|(k, _)| k == n) {
                    Some(slot) => slot.1 = v.clone(),
                    None => kept.headers.push((n.clone(), v.clone())),
                }
            }
            return Ok(kept);
        }
    }
    if answer.status == 200 {
        lock(&CACHE).put(key, answer.clone(), CACHE_BYTES);
    } else if conditional {
        lock(&CACHE).drop_key(&key);
    }
    Ok(answer)
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
    client::run(request, progress)
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

    fn tagged(status: u16, tag: &str, body: &str) -> Result<Response, HttpError> {
        let mut r = reply(status, body)?;
        r.headers.push(("etag".into(), tag.into()));
        Ok(r)
    }

    #[test]
    fn a_304_returns_the_kept_body() {
        let seen = Arc::new(Mutex::new(Vec::<Option<String>>::new()));
        let log = seen.clone();
        let _g = with_transport(move |r| {
            let sent = (r.headers.iter())
                .find(|(n, _)| n == "If-None-Match")
                .map(|(_, v)| v.clone());
            log.lock().unwrap().push(sent.clone());
            match sent {
                Some(_) => Ok(Response {
                    status: 304,
                    headers: vec![("x-poll-interval".into(), "60".into())],
                    body: Vec::new(),
                }),
                None => tagged(200, "\"v1\"", "[1,2]"),
            }
        });
        let r = Request::get("https://h.example/list").header("Authorization", "Bearer a");
        let first = send(&r).unwrap();
        assert_eq!((first.status, first.text()), (200, "[1,2]".into()));
        let second = send(&r).unwrap();
        assert_eq!((second.status, second.text()), (200, "[1,2]".into()));
        assert_eq!(second.poll_interval(), Some(60));
        assert_eq!(
            *seen.lock().unwrap(),
            vec![None, Some("\"v1\"".to_string())]
        );
    }

    #[test]
    fn another_token_does_not_share_an_answer() {
        let count = Arc::new(Mutex::new(0));
        let n = count.clone();
        let _g = with_transport(move |r| {
            assert!(!r.headers.iter().any(|(n, _)| n == "If-None-Match"));
            *n.lock().unwrap() += 1;
            tagged(200, "\"v1\"", "x")
        });
        send(&Request::get("https://h.example/u").header("Authorization", "Bearer a")).unwrap();
        send(&Request::get("https://h.example/u").header("Authorization", "Bearer b")).unwrap();
        assert_eq!(*count.lock().unwrap(), 2);
    }

    #[test]
    fn the_cache_stays_under_its_bound_and_drops_the_least_recent() {
        let mut c = Cache::new();
        let key = |n: u8| (format!("u{n}"), [n; 32]);
        let body = |n: usize| {
            let mut r = tagged(200, "\"t\"", &"x".repeat(n)).unwrap();
            r.status = 200;
            r
        };
        c.put(key(1), body(40), 100);
        c.put(key(2), body(40), 100);
        c.validators(&key(1)); // 1 is now the more recent
        c.put(key(3), body(40), 100);
        assert!(c.bytes <= 100);
        assert!(c.get(&key(1)).is_some());
        assert!(c.get(&key(2)).is_none());
        assert!(c.get(&key(3)).is_some());
        c.put(key(4), body(101), 100); // larger than the bound: not kept
        assert!(c.get(&key(4)).is_none());
        assert!(c.bytes <= 100);
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
    fn a_transport_answers_in_place_of_the_network_and_records() {
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

    /// The real client: `cargo test -p artoo-core live_get -- --ignored`.
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
