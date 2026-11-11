//! The agents' kinds of notice (#196), offered by the agents as they install; the tab raises the two it shows
//! (`SHOWS`, `PRESENCE`) as toasts of these kinds.

/// An agent asks you something about your note: it waits in the Inbox, a toast and the system say so.
pub const ASKS: &str = "agents.asks";
/// An agent finished what your note asked: it waits in the Inbox, the system says so.
pub const DONE: &str = "agents.done";
/// An agent shows you something — pending comments, changes opened, a request drafted, a place it points at: its home
/// is where it shows, a toast and the system say so.
pub const SHOWS: &str = "agents.shows";
/// A chat begins or stops listening: the Agents window is its home, a toast says so.
pub const PRESENCE: &str = "agents.presence";
