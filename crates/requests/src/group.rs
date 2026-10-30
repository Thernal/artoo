//! The requests list's groups: what a request needs from you, in their order, as a band says it and the list's file
//! keeps it.

use artoo_ui_kit::theme;
use gpui::Rgba;

/// What a request needs from you: the list's groups, in their order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Group {
    /// You are asked to review it, or started at its head.
    Asks,
    /// You marked files of it, and it was pushed since.
    Updated,
    /// Not asked of you, and nothing of it reviewed yet: new to you; after what changed, before
    /// what is reviewed.
    New,
    /// Its head is reviewed.
    Reviewed,
    Yours,
}

impl Group {
    pub fn heading(self) -> &'static str {
        match self {
            Group::Asks => "Needs your review",
            Group::Updated => "Updated since your review",
            Group::New => "New",
            Group::Reviewed => "Reviewed, nothing new",
            Group::Yours => "Yours",
        }
    }

    /// The icon and colour its band opens with: the group says how far your review is, its rows
    /// do not (#81).
    pub fn icon(self) -> (&'static str, Rgba) {
        match self {
            Group::Asks => ("eye", theme::accent()),
            Group::Updated => ("refresh", theme::warning()),
            Group::New => ("pull-request", theme::muted()),
            Group::Reviewed => ("check", theme::added()),
            Group::Yours => ("user", theme::faint()),
        }
    }

    /// As `requests-list.txt` writes it.
    pub fn key(self) -> &'static str {
        match self {
            Group::Asks => "asks",
            Group::Updated => "updated",
            // "others", its name before.
            Group::New => "others",
            Group::Reviewed => "reviewed",
            Group::Yours => "yours",
        }
    }

    pub fn parse(key: &str) -> Option<Group> {
        [
            Group::Asks,
            Group::Updated,
            Group::New,
            Group::Reviewed,
            Group::Yours,
        ]
        .into_iter()
        .find(|g| g.key() == key)
    }
}
