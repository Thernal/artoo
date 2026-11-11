//! What the Inbox sorts its rows by (#197), here because the notices' registry (`artoo-shell-api`), the agents'
//! contract and the Inbox's (`artoo-requests-api`) all name it.

/// What happened: the Inbox's filter row, a fixed set every kind of notice is filed under as its source declares it
/// (`artoo_shell_api::notices::Kind::why`), so a new source adds no filter. A kind under none is listed under All.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Why {
    Mentions,
    Reviews,
    Asks,
    Failures,
}

impl Why {
    pub const ALL: [Why; 4] = [Why::Mentions, Why::Reviews, Why::Asks, Why::Failures];

    /// Its filter's name.
    pub fn name(self) -> &'static str {
        match self {
            Why::Mentions => "Mentions",
            Why::Reviews => "Review requests",
            Why::Asks => "Asks you",
            Why::Failures => "Failures",
        }
    }
}

/// A note of the reviewer's an agent answered that waits for them in the Inbox (#116).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    /// The note's id.
    pub id: u64,
    /// When the agent last wrote, Unix seconds.
    pub at: i64,
    /// Read once looked at: a new answer is new again.
    pub key: String,
    /// It asks you something (else it says it is done): Asks you's.
    pub asks: bool,
    /// The note and what the agent wrote, for the Inbox's filter to look in.
    pub text: String,
}
