//! The requests list's groups: what a request needs from you, in their order, as a band says it and the list's file
//! keeps it.

use artoo_ui_kit::theme;
use gpui::Rgba;

pub use artoo_requests_api::Group;

/// How a group's band says it, and how the list's file keeps it.
pub trait Band {
    fn heading(self) -> &'static str;
    fn icon(self) -> (&'static str, Rgba);
    fn key(self) -> &'static str;
}

impl Band for Group {
    fn heading(self) -> &'static str {
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
    fn icon(self) -> (&'static str, Rgba) {
        match self {
            Group::Asks => ("eye", theme::accent()),
            Group::Updated => ("refresh", theme::warning()),
            Group::New => ("pull-request", theme::muted()),
            Group::Reviewed => ("check", theme::added()),
            Group::Yours => ("user", theme::faint()),
        }
    }

    /// As `requests-list.txt` writes it.
    fn key(self) -> &'static str {
        match self {
            Group::Asks => "asks",
            Group::Updated => "updated",
            // "others", its name before.
            Group::New => "others",
            Group::Reviewed => "reviewed",
            Group::Yours => "yours",
        }
    }
}

/// The group the list's file names `key`.
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
