//! Merge requests (#165 stage 4): what the tab keeps of them — the lists and their groups, the request open, the
//! changes sent to its host and not yet read back (`waiting`), what became of an outdated discussion's line
//! (`outdated`), a request's number in the tab's texts a link (`numbers`), the reactions on its notes, where merging it stands (`merge`), the notifications (`notices`) and the Inbox's (`inbox`) — and what they read from their host
//! (`reading`). The space's own page is theirs (`root`); the app's other views read and draw them for now.

mod bar;
mod complete;
mod create;
mod discussions;
mod group;
mod images;
mod inbox;
mod inbox_page;
mod inbox_view;
mod link;
mod list;
mod merge;
mod merging;
mod notices;
mod numbers;
mod outdated;
mod page;
mod pieces;
mod reactions;
mod reading;
mod requests;
mod root;
mod search;
mod submit;
mod tabs;
mod thread;
mod turn;
mod waiting;
mod why;

use artoo_shell_api::spaces::{self, Space, SpaceInfo};
use artoo_shell_api::{Action, Frame, Heading, Menu, Parts, when};
use gpui::{App, AppContext, Entity, Window};
use std::rc::Rc;

gpui::actions!(
    artoo,
    [
        /// ⇧⌘D: the Discussions window shown or hidden.
        ToggleComments,
        /// ⌃⌘1: the Inbox.
        GoToInbox,
        /// ⌃⌘4: Merge requests.
        GoToMergeRequests,
    ]
);

/// The requests' spaces, the Inbox first in the menu and Merge requests last, and the keys that go to them; their
/// parts in every tab: a request being written's header and a request's line on the item page, the space's own
/// page, a request's Overview and Commits among the tab's services (`tabs`); and the Inbox's (`inbox_parts`).
pub fn install(cx: &mut App) {
    artoo_shell_api::notices::add_source(cx, notices::source());
    spaces::add(
        cx,
        SpaceInfo {
            space: Space(artoo_requests_api::INBOX),
            label: "Inbox",
            icon: "inbox",
            order: 10,
            go: "requests.inbox",
        },
    );
    spaces::add(
        cx,
        SpaceInfo {
            space: Space(artoo_requests_api::SPACE),
            label: "Merge requests",
            icon: "pull-request",
            order: 40,
            go: "requests.space",
        },
    );
    // First in ⌘K and in the Go menu, as the spaces always were.
    artoo_shell_api::add_action(
        cx,
        Action::new("requests.inbox", GoToInbox)
            .heading(Heading::Move)
            .words("Inbox")
            .order(0)
            .key("ctrl-cmd-1", when::TAB)
            .menu(Menu::Go, "Inbox")
            .at(0, 0),
    );
    artoo_shell_api::add_action(
        cx,
        Action::new("requests.space", GoToMergeRequests)
            .heading(Heading::Move)
            .words("Merge requests")
            .order(30)
            .key("ctrl-cmd-4", when::TAB)
            .menu(Menu::Go, "Merge Requests")
            .at(0, 3),
    );
    artoo_shell_api::add_action(
        cx,
        Action::new("requests.discussions", ToggleComments)
            .heading(Heading::View)
            .words("Show or hide the Discussions window")
            // Its place in ⌘K among Artoo's own, as it always had.
            .order(150)
            .key("cmd-shift-d", when::NOT_TYPING),
    );
    artoo_shell_api::add_parts(cx, parts);
    artoo_shell_api::add_parts(cx, inbox_parts);
}

/// The parts of a tab that answers the requests' acts (`TabActs`): their state made — the requests, the
/// notifications, what the Inbox selected —, `Requests` offered and their own `Acts`; none in another tab.
fn parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let services = frame.services();
    let Some(tab) = services.get::<artoo_requests_api::TabActs>() else {
        return Parts::default();
    };
    let requests = cx.new(|_| requests::Requests {
        // The viewer an image opens in is read from them as it is clicked (#211).
        services: Rc::downgrade(&services),
        ..Default::default()
    });
    services.set(requests.clone());
    services.set(cx.new(|_| notices::Notices::default()));
    services.set(cx.new(|_| inbox::InboxView::default()));
    services.set(requests::handle(&requests, &services));
    let acts = requests::acts(&requests, tab);
    services.set(acts.clone());
    services.set(tabs::view(requests.clone(), &frame, cx));
    // The Discussions window, in a tab that says what it is drawn with.
    let tool = services
        .get::<Rc<dyn artoo_requests_api::DiscussionsHost>>()
        .map(|host| discussions::tool(requests.clone(), host, Rc::downgrade(&services), acts, cx));
    let (shows, goes) = (frame.clone(), frame.clone());
    Parts {
        tool,
        actions: vec![
            (
                Box::new(ToggleComments),
                Rc::new(move |_, cx| shows.toggle_tool(artoo_requests_api::DISCUSSIONS, cx)),
            ),
            (
                Box::new(GoToMergeRequests),
                Rc::new(move |_, cx| goes.go_space(artoo_requests_api::SPACE, cx)),
            ),
        ],
        inputs: vec![create::input(requests.clone(), services.clone())],
        // The request being written first: it is over whatever page it was opened on.
        pieces: vec![
            pieces::create_header(requests.clone(), services.clone()),
            pieces::create_switch(requests.clone(), services.clone()),
            pieces::create_form(requests.clone(), frame.clone()),
            outdated::since_piece(requests.clone(), services.clone()),
            pieces::done(requests.clone(), services),
        ],
        links: vec![numbers::pattern(&requests, cx)],
        searches: vec![search::scope(Rc::downgrade(&frame.services()))],
        pages: Some(root::pages(requests, frame, cx)),
        ..Default::default()
    }
}

/// The Inbox's part in a tab that keeps its notifications: its page, and the key that goes to it.
fn inbox_parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let services = frame.services();
    let (Some(notices), Some(view), Some(requests)) = (
        services.get::<Entity<notices::Notices>>(),
        services.get::<Entity<inbox::InboxView>>(),
        services.get::<Entity<requests::Requests>>(),
    ) else {
        return Parts::default();
    };
    let goes = frame.clone();
    // Source ▾ takes the keyboard and opens its menus through the frame (#197).
    view.update(cx, |v, _| v.frame = Some(Rc::downgrade(&frame)));
    Parts {
        actions: vec![(
            Box::new(GoToInbox),
            Rc::new(move |_, cx| goes.go_space(artoo_requests_api::INBOX, cx)),
        )],
        inputs: vec![inbox::sources_input(view.clone(), Rc::downgrade(&services))],
        pages: Some(inbox_page::pages(notices, view, requests, frame, cx)),
        ..Default::default()
    }
}
