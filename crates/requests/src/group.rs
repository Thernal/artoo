//! The requests list's groups: whose turn each request is (#189), or its author, in their order, as a band says it
//! and the list's file keeps it.

use artoo_ui_kit::theme;
use gpui::Rgba;

pub use artoo_requests_api::{Group, GroupBy, SortBy};

/// How a group's band says it, and how the list's file keeps it.
pub trait Band {
    fn heading(&self) -> String;
    fn icon(&self) -> (&'static str, Rgba);
    fn key(&self) -> String;
}

impl Band for Group {
    fn heading(&self) -> String {
        match self {
            Group::You => "Waiting on you".into(),
            Group::Others => "Waiting on others".into(),
            Group::Yours => "Yours".into(),
            Group::Done => "Done".into(),
            Group::Author(name) => name.clone(),
            Group::All => "All".into(),
        }
    }

    /// The icon and colour its band opens with.
    fn icon(&self) -> (&'static str, Rgba) {
        match self {
            Group::You => ("eye", theme::accent()),
            Group::Others => ("clock", theme::muted()),
            Group::Yours => ("user", theme::faint()),
            Group::Done => ("check", theme::added()),
            Group::Author(_) => ("user", theme::faint()),
            Group::All => ("pull-request", theme::muted()),
        }
    }

    /// As `requests-list.txt` writes it.
    fn key(&self) -> String {
        match self {
            Group::You => "you".into(),
            Group::Others => "others".into(),
            Group::Yours => "yours".into(),
            Group::Done => "done".into(),
            // A name has no tab, no comma.
            Group::Author(name) => format!("author:{}", name.replace([',', '\t'], " ")),
            Group::All => "all".into(),
        }
    }
}

/// The group the list's file names `key`.
pub fn parse(key: &str) -> Option<Group> {
    match key {
        "you" => Some(Group::You),
        "others" => Some(Group::Others),
        "yours" => Some(Group::Yours),
        "done" => Some(Group::Done),
        "all" => Some(Group::All),
        _ => key
            .strip_prefix("author:")
            .map(|n| Group::Author(n.to_owned())),
    }
}

/// `Display ▾`'s choices as the list's file keeps them: `turn`, `author`, `none`; `updated`, `oldest`, `size`.
pub fn display_key(group: GroupBy, sort: SortBy) -> String {
    let g = match group {
        GroupBy::Turn => "turn",
        GroupBy::Author => "author",
        GroupBy::None => "none",
    };
    let s = match sort {
        SortBy::Updated => "updated",
        SortBy::Oldest => "oldest",
        SortBy::Size => "size",
    };
    format!("{g}/{s}")
}

/// `display_key` read back; the defaults for what it does not say.
pub fn parse_display(key: &str) -> (GroupBy, SortBy) {
    let (g, s) = key.split_once('/').unwrap_or((key, ""));
    (
        match g {
            "author" => GroupBy::Author,
            "none" => GroupBy::None,
            _ => GroupBy::Turn,
        },
        match s {
            "oldest" => SortBy::Oldest,
            "size" => SortBy::Size,
            _ => SortBy::Updated,
        },
    )
}
