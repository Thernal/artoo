//! Branches (#165 stage 4): what the tab keeps of the repository's branches — the branch open with its commits and
//! versions and what the diff shows of them (`history`), what its page says of it (`facts`), the commits' search
//! (`find`), a comparison's own commits (`compare`), the working tree as last looked at (`working`) and what the
//! Branches list reads for its rows (`roots`) — and what they read from the repository (`reading`), each read the
//! entity's own task, the tab hearing what follows as a `history::Event`. Its views draw the side panel's rows
//! (`side`), a commit's card (`card`), the line over a branch's page (`facts`), a comparison's pill (`compare`) and
//! the Branches list (`list`) and the head over a diff with its toolbar's left (`head`), a click asking the tab a
//! `history::Act`; the app's other views read and draw the history for now.

pub mod card;
pub mod compare;
pub mod facts;
pub mod find;
pub mod head;
pub mod history;
pub mod list;
pub mod opening;
pub mod panel;
pub mod place;
pub mod reading;
pub mod roots;
pub mod shown;
pub mod side;
pub mod waiting;
pub mod working;

use artoo_shell_api::{Frame, Parts};
use gpui::{App, Entity, Window};
use std::rc::Rc;

/// Branches' parts in every tab: the commits' search as one of its inputs, a branch's line and the head over a
/// diff on the item page.
pub fn install(cx: &mut App) {
    artoo_shell_api::add_parts(cx, parts);
}

/// The parts of a tab that offers its history; none in one that does not.
fn parts(frame: Rc<dyn Frame>, _: &mut Window, _: &mut App) -> Parts {
    let services = frame.services();
    let Some(history) = services.get::<Entity<history::History>>() else {
        return Parts::default();
    };
    Parts {
        inputs: vec![find::input(history.clone())],
        // A branch's line before the head, which any diff has.
        pieces: vec![
            facts::line(history.clone(), services.clone()),
            head::piece(history.clone(), services.clone()),
            head::bar_piece(history, services),
        ],
        ..Default::default()
    }
}
