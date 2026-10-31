//! Merge requests (#165 stage 4): what the tab keeps of them — the lists and their groups, the request open, the
//! changes sent to its host and not yet read back (`waiting`), what became of an outdated discussion's line
//! (`outdated`), the reactions on its notes, where merging it stands (`merge`), the notifications (`notices`) and the Inbox's (`inbox`) — and what they read from their host
//! (`reading`). The space's own page is theirs (`root`); the app's other views read and draw them for now.

pub mod bar;
pub mod complete;
pub mod create;
pub mod discussions;
pub mod group;
pub mod inbox;
pub mod inbox_page;
pub mod inbox_view;
pub mod list;
pub mod merge;
pub mod merging;
pub mod notices;
pub mod outdated;
pub mod page;
pub mod pieces;
pub mod reactions;
pub mod reading;
pub mod requests;
pub mod root;
pub mod submit;
pub mod thread;
pub mod waiting;

use artoo_shell_api::{Frame, Parts};
use gpui::{App, Entity, Window};
use std::rc::Rc;

/// The requests' parts in every tab: a request being written's header and a request's line on the item page,
/// the space's own page; and the Inbox's (`inbox_parts`).
pub fn install(cx: &mut App) {
    artoo_shell_api::add_parts(cx, parts);
    artoo_shell_api::add_parts(cx, inbox_parts);
}

/// The parts of a tab that offers its requests; none in one that does not.
fn parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let services = frame.services();
    let Some(requests) = services.get::<Entity<requests::Requests>>() else {
        return Parts::default();
    };
    Parts {
        // The request being written first: it is over whatever page it was opened on.
        pieces: vec![
            pieces::create_header(requests.clone(), services.clone()),
            pieces::line(requests.clone(), services.clone()),
            pieces::create_switch(requests.clone(), services.clone()),
            pieces::done(requests.clone(), services),
        ],
        pages: Some(root::pages(requests, frame, cx)),
        ..Default::default()
    }
}

/// The Inbox's part in a tab that keeps its notifications: its page.
fn inbox_parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let services = frame.services();
    let (Some(notices), Some(view), Some(requests)) = (
        services.get::<Entity<notices::Notices>>(),
        services.get::<Entity<inbox::InboxView>>(),
        services.get::<Entity<requests::Requests>>(),
    ) else {
        return Parts::default();
    };
    Parts {
        pages: Some(inbox_page::pages(notices, view, requests, frame, cx)),
        ..Default::default()
    }
}
