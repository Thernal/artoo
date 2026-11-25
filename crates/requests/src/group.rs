//! The requests list's groups: whose turn each request is (#189), or its author, in their order, as a band says it
//! and the list's file keeps it.

pub use artoo_requests_api::{Group, GroupBy, SortBy};

/// How a group's band says it, and how the list's file keeps it.
pub trait Band {
    fn heading(&self) -> String;
    fn key(&self) -> String;
}

impl Band for Group {
    fn heading(&self) -> String {
        match self {
            Group::You => "Needs your review".into(),
            Group::Updated => "Updated since your review".into(),
            Group::Reviewed => "Reviewed, nothing new".into(),
            Group::Others => "Others".into(),
            Group::Yours => "Yours".into(),
            Group::Author(name) => name.clone(),
            Group::All => "All".into(),
            Group::Today => "Today".into(),
            Group::ThisWeek => "This week".into(),
            Group::Older => "Older".into(),
        }
    }

    /// As `requests-list.txt` writes it.
    fn key(&self) -> String {
        match self {
            Group::You => "you".into(),
            Group::Updated => "updated".into(),
            // The key its folds were kept under.
            Group::Reviewed => "done".into(),
            Group::Others => "others".into(),
            Group::Yours => "yours".into(),
            // A name has no tab, no comma.
            Group::Author(name) => format!("author:{}", name.replace([',', '\t'], " ")),
            Group::All => "all".into(),
            Group::Today => "today".into(),
            Group::ThisWeek => "week".into(),
            Group::Older => "older".into(),
        }
    }
}

/// The group the list's file names `key`.
pub fn parse(key: &str) -> Option<Group> {
    match key {
        "you" => Some(Group::You),
        "updated" => Some(Group::Updated),
        "done" => Some(Group::Reviewed),
        "others" => Some(Group::Others),
        "yours" => Some(Group::Yours),
        "all" => Some(Group::All),
        "today" => Some(Group::Today),
        "week" => Some(Group::ThisWeek),
        "older" => Some(Group::Older),
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
