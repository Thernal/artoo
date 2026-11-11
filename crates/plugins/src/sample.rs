//! Sample plugins' notices, for a snapshot only (`ARTOO_SNAPSHOT_SOURCES`): the Inbox with nine sources (#197). Each
//! source is offered as the loader offers a plugin's — as its guest, so its id, its kinds and its System are checked
//! as a plugin's are — and its rows are listed by a part, as a plugin's part lists them (#196).

use artoo_shell_api::notices::{self, Kind, Listed, Lister, Listers, Source, Why};
use gpui::App;
use std::rc::Rc;

/// Offers the samples when a snapshot asks for them; nothing otherwise.
pub(crate) fn install(cx: &mut App) {
    if !artoo_core::storage::snapshot_run() || std::env::var_os("ARTOO_SNAPSHOT_SOURCES").is_none()
    {
        return;
    }
    let sources = [
        Source::new(
            "sentry",
            "Sentry",
            vec![Kind::listed("sentry.error", "New errors").why(Why::Failures)],
        ),
        Source::new(
            "slack",
            "Slack",
            vec![Kind::listed("slack.mention", "Mentions you").why(Why::Mentions)],
        ),
        Source::new(
            "jira",
            "Jira",
            vec![
                // Its toast on as declared (Design M1b).
                Kind::listed("jira.assigned", "Assigned to you").toast(),
                Kind::listed("jira.mention", "Mentions you").why(Why::Mentions),
                Kind::listed("jira.changed", "Your issue changed"),
            ],
        ),
        Source::new(
            "jenkins",
            "Jenkins",
            vec![Kind::listed("jenkins.failed", "Failed builds").why(Why::Failures)],
        ),
        Source::new(
            "confluence",
            "Confluence",
            vec![Kind::listed("confluence.reply", "Replies to you")],
        ),
        Source::new(
            "linear",
            "Linear",
            vec![Kind::listed("linear.changed", "Your issue changed")],
        ),
        Source::new(
            "pagerduty",
            "PagerDuty",
            vec![Kind::listed("pagerduty.incident", "Incidents").why(Why::Failures)],
        ),
    ];
    for source in sources {
        notices::guest(cx, Some(source.id.to_owned()));
        notices::add_source(cx, source);
    }
    notices::guest(cx, None);
    artoo_shell_api::add_parts(cx, |frame, _, _| {
        Listers::of(&frame.services()).add(Lister {
            rows: Rc::new(|_| rows()),
            open: Rc::new(|_, _| {}),
            done: None,
        });
        artoo_shell_api::Parts::default()
    });
}

/// What the samples list, minutes ago: PagerDuty has nothing new.
fn rows() -> Vec<Listed> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let row = |kind, key: &str, says: &str, about: &str, text: &str, minutes: i64| Listed {
        kind,
        key: key.to_owned(),
        says: says.to_owned(),
        about: about.to_owned(),
        text: text.to_owned(),
        at: now - minutes * 60,
    };
    vec![
        row(
            "sentry.error",
            "PAYMENTS-2F1",
            "A new error in refunds",
            "ArithmeticException: Rounding necessary",
            "RefundCalculator.refund · 38 events in 10 min",
            9,
        ),
        row(
            "slack.mention",
            "slack:payments:1",
            "Kestrel Okafor mentioned you",
            "#payments",
            "@you is the refund fix in today's release?",
            22,
        ),
        row(
            "jira.assigned",
            "PAY-412:assigned",
            "Assigned to you",
            "PAY-412 Refund rounds down on partial captures",
            "",
            30,
        ),
        row(
            "jenkins.failed",
            "payments-service#1288",
            "Build failed on release/2.4",
            "payments-service #1288",
            "integration-tests: 2 failed",
            35,
        ),
        row(
            "jira.mention",
            "PAY-398:mention:1",
            "Ember Lind mentioned you",
            "PAY-398 Login keeps the last error",
            "@you can this ship with !398?",
            60,
        ),
        row(
            "confluence.reply",
            "confluence:refunds:1",
            "Juniper Hale replied to your comment",
            "Refunds: how we round",
            "Half-even, once, in the source currency.",
            26 * 60,
        ),
        row(
            "linear.changed",
            "MOB-31:in-review",
            "Moved to In review",
            "MOB-31 Biometric sign-in",
            "",
            30 * 60,
        ),
    ]
}
