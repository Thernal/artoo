//! The links a tab's texts have without being written as links (#171, D11): a feature offers a kind of them in its
//! parts (`Parts::links`) — the requests' numbers, `!398` on GitLab and `#12` on GitHub —, and every text the tab
//! draws through the kit's markdown (comments, descriptions, a commit's message) shows them as links, followed as the
//! written ones are: in the app when a host reads the address back, else in the browser.

use artoo_ui_kit::markdown::FindLinks;
use gpui::App;
use std::ops::Range;

/// A kind of link a feature finds in text, in its `order` among the others.
#[derive(Clone)]
pub struct Pattern {
    /// Its id, for the log and the tests: `requests.number`.
    pub id: &'static str,
    pub order: u16,
    /// Its links in a text: each one's bytes and its address, in order and apart.
    pub find: FindLinks,
}

/// Every pattern's links in `text`, in the patterns' order: one found where an earlier one found none, then sorted by
/// where they start.
pub fn find(patterns: &[Pattern], text: &str, cx: &App) -> Vec<(Range<usize>, String)> {
    let mut found: Vec<(Range<usize>, String)> = Vec::new();
    for pattern in patterns {
        for (range, url) in (pattern.find)(text, cx) {
            if !found
                .iter()
                .any(|(r, _)| r.start < range.end && range.start < r.end)
            {
                found.push((range, url));
            }
        }
    }
    found.sort_by_key(|(r, _)| r.start);
    found
}
