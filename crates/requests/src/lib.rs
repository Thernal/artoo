//! Merge requests (#165 stage 4): what the tab keeps of them — the lists and their groups, the request open, the
//! changes sent to its host and not yet read back (`waiting`), what became of an outdated discussion's line
//! (`outdated`), the reactions on its notes, where merging it stands (`merge`) — and what they read from their host
//! (`reading`). The app's views read and draw them for now.

mod area;
pub mod bar;
pub mod complete;
pub mod discussions;
pub mod group;
pub mod merge;
pub mod merging;
pub mod outdated;
pub mod reactions;
pub mod reading;
pub mod requests;
pub mod submit;
pub mod waiting;
