//! A line written on, found again in the text as it is now: where a comment or an agent's note sits once the
//! file changed (#46).

/// Where a line written on sits in the file as it is now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// On this 1-based line.
    Line(u32),
    /// The line is gone or changed beyond recognition.
    Outdated,
}

/// Line `at` reading `code`, found again in a side of `count` lines: there if it still reads the
/// same, else the one line that does; ambiguous or missing means outdated. A comment's line is found
/// this way, and an agent's notes on its changes (#151).
pub fn find_line<'a>(at: u32, code: &str, count: u32, line: impl Fn(u32) -> &'a str) -> Place {
    if at >= 1 && at <= count && line(at) == code {
        return Place::Line(at);
    }
    let mut found = (1..=count).filter(|&n| line(n) == code);
    match (found.next(), found.next()) {
        (Some(only), None) if !code.trim().is_empty() => Place::Line(only),
        _ => Place::Outdated,
    }
}
