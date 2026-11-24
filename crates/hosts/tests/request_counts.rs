//! What the hosts' actions cost in requests today (audit step 2): the baseline the later steps lower. Each count is
//! measured through `artoo_core::http::with_transport`, which sees every request on any thread — not through
//! `ARTOO_MR_FIXTURE`, which answers before `send`. A step that lowers a count changes its constant here and says
//! so in the audit report's map.
//!
//! The sequences are the callers' own, kept beside the real code they mirror: a list read is
//! `Requests::read_list` (`crates/requests/src/reading.rs`), a row's facts `facts_of` (`crates/requests/src/list.rs`),
//! a page open `Requests::read_open`, a notices read `notices::read` (`crates/requests/src/notices.rs`).

use artoo_core::git::WebRemote;
use artoo_core::http::{self, Request, Response};
use artoo_hosts_api::{Host, Mr, MrState};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, Once};

/// Open requests in the canned list.
const ROWS: u64 = 3;

/// The transport is one per process: the tests that install one take this first and run one after another.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn tokens() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: first thing every test does, before it starts a thread of its own; nothing else here reads the env.
        unsafe {
            std::env::set_var("GITLAB_TOKEN", "test");
            std::env::set_var("GITHUB_TOKEN", "test");
        }
    });
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    GitLab,
    GitHub,
}

/// What answers: the canned world of one host, `notes` comments on every request.
struct World {
    side: Side,
    notes: usize,
}

fn comment(side: Side, id: usize) -> Value {
    match side {
        Side::GitLab => json!({
            "id": format!("d{id}"), "individual_note": true,
            "notes": [{ "id": id, "body": "note", "author": { "username": "ann", "name": "Ann" },
                        "created_at": "2026-11-01T10:00:00Z", "system": false }]
        }),
        Side::GitHub => json!({ "id": id, "body": "note", "user": { "login": "ann" },
                                "created_at": "2026-11-01T10:00:00Z" }),
    }
}

/// The `page=` of a listing's url (the first when it has none).
fn page_of(url: &str) -> usize {
    url.split(['?', '&'])
        .find_map(|p| p.strip_prefix("page="))
        .and_then(|p| p.parse().ok())
        .unwrap_or(1)
}

impl World {
    /// The answer to `r`, or a plain empty one.
    fn answer(&self, r: &Request) -> Response {
        let url = r.url.as_str();
        let body = r.body.as_deref().unwrap_or("");
        let value = match self.side {
            Side::GitLab => self.gitlab(url),
            Side::GitHub => self.github(url, body),
        };
        // Every GET carries a validator, as the hosts' REST reads do; a repeat that sends it is told 304.
        let tag = format!("\"{url}\"");
        let get = r.method == "GET";
        if get
            && r.headers
                .iter()
                .any(|(n, v)| n == "If-None-Match" && *v == tag)
        {
            return Response {
                status: 304,
                ..Response::default()
            };
        }
        match http::reply(200, &value.to_string()) {
            Ok(mut r) => {
                if get {
                    r.headers.push(("etag".into(), tag));
                }
                r
            }
            Err(_) => unreachable!("reply is always Ok"),
        }
    }

    fn gitlab(&self, url: &str) -> Value {
        if url.ends_with("/user") {
            json!({ "username": "me", "name": "Me" })
        } else if url.contains("/merge_requests?state=opened") {
            Value::Array(
                (1..=ROWS)
                    .map(|iid| {
                        json!({ "iid": iid, "state": "opened", "title": "t", "sha": "abc",
                                "updated_at": "2026-11-01T10:00:00Z", "created_at": "2026-11-01T09:00:00Z",
                                "author": { "username": "ann", "name": "Ann" } })
                    })
                    .collect(),
            )
        } else if url.contains("/discussions?") {
            let from = (page_of(url) - 1) * 100;
            Value::Array(
                (from..self.notes.min(from + 100))
                    .map(|i| comment(Side::GitLab, i + 1))
                    .collect(),
            )
        } else if url.contains("/todos?") {
            Value::Array(Vec::new())
        } else if url.contains("/pipelines") {
            json!([{ "status": "success", "web_url": "u", "updated_at": "2026-11-01T10:00:00Z" }])
        } else if url.contains("/approvals") {
            json!({ "approved_by": [], "approvals_required": 1 })
        } else if url.ends_with("%2Fproj") || url.contains("/merge_requests/") {
            json!({ "iid": 1, "state": "opened", "description": "d", "labels": [] })
        } else {
            Value::Array(Vec::new())
        }
    }

    fn github(&self, url: &str, body: &str) -> Value {
        if url.ends_with("/user") {
            json!({ "login": "me", "name": "Me" })
        } else if url.contains("/graphql") && body.contains("pullRequests(") {
            json!({ "data": { "repository": { "pullRequests": {
                "nodes": (1..=ROWS).map(|n| json!({ "number": n, "state": "OPEN", "title": "t",
                    "headRefOid": "abc", "updatedAt": "2026-11-01T10:00:00Z",
                    "createdAt": "2026-11-01T09:00:00Z", "author": { "login": "ann" } })).collect::<Vec<_>>(),
                "pageInfo": { "hasNextPage": false, "endCursor": null } } } } })
        } else if url.contains("/graphql") {
            json!({ "data": { "repository": { "pullRequest": {
                "reviewThreads": { "nodes": [] }, "reviews": { "nodes": [] } } } } })
        } else if url.contains("/issues/") && url.contains("/comments") {
            let page = page_of(url);
            let from = (page - 1) * 100;
            Value::Array(
                (from..self.notes.min(from + 100))
                    .map(|i| comment(Side::GitHub, i + 1))
                    .collect(),
            )
        } else if url.contains("/notifications") {
            Value::Array(Vec::new())
        } else if url.contains("/pulls/") && !url.contains("/reviews") {
            json!({ "number": 1, "state": "open", "head": { "sha": "abc" }, "changed_files": 2,
                    "additions": 3, "deletions": 1, "commits": 1 })
        } else if url.contains("/check-runs") {
            json!({ "check_runs": [] })
        } else if url.contains("/status") {
            json!({ "statuses": [] })
        } else {
            Value::Array(Vec::new())
        }
    }
}

/// The requests `act` makes against `host` on a world of `side`, each as `METHOD url`.
fn counted(
    side: Side,
    notes: usize,
    act: impl FnOnce(&Arc<dyn Host>) -> Vec<Mr>,
) -> (Vec<String>, Vec<Mr>) {
    tokens();
    let world = World { side, notes };
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let guard = http::with_transport(move |r| {
        log.lock().unwrap().push(format!("{} {}", r.method, r.url));
        Ok(world.answer(r))
    });
    let base = match side {
        Side::GitLab => "https://gl.example/team/proj",
        Side::GitHub => "https://github.com/team/proj",
    };
    let host = artoo_hosts::hosts().of(&WebRemote {
        base: base.into(),
        github: side == Side::GitHub,
    });
    let out = act(&host);
    drop(guard);
    let seen = seen.lock().unwrap().clone();
    (seen, out)
}

/// `Requests::read_list`: the list and who the token is (unless known) first, then the members and who could not be
/// reached; the hosts keep the three side reads for 15 minutes.
fn list_read(host: &Arc<dyn Host>, login_known: bool) -> Vec<Mr> {
    if !login_known {
        host.username().ok();
    }
    let list = host.list().unwrap_or_default();
    host.members().unwrap_or_default();
    host.unreached().unwrap_or_default();
    list
}

/// `facts_of` without a local clone: what one row's facts read.
fn row_facts(host: &Arc<dyn Host>, m: &Mr) {
    host.size(m.iid).ok();
    host.pipeline(m.iid).ok();
    host.approvals(m.iid).ok();
    host.threads(m.iid).ok();
    host.versions(m.iid).ok();
}

/// `Requests::read_open` on an open request, who the token is known.
fn page_open(host: &Arc<dyn Host>, iid: u64) {
    host.drafts(iid).ok();
    host.pipeline(iid).ok();
    host.approvals(iid).ok();
    host.details(iid).ok();
    host.merge_state(iid).ok();
    host.threads(iid).ok();
}

/// `notices::read`: the to-dos (the login is the one the list read already holds), then the discussions of each request whose `updated` moved since
/// the last read (`cold`: every one; a warm read finds them all as they were).
fn notices_read(host: &Arc<dyn Host>, rows: &[Mr], cold: bool) {
    host.todos_all().ok();
    if cold {
        for m in rows {
            host.threads(m.iid).ok();
        }
    }
}

fn check(what: &str, seen: &[String], expected: usize) {
    assert_eq!(
        seen.len(),
        expected,
        "{what}: {} requests, expected {expected}:\n{}",
        seen.len(),
        seen.join("\n")
    );
}

/// What each action costs today, per host. The step named beside a count lowers it; whoever does so changes the
/// constant and the audit report's map ("measured").
struct Counts {
    /// A cold list read: who the token is, members, who could not be reached, the list. Step 6 emits the list first
    /// and keeps the side calls per host for 15 minutes (the same requests once; unreached shares the project and
    /// members reads).
    list_cold: usize,
    /// One row's facts (size, pipeline, approvals, discussions, versions), read for each of the list's rows after
    /// it, again whenever the row's `updated` moves. Step 6 (side reads shared), step 10 (one `pulls/{n}` on GitHub).
    facts_per_row: usize,
    /// A warm list read: login known, the side reads kept: the list's own call (step 6). Step 4 turns the repeat into
    /// 304s.
    list_warm: usize,
    /// A page open on a request with 5 notes, the labels kept (step 6: only the first page open in 15 minutes reads
    /// them). Step 9 reads in parallel (same count) and keeps the last pages; step 10 shares `pulls/{n}` on GitHub.
    page_5: usize,
    /// A page open on a request with 150 notes (two pages of discussions). Same steps as `page_5`.
    page_150: usize,
    /// A cold notices read: the to-dos, the discussions of every request listed (step 5 dropped `/user`, the login is
    /// held; the discussions go in three lanes). Step 11 shares the to-dos between tabs.
    notices_cold: usize,
    /// A warm notices read, nothing changed. Step 11 shares it between tabs; step 7 reads on a fresh list.
    notices_warm: usize,
}

const GITLAB: Counts = Counts {
    list_cold: 4,
    facts_per_row: 5,
    list_warm: 1,
    page_5: 8,
    page_150: 9,
    notices_cold: 4,
    notices_warm: 1,
};

const GITHUB: Counts = Counts {
    list_cold: 3,
    facts_per_row: 8,
    list_warm: 1,
    page_5: 11,
    page_150: 12,
    notices_cold: 7,
    notices_warm: 1,
};

fn run(side: Side, c: Counts) {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    artoo_hosts::forget_kept();
    let (seen, rows) = counted(side, 0, |h| list_read(h, false));
    check("cold list read", &seen, c.list_cold);
    assert_eq!(rows.len() as u64, ROWS, "the canned list is read");
    assert!(rows.iter().all(|m| m.state == MrState::Opened));

    let (seen, _) = counted(side, 5, |h| {
        row_facts(h, &rows[0]);
        Vec::new()
    });
    check("one row's facts", &seen, c.facts_per_row);

    let (seen, _) = counted(side, 0, |h| list_read(h, true));
    check("warm list read", &seen, c.list_warm);

    // The first page open reads the labels once; the ones after it do not.
    let (cold, _) = counted(side, 5, |h| {
        page_open(h, 1);
        Vec::new()
    });
    let labels = cold.iter().filter(|r| r.contains("/labels")).count();
    assert_eq!(
        labels,
        usize::from(side == Side::GitLab),
        "first page open:\n{}",
        cold.join("\n")
    );
    let (seen, _) = counted(side, 5, |h| {
        page_open(h, 1);
        Vec::new()
    });
    check("page open, 5 notes", &seen, c.page_5);
    let (seen, _) = counted(side, 150, |h| {
        page_open(h, 1);
        Vec::new()
    });
    check("page open, 150 notes", &seen, c.page_150);

    let (seen, _) = counted(side, 5, |h| {
        notices_read(h, &rows, true);
        Vec::new()
    });
    check("cold notices read", &seen, c.notices_cold);
    let (seen, _) = counted(side, 5, |h| {
        notices_read(h, &rows, false);
        Vec::new()
    });
    check("warm notices read", &seen, c.notices_warm);
}

#[test]
fn what_a_gitlab_project_costs_in_requests() {
    run(Side::GitLab, GITLAB);
}

#[test]
fn what_a_github_project_costs_in_requests() {
    run(Side::GitHub, GITHUB);
}

/// The canned notes are the notes the counts are about: a request with 150 reads 150 discussions.
#[test]
fn the_canned_notes_are_read() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    for side in [Side::GitLab, Side::GitHub] {
        let (_, got) = counted(side, 150, |h| {
            assert_eq!(h.threads(1).map(|t| t.len()).unwrap_or(0), 150);
            Vec::new()
        });
        assert!(got.is_empty());
    }
}

/// Audit step 4: a second identical read asks with the validators and is answered 304 for every GET; the request
/// count is the same (step 6 and later lower it), what changes is that no body travels, and the rows read the same.
#[test]
fn a_repeated_list_read_is_answered_with_304s() {
    for side in [Side::GitLab, Side::GitHub] {
        tokens();
        let world = World { side, notes: 0 };
        let statuses = Arc::new(Mutex::new(Vec::new()));
        let log = statuses.clone();
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        artoo_hosts::forget_kept();
        let guard = http::with_transport(move |r| {
            let a = world.answer(r);
            log.lock().unwrap().push((r.method.clone(), a.status));
            Ok(a)
        });
        let base = match side {
            Side::GitLab => "https://gl.example/team/proj",
            Side::GitHub => "https://github.com/team/proj",
        };
        let host = artoo_hosts::hosts().of(&WebRemote {
            base: base.into(),
            github: side == Side::GitHub,
        });
        let first = list_read(&host, true);
        let n = statuses.lock().unwrap().len();
        // The 15 minutes' keep would answer the side reads itself; this asks again, conditionally.
        artoo_hosts::forget_kept();
        let second = list_read(&host, true);
        drop(guard);
        let seen = statuses.lock().unwrap().clone();
        assert_eq!(seen.len(), 2 * n, "the repeat costs as many requests");
        let repeat = &seen[n..];
        assert!(
            repeat.iter().any(|(_, s)| *s == 304),
            "some of the repeat is answered 304"
        );
        assert!(
            repeat.iter().all(|(m, s)| (m == "GET") == (*s == 304)),
            "every GET is 304 and nothing else is: {repeat:?}"
        );
        assert_eq!(first.len(), second.len());
        assert_eq!(
            first.len() as u64,
            ROWS,
            "the list reads the same from the kept body"
        );
    }
}
