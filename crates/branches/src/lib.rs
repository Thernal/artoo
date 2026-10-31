//! Branches (#165 stage 4): what the tab keeps of the repository's branches — the branch open with its commits and
//! versions and what the diff shows of them (`history`), what its page says of it (`facts`), the commits' search
//! (`find`), a comparison's own commits (`compare`), the working tree as last looked at (`working`) and what the
//! Branches list reads for its rows (`roots`) — and what they read from the repository (`reading`), each read the
//! entity's own task, the tab hearing what follows as a `history::Event`. The app's views read and draw them for
//! now.

pub mod compare;
pub mod facts;
pub mod find;
pub mod history;
pub mod reading;
pub mod roots;
pub mod side;
pub mod working;
