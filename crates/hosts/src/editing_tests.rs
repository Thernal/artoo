//! Saving an edit of a request, seen at the HTTP seam: what goes to each host, and nothing more. No host is
//! called: the transport is the test's.

use crate::{github, gitlab};
use artoo_core::git::WebRemote;
use artoo_core::http::{self, Request};
use artoo_core::mr::Edits;
use std::sync::{Arc, Mutex};

/// Runs `f` with `remote`'s token saved in a home of its own and every request answered by `answer`; the
/// requests sent, as (method, address, body).
fn sent(
    remote: &WebRemote,
    answer: impl Fn(&Request) -> &'static str + Send + Sync + 'static,
    f: impl FnOnce(),
) -> Vec<(String, String, String)> {
    let dir = tempfile::TempDir::new().unwrap();
    crate::connect::TEST_FIXTURE.with(|d| *d.borrow_mut() = Some(dir.path().to_owned()));
    let host = remote.base.rsplitn(3, '/').last().unwrap().to_owned();
    crate::tokens::save(&host, "test-token").unwrap();
    let log = Arc::new(Mutex::new(Vec::new()));
    let seen = log.clone();
    let guard = http::with_transport(move |r| {
        // A JSON body, or the form's fields.
        let form: Vec<String> = r.form.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let body = r.body.clone().unwrap_or_else(|| form.join(" "));
        seen.lock()
            .unwrap()
            .push((r.method.clone(), r.url.clone(), body));
        http::reply(200, answer(r))
    });
    f();
    drop(guard);
    crate::connect::TEST_FIXTURE.with(|d| *d.borrow_mut() = None);
    log.lock().unwrap().clone()
}

#[test]
fn gitlab_takes_an_edit_in_one_put_and_says_a_draft_in_the_title() {
    let remote = WebRemote {
        base: "https://gitlab.edit.test/group/app".into(),
        github: false,
    };
    let log = sent(
        &remote,
        |r| match r.method.as_str() {
            "PUT" => r#"{"title":"Draft: Louder"}"#,
            _ => r#"{"title":"Louder"}"#,
        },
        || {
            let edits = Edits {
                description: Some("Why & how".into()),
                ..Default::default()
            };
            gitlab::edit_request(&remote, 7, &edits).unwrap();
            let draft = Edits {
                draft: Some(true),
                ..Default::default()
            };
            let title = gitlab::edit_request(&remote, 7, &draft).unwrap();
            assert_eq!(title.as_deref(), Some("Draft: Louder"));
        },
    );
    let api = "https://gitlab.edit.test/api/v4/projects/group%2Fapp/merge_requests/7";
    let said: Vec<(&str, &str)> = (log.iter())
        .map(|(m, u, _)| (m.as_str(), u.as_str()))
        .collect();
    // The title read only to say the draft in it.
    assert_eq!(said, [("PUT", api), ("GET", api), ("PUT", api)]);
    assert_eq!(log[0].2, "description=Why & how");
    assert_eq!(log[2].2, "title=Draft: Louder");
}

#[test]
fn github_takes_the_words_in_a_patch_and_the_draft_by_its_mutation() {
    let remote = WebRemote {
        base: "https://github.com/group/app".into(),
        github: true,
    };
    let log = sent(
        &remote,
        |r| match r.url.ends_with("graphql") {
            true => r#"{"data":{}}"#,
            false => r#"{"node_id":"PR_1"}"#,
        },
        || {
            let edits = Edits {
                title: Some("Louder".into()),
                description: Some("Why".into()),
                draft: Some(false),
            };
            github::edit_request(&remote, 7, &edits).unwrap();
        },
    );
    let pull = "https://api.github.com/repos/group/app/pulls/7";
    assert_eq!((log[0].0.as_str(), log[0].1.as_str()), ("PATCH", pull));
    assert!(log[0].2.contains(r#""body":"Why""#) && log[0].2.contains(r#""title":"Louder""#));
    assert_eq!((log[1].0.as_str(), log[1].1.as_str()), ("GET", pull));
    assert!(log[2].2.contains("markPullRequestReadyForReview") && log[2].2.contains("PR_1"));
    assert_eq!(log.len(), 3);
}

#[test]
fn a_draft_is_said_in_a_gitlab_title_once() {
    assert_eq!(gitlab::draft_title("Louder", true), "Draft: Louder");
    assert_eq!(gitlab::draft_title("Draft: Louder", true), "Draft: Louder");
    assert_eq!(gitlab::draft_title("[Draft] WIP: Louder", false), "Louder");
}
