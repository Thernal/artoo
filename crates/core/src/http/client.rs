//! The in-process client behind `http::send`: one pooled agent for the whole app, so a second call to a host reuses the
//! connection (no new TCP and TLS handshake), answers are gzip-decoded, and the system's trust store and proxies apply
//! as they did for curl: a keychain-trusted company CA works over the VPN, and `HTTPS_PROXY` / `NO_PROXY` are read.
//!
//! The calls block, so they run on the callers' background threads; a small bounded pool (`SLOTS`) keeps a burst of
//! them from opening dozens of connections at once.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

use ureq::config::RedirectAuthHeaders;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
use ureq::{Agent, AsSendBody, SendBody};

use super::{HttpError, Kind, Request, Response, host_of};

/// How many calls run at once; the rest wait their turn.
const SLOTS: usize = 8;
/// Connecting alone gives up after this long.
const CONNECT: Duration = Duration::from_secs(5);
/// The most a body kept in memory may be when the request names no limit.
const MEMORY_LIMIT: u64 = 256 * 1024 * 1024;

static AGENT: OnceLock<Agent> = OnceLock::new();
static BUSY: Mutex<usize> = Mutex::new(0);
static FREED: Condvar = Condvar::new();

fn agent() -> &'static Agent {
    AGENT.get_or_init(|| {
        let tls = TlsConfig::builder()
            .provider(TlsProvider::Rustls)
            .root_certs(RootCerts::PlatformVerifier)
            .build();
        let config = Agent::config_builder()
            .tls_config(tls)
            .http_status_as_error(false)
            .timeout_connect(Some(CONNECT))
            // curl kept `Authorization` on the same host and off another one; so does this.
            .redirect_auth_headers(RedirectAuthHeaders::SameHost)
            .max_idle_connections(32)
            .max_idle_connections_per_host(SLOTS)
            .max_idle_age(Duration::from_secs(60))
            .user_agent("Artoo")
            .build();
        Agent::new_with_config(config)
    })
}

/// One of the `SLOTS` places; given back when dropped.
struct Slot;

impl Slot {
    fn take() -> Slot {
        let mut busy = BUSY.lock().unwrap_or_else(|p| p.into_inner());
        while *busy >= SLOTS {
            busy = FREED.wait(busy).unwrap_or_else(|p| p.into_inner());
        }
        *busy += 1;
        Slot
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        *BUSY.lock().unwrap_or_else(|p| p.into_inner()) -= 1;
        FREED.notify_one();
    }
}

/// Counts the bytes that go out and tells `progress` how far they are (0.0–1.0).
struct Counting<'a> {
    inner: std::io::Cursor<Vec<u8>>,
    progress: Option<&'a dyn Fn(f32)>,
}

impl Read for Counting<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if let Some(progress) = self.progress {
            let total = self.inner.get_ref().len().max(1) as f32;
            progress((self.inner.position() as f32 / total).min(1.));
        }
        Ok(n)
    }
}

pub fn run(request: &Request, progress: Option<&dyn Fn(f32)>) -> Result<Response, HttpError> {
    let host = host_of(&request.url);
    let failed = |detail: String| HttpError {
        kind: Kind::Failed,
        host: host.clone(),
        code: None,
        detail,
        until: 0,
    };
    let (content_type, body) = outgoing(request).map_err(failed)?;

    let mut builder = ureq::http::Request::builder()
        .method(request.method.as_str())
        .uri(request.url.as_str());
    let sets = |name: &str| (request.headers.iter()).any(|(n, _)| n.eq_ignore_ascii_case(name));
    for (name, value) in &request.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    if let (Some(kind), false) = (&content_type, sets("content-type")) {
        builder = builder.header("Content-Type", kind.as_str());
    }
    let sending = body.is_some();
    if sending {
        let length = body.as_ref().map_or(0, Vec::len);
        builder = builder.header("Content-Length", length.to_string());
    }
    let head = builder.body(()).map_err(|e| failed(e.to_string()))?;
    let mut reader = Counting {
        inner: std::io::Cursor::new(body.unwrap_or_default()),
        progress: progress.filter(|_| request.upload.is_some()),
    };
    let _slot = Slot::take();
    let answer = if sending {
        go(head.map(|()| SendBody::from_reader(&mut reader)), request)
    } else {
        go(head, request)
    }
    .map_err(|e| error(&e, &host))?;
    let (parts, body) = answer.into_parts();
    let headers = (parts.headers.iter())
        .map(|(n, v)| {
            (
                n.as_str().to_ascii_lowercase(),
                String::from_utf8_lossy(v.as_bytes()).trim().to_owned(),
            )
        })
        .collect();
    let limit = request.max_filesize.unwrap_or(MEMORY_LIMIT);
    let mut body = body.into_with_config().limit(limit).reader();
    let bytes = match &request.output {
        Some(file) => {
            save(&mut body, file).map_err(|e| match e {
                Saved::Net(e) => error(&e, &host),
                Saved::Disk(detail) => failed(detail),
            })?;
            Vec::new()
        }
        None => {
            let mut bytes = Vec::new();
            body.read_to_end(&mut bytes)
                .map_err(|e| from_read(&e, &host))?;
            bytes
        }
    };
    Ok(Response {
        status: parts.status.as_u16(),
        headers,
        body: bytes,
    })
}

/// The call itself, on the shared agent, with this request's timeout and redirect rule.
fn go<S: AsSendBody>(
    head: ureq::http::Request<S>,
    request: &Request,
) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    let agent = agent();
    let call = agent
        .configure_request(head)
        .timeout_global(Some(request.timeout.max(Duration::from_secs(1))))
        .max_redirects(if request.follow { 10 } else { 0 })
        .max_redirects_will_error(false)
        .build();
    agent.run(call)
}

/// The body as it goes out, and its content type when the call chose one: JSON text as given, form fields
/// urlencoded, or a file as one multipart field (curl's `--form`, which also named the file).
fn outgoing(request: &Request) -> Result<(Option<String>, Option<Vec<u8>>), String> {
    if let Some((field, path)) = &request.upload {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = (path.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let boundary = format!("----artoo{:016x}", fingerprint(&bytes));
        let mut body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
            escape(field),
            escape(&name),
            mime_of(path),
        )
        .into_bytes();
        body.extend_from_slice(&bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        return Ok((
            Some(format!("multipart/form-data; boundary={boundary}")),
            Some(body),
        ));
    }
    if !request.form.is_empty() {
        let pairs: Vec<String> = (request.form.iter())
            .map(|(n, v)| format!("{}={}", encode(n), encode(v)))
            .collect();
        let kind = "application/x-www-form-urlencoded".to_owned();
        return Ok((Some(kind), Some(pairs.join("&").into_bytes())));
    }
    if let Some(body) = &request.body {
        // curl's `--data-raw` said this when the caller gave no type.
        let kind = "application/x-www-form-urlencoded".to_owned();
        return Ok((Some(kind), Some(body.clone().into_bytes())));
    }
    // A write with no body still says so; curl sent `Content-Length: 0` for a POST.
    let writes = matches!(request.method.as_str(), "POST" | "PUT" | "PATCH");
    Ok((None, writes.then(Vec::new)))
}

/// A header value inside quotes.
fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "%22")
        .replace(['\r', '\n'], " ")
}

/// Percent-encoded as a form field (spaces as `+`, curl's `--data-urlencode` used `%20`; both are read the same).
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn mime_of(path: &Path) -> &'static str {
    match (path.extension().and_then(|e| e.to_str()))
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("mp4") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("pdf") => "application/pdf",
        Some("txt" | "log") => "text/plain",
        Some("json") => "application/json",
        Some("zip") => "application/zip",
        _ => "application/octet-stream",
    }
}

/// A boundary that does not occur in the file: derived from its bytes.
fn fingerprint(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.len().hash(&mut hasher);
    bytes
        .iter()
        .step_by(bytes.len() / 64 + 1)
        .for_each(|b| b.hash(&mut hasher));
    std::time::SystemTime::now().hash(&mut hasher);
    hasher.finish()
}

enum Saved {
    Net(ureq::Error),
    Disk(String),
}

/// The body into `file`, written beside it and renamed, so a failed download leaves no half file.
fn save(body: &mut dyn Read, file: &Path) -> Result<(), Saved> {
    let disk = |e: std::io::Error| Saved::Disk(format!("{}: {e}", file.display()));
    let mut part = file.as_os_str().to_owned();
    part.push(".dl");
    let part = std::path::PathBuf::from(part);
    let mut out = std::fs::File::create(&part).map_err(disk)?;
    let mut buf = vec![0u8; 64 * 1024];
    let copied = loop {
        match body.read(&mut buf) {
            Ok(0) => break Ok(()),
            Ok(n) => {
                if let Err(e) = out.write_all(&buf[..n]) {
                    break Err(disk(e));
                }
            }
            Err(e) => break Err(Saved::Net(read_error(e))),
        }
    };
    let done = copied.and_then(|()| out.flush().map_err(disk));
    drop(out);
    match done {
        Ok(()) => std::fs::rename(&part, file).map_err(disk),
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

/// The `ureq` error a failed read of the body carries (it travels inside an `io::Error`).
fn read_error(e: std::io::Error) -> ureq::Error {
    match e.downcast::<ureq::Error>() {
        Ok(inner) => inner,
        Err(e) => ureq::Error::Io(e),
    }
}

fn from_read(e: &std::io::Error, host: &str) -> HttpError {
    let copy = std::io::Error::new(e.kind(), e.to_string());
    let inner = match e.get_ref().and_then(|i| i.downcast_ref::<ureq::Error>()) {
        Some(inner) => return error(inner, host),
        None => copy,
    };
    error(&ureq::Error::Io(inner), host)
}

/// The words and kind of what went wrong before an answer.
fn error(e: &ureq::Error, host: &str) -> HttpError {
    use ureq::Error as E;
    let kind = match e {
        E::Timeout(_) => Kind::Timeout,
        E::HostNotFound | E::ConnectionFailed => Kind::Offline,
        E::Io(io) => match io.kind() {
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => Kind::Timeout,
            std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::HostUnreachable
            | std::io::ErrorKind::NetworkUnreachable
            | std::io::ErrorKind::UnexpectedEof
            | std::io::ErrorKind::BrokenPipe => Kind::Offline,
            // A name that did not resolve has no kind of its own (`getaddrinfo`'s words).
            _ if io.to_string().contains("lookup address") => Kind::Offline,
            _ => Kind::Failed,
        },
        E::Tls(_) | E::Pem(_) | E::Rustls(_) => Kind::Tls,
        _ => Kind::Failed,
    };
    let detail = match e {
        E::BodyExceedsLimit(_) => "Maximum file size exceeded".to_owned(),
        other => other.to_string(),
    };
    HttpError {
        kind,
        host: host.to_owned(),
        code: None,
        detail,
        until: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// What the server saw of one request.
    struct Seen {
        head: String,
        body: Vec<u8>,
    }

    /// A local server: `answer` makes each reply (head lines after the status, body) from the request; counts the
    /// connections it was given, and keeps each connection open for further requests.
    fn serve(
        answer: impl Fn(&Seen) -> (String, Vec<u8>) + Send + Sync + 'static,
    ) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let connections = Arc::new(AtomicUsize::new(0));
        let count = connections.clone();
        let answer = Arc::new(answer);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                count.fetch_add(1, Ordering::SeqCst);
                let answer = answer.clone();
                std::thread::spawn(move || talk(stream, &*answer));
            }
        });
        (base, connections)
    }

    fn talk(stream: TcpStream, answer: &dyn Fn(&Seen) -> (String, Vec<u8>)) {
        let mut out = stream.try_clone().unwrap();
        let mut input = BufReader::new(stream);
        loop {
            let mut head = String::new();
            loop {
                let mut line = String::new();
                if input.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                head.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let length = head
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse().unwrap())
                })
                .unwrap_or(0usize);
            let mut body = vec![0; length];
            input.read_exact(&mut body).unwrap();
            let (lines, payload) = answer(&Seen { head, body });
            let reply = format!("{lines}\r\nContent-Length: {}\r\n\r\n", payload.len());
            if out.write_all(reply.as_bytes()).is_err() || out.write_all(&payload).is_err() {
                return;
            }
        }
    }

    fn ok(body: &str) -> (String, Vec<u8>) {
        ("HTTP/1.1 200 OK".to_owned(), body.as_bytes().to_vec())
    }

    fn get(url: &str) -> Request {
        Request::get(url).timeout(10)
    }

    #[test]
    fn a_second_call_reuses_the_connection() {
        let (base, connections) = serve(|_| ok("[]"));
        for _ in 0..3 {
            let r = run(&get(&format!("{base}/a")), None).unwrap();
            assert_eq!((r.status, r.text().as_str()), (200, "[]"));
        }
        assert_eq!(connections.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn headers_come_back_in_lower_case_and_a_post_sends_its_body_and_type() {
        let (base, _) = serve(|seen| {
            let head = seen.head.to_ascii_lowercase();
            assert!(head.starts_with("post /x "), "{head}");
            assert!(head.contains("private-token: sekrit"));
            assert!(head.contains("content-type: application/json"));
            assert!(head.contains("accept-encoding: gzip"));
            assert_eq!(seen.body, b"{\"a\":1}");
            (
                "HTTP/1.1 201 Created\r\nX-Poll-Interval: 60".to_owned(),
                b"made".to_vec(),
            )
        });
        let request = get(&format!("{base}/x"))
            .method("POST")
            .header("PRIVATE-TOKEN", "sekrit")
            .header("Content-Type", "application/json")
            .body("{\"a\":1}");
        let r = run(&request, None).unwrap();
        assert_eq!(
            (r.status, r.text().as_str(), r.poll_interval()),
            (201, "made", Some(60))
        );
    }

    #[test]
    fn a_body_with_no_type_goes_as_a_form_as_curl_sent_it() {
        let (base, _) = serve(|seen| {
            assert!(
                seen.head
                    .to_ascii_lowercase()
                    .contains("content-type: application/x-www-form-urlencoded")
            );
            ok("")
        });
        run(&get(&format!("{base}/")).method("POST").body("a=1"), None).unwrap();
    }

    #[test]
    fn form_fields_are_urlencoded() {
        let (base, _) = serve(|seen| {
            assert_eq!(String::from_utf8_lossy(&seen.body), "a=x%20y%26z&b=%C3%BC");
            ok("")
        });
        let request = get(&format!("{base}/"))
            .method("POST")
            .form(&[("a", "x y&z".into()), ("b", "ü".into())]);
        run(&request, None).unwrap();
    }

    #[test]
    fn a_post_with_no_body_says_so() {
        let (base, _) = serve(|seen| {
            assert!(
                seen.head.to_ascii_lowercase().contains("content-length: 0"),
                "{}",
                seen.head
            );
            ok("")
        });
        run(&get(&format!("{base}/")).method("PUT"), None).unwrap();
    }

    #[test]
    fn a_gzip_answer_is_decoded() {
        let (base, _) = serve(|_| {
            let mut z = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            z.write_all(b"{\"zipped\":true}").unwrap();
            (
                "HTTP/1.1 200 OK\r\nContent-Encoding: gzip".to_owned(),
                z.finish().unwrap(),
            )
        });
        assert_eq!(
            run(&get(&format!("{base}/")), None).unwrap().text(),
            "{\"zipped\":true}"
        );
    }

    #[test]
    fn a_file_goes_as_one_multipart_field_and_progress_reaches_the_end() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("shot.png");
        std::fs::write(&file, [1u8, 2, 3, 255, 0]).unwrap();
        let (base, _) = serve(|seen| {
            let head = seen.head.to_ascii_lowercase();
            assert!(head.contains("content-type: multipart/form-data; boundary="));
            let text = String::from_utf8_lossy(&seen.body).into_owned();
            assert!(
                text.contains("name=\"file\"; filename=\"shot.png\""),
                "{text}"
            );
            assert!(text.contains("Content-Type: image/png"));
            ok("{\"url\":\"/uploads/x\"}")
        });
        let last = std::sync::Mutex::new(0.0f32);
        let request = get(&format!("{base}/up"))
            .method("POST")
            .upload("file", file);
        let r = run(&request, Some(&|p| *last.lock().unwrap() = p)).unwrap();
        assert_eq!(r.text(), "{\"url\":\"/uploads/x\"}");
        assert_eq!(*last.lock().unwrap(), 1.0);
    }

    #[test]
    fn a_redirect_is_followed_only_when_asked() {
        let (base, _) = serve(|seen| {
            if seen.head.starts_with("GET /old ") {
                (
                    "HTTP/1.1 302 Found\r\nLocation: /new".to_owned(),
                    Vec::new(),
                )
            } else {
                ok("there")
            }
        });
        let stay = run(&get(&format!("{base}/old")), None).unwrap();
        assert_eq!((stay.status, stay.header("location")), (302, Some("/new")));
        let go = run(&get(&format!("{base}/old")).follow(), None).unwrap();
        assert_eq!((go.status, go.text().as_str()), (200, "there"));
    }

    #[test]
    fn authorization_stays_off_another_host_on_a_redirect() {
        let (other, _) = serve(|seen| {
            assert!(
                !seen.head.to_ascii_lowercase().contains("authorization"),
                "{}",
                seen.head
            );
            ok("bytes")
        });
        let target = other.replace("127.0.0.1", "localhost");
        let (base, _) = serve(move |_| {
            (
                format!("HTTP/1.1 302 Found\r\nLocation: {target}/file"),
                Vec::new(),
            )
        });
        let request = get(&format!("{base}/asset"))
            .header("Authorization", "Bearer sekrit")
            .follow();
        assert_eq!(run(&request, None).unwrap().text(), "bytes");
    }

    #[test]
    fn an_answer_can_go_to_a_file_and_the_size_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let (base, _) = serve(|_| ok("0123456789"));
        let file = dir.path().join("out.zip");
        let r = run(&get(&format!("{base}/")).output(file.clone()), None).unwrap();
        assert!(r.body.is_empty());
        assert_eq!(std::fs::read(&file).unwrap(), b"0123456789");
        let small = dir.path().join("small.zip");
        let e = run(
            &get(&format!("{base}/"))
                .output(small.clone())
                .max_filesize(4),
            None,
        )
        .unwrap_err();
        assert_eq!(e.to_string(), "Maximum file size exceeded");
        assert!(!small.exists(), "a failed download leaves no file");
        let e = run(&get(&format!("{base}/")).max_filesize(4), None).unwrap_err();
        assert_eq!(e.kind, Kind::Failed);
    }

    #[test]
    fn a_closed_port_is_offline_and_a_silent_host_times_out() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let e = run(&get(&format!("http://127.0.0.1:{port}/")), None).unwrap_err();
        assert_eq!(e.kind, Kind::Offline);
        let quiet = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", quiet.local_addr().unwrap());
        let e = run(&Request::get(url).timeout(1), None).unwrap_err();
        assert_eq!(e.kind, Kind::Timeout);
    }

    #[test]
    fn an_unknown_name_is_offline() {
        let e = run(&get("http://no-such-host.invalid/"), None).unwrap_err();
        assert_eq!(e.kind, Kind::Offline);
    }

    /// Real network: `cargo test -p artoo-core reused_connection -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn reused_connection_to_github_is_fast() {
        let call = || {
            let t = std::time::Instant::now();
            let r = run(
                &Request::get("https://api.github.com/zen").header("User-Agent", "Artoo"),
                None,
            )
            .unwrap();
            assert_eq!(r.status, 200);
            t.elapsed()
        };
        let first = call();
        let rest: Vec<_> = (0..5).map(|_| call()).collect();
        eprintln!("first {first:?}, then {rest:?}");
    }
}
