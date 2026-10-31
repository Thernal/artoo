//! Branches (#165 stage 4): what the tab keeps of the repository's branches — the branch open with its commits and
//! versions and what the diff shows of them (`history`), what its page says of it (`facts`), the commits' search
//! (`find`), a comparison's own commits (`compare`), the working tree as last looked at (`working`) and what the
//! Branches list reads for its rows (`roots`) — and what they read from the repository (`reading`), each read the
//! entity's own task, the tab hearing what follows as a `history::Event`. Its views draw the side panel's rows
//! (`side`), a commit's card (`card`), the line over a branch's page (`facts`), a comparison's pill (`compare`) and
//! the Branches list (`list`), a click asking the tab a `history::Act`; the app's other views read and draw the
//! history for now.

pub mod card;
pub mod compare;
pub mod facts;
pub mod find;
pub mod history;
pub mod list;
pub mod reading;
pub mod roots;
pub mod side;
pub mod working;
