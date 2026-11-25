//! GitLab's rebase on the HTTP seam (#244): every 2xx is success, with a body or without, and the request read again
//! says whether GitLab still rebases it and why one failed. Answered by `artoo_core::http::with_transport`; no host is
//! called.

use artoo_core::git::WebRemote;
use artoo_core::http::{self, Request, Response};
use artoo_hosts_api::Host;
use std::sync::{Arc, Mutex, Once};

/// The transport is one per process: the tests take this first and run one after another.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn tokens() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: first thing every test does, before it starts a thread of its own; nothing else here reads the env.
        unsafe { std::env::set_var("GITLAB_TOKEN", "test") };
    });
}

fn answer(status: u16, body: &str) -> Response {
    Response {
        status,
        headers: Vec::new(),
        body: body.as_bytes().to_vec(),
    }
}

/// What `act` sends to a GitLab host whose transport answers `reply`, each as `METHOD url form`, and what it gave.
fn sent<T>(
    reply: impl Fn(&Request) -> Response + Send + Sync + 'static,
    act: impl FnOnce(&Arc<dyn Host>) -> T,
) -> (Vec<String>, T) {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    tokens();
    artoo_hosts::forget_reads();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let guard = http::with_transport(move |r| {
        let form: Vec<String> = r.form.iter().map(|(k, v)| format!("{k}={v}")).collect();
        log.lock()
            .unwrap()
            .push(format!("{} {} {}", r.method, r.url, form.join("&")));
        Ok(reply(r))
    });
    let host = artoo_hosts::hosts().of(&WebRemote {
        base: "https://gl.example/team/proj".into(),
        github: false,
    });
    let out = act(&host);
    drop(guard);
    let seen = seen.lock().unwrap().clone();
    (seen, out)
}

const REBASE: &str =
    "PUT https://gl.example/api/v4/projects/team%2Fproj/merge_requests/398/rebase ";

#[test]
fn a_rebase_answered_202_with_its_body_is_taken() {
    let (seen, done) = sent(
        |_| answer(202, r#"{"rebase_in_progress":true}"#),
        |host| host.rebase(398, false),
    );
    assert!(done.is_ok(), "{done:?}");
    assert_eq!(seen, [REBASE]);
}

#[test]
fn rebase_without_pipeline_sends_skip_ci() {
    let (seen, done) = sent(
        |_| answer(202, r#"{"rebase_in_progress":true}"#),
        |host| host.rebase(398, true),
    );
    assert!(done.is_ok(), "{done:?}");
    assert_eq!(seen, [format!("{REBASE}skip_ci=true")]);
}

#[test]
fn a_2xx_without_a_body_or_unknown_is_success_too() {
    for (status, body) in [(202, ""), (200, ""), (204, ""), (299, "accepted")] {
        let (_, done) = sent(
            move |_| answer(status, body),
            |host| host.rebase(398, false),
        );
        assert!(done.is_ok(), "{status} {body:?}: {done:?}");
    }
}

#[test]
fn a_refusal_is_still_an_error() {
    let (_, done) = sent(
        |_| answer(409, r#"{"message":"Rebase is already in progress"}"#),
        |host| host.rebase(398, false),
    );
    assert!(
        done.unwrap_err()
            .to_string()
            .contains("Rebase is already in progress")
    );
}

/// The request as GitLab answers it while rebasing, then once done: `merge_error` set when it failed.
fn request(rebasing: bool, merge_error: Option<&str>) -> String {
    serde_json::json!({
        "iid": 398, "detailed_merge_status": if rebasing { "checking" } else { "mergeable" },
        "diverged_commits_count": if rebasing { 4 } else { 0 },
        "rebase_in_progress": rebasing, "merge_error": merge_error,
    })
    .to_string()
}

#[test]
fn the_request_read_again_says_it_is_rebasing_then_done() {
    let reads = Arc::new(Mutex::new(0));
    let n = reads.clone();
    let (seen, states) = sent(
        move |r| {
            if r.url.contains("/merge_requests/398?") {
                let mut n = n.lock().unwrap();
                *n += 1;
                return answer(200, &request(*n == 1, None));
            }
            answer(200, "{}")
        },
        |host| {
            [
                host.merge_state(398).unwrap(),
                host.merge_state(398).unwrap(),
            ]
        },
    );
    assert!(seen.iter().any(|s| s.contains(
        "merge_requests/398?include_diverged_commits_count=true&include_rebase_in_progress=true"
    )));
    assert!(states[0].rebasing);
    assert_eq!((states[1].rebasing, states[1].behind), (false, 0));
    assert_eq!(states[1].merge_error, "");
}

#[test]
fn a_rebase_that_failed_says_gitlabs_reason() {
    let (_, state) = sent(
        |r| match r.url.contains("/merge_requests/398?") {
            true => answer(
                200,
                &request(
                    false,
                    Some(
                        "Rebase failed: Rebase locally, resolve all conflicts, then push the branch.",
                    ),
                ),
            ),
            false => answer(200, "{}"),
        },
        |host| host.merge_state(398).unwrap(),
    );
    assert!(!state.rebasing);
    assert!(state.merge_error.starts_with("Rebase failed"));
}
