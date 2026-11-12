//! Branches (#165 stage 4, the tab split's B): the repository's branches in a tab — the branch open with its commits
//! and versions and what the diff shows of them (`history`), what its page says of it (`facts`), the commits' search
//! (`find`), a comparison's own commits (`compare`), the working tree as last looked at (`working`) and what the
//! Branches list reads for its rows (`roots`) — and what they read from the repository (`reading`), each read the
//! entity's own task. Its parts make the history in a tab that answers its acts and offer it as
//! `artoo_branches_api::Branches`, which the app and the other features read and ask; what follows a read the tab
//! hears as the contract's `Event`. Its views draw the side panel (`panel`, `side`), a commit's card (`card`), the line
//! over a branch's page (`facts`), a comparison's pill (`compare`), the Branches list (`list`) and the head over a
//! diff with its toolbar's left (`head`); a click is an `Act`, the branches' own answered here, the rest the tab's
//! (`TabActs`). The Branches space's own page is theirs (`page`).

mod card;
mod commits_page;
mod compare;
mod done;
mod facts;
mod find;
mod head;
mod history;
mod link;
mod list;
mod news;
mod opening;
mod page;
mod pairs;
mod panel;
mod place;
mod reading;
mod refs;
mod roots;
mod search;
mod shown;
mod side;
mod since;
mod waiting;
mod working;

use artoo_shell_api::spaces::{self, Space, SpaceInfo};
use artoo_shell_api::{Action, Frame, Heading, Menu, Parts, when};
use gpui::{App, AppContext, Window};
use std::rc::Rc;

gpui::actions!(
    artoo,
    [
        /// ⌃⌘3: Branches.
        GoToBranches
    ]
);

/// The Branches space, third in the menu, and the key that goes to it; Branches' parts in every tab: the commits'
/// search as one of its inputs, a branch's line, the head over a diff, two versions' commits paired and the card of
/// what next on the item page, the space's own page; its side panel and a branch's Commits among the tab's services
/// (`panel`, `commits_page`).
pub fn install(cx: &mut App) {
    spaces::add(
        cx,
        SpaceInfo {
            space: Space(artoo_branches_api::SPACE),
            label: "Branches",
            icon: "branch",
            order: 30,
            go: "branches.space",
        },
    );
    artoo_shell_api::add_action(
        cx,
        Action::new("branches.space", GoToBranches)
            .heading(Heading::Move)
            .words("Branches")
            .order(20)
            .key("ctrl-cmd-3", when::TAB)
            .menu(Menu::Go, "Branches")
            .at(0, 2),
    );
    artoo_shell_api::add_parts(cx, parts);
}

/// The parts of a tab that answers the branches' acts (`TabActs`): its history, made here, its branches as the
/// other parts read them (`Branches`); none in one that does not.
fn parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let services = frame.services();
    let Some(tab) = services.get::<artoo_branches_api::TabActs>() else {
        return Parts::default();
    };
    let history = cx.new(|_| history::History::new());
    services.set(history::handle(&history, &services));
    services.set(history::acts(&history, tab));
    let (versions, commits) = (history.downgrade(), history.downgrade());
    let goes = frame.clone();
    let panel = panel::view(history.clone(), &frame, cx);
    services.set(panel.clone());
    services.set(commits_page::view(history.clone(), &frame, cx));
    Parts {
        inputs: vec![find::input(history.clone())],
        // A branch's line before the head, which any diff has.
        pieces: vec![
            facts::line(history.clone(), services.clone()),
            head::piece(history.clone(), services.clone()),
            head::bar_piece(history.clone(), services.clone()),
            done::piece(history.clone(), services.clone()),
            pairs::piece(history.clone(), services),
        ],
        searches: vec![
            search::scope(Rc::downgrade(&frame.services())),
            search::commits(Rc::downgrade(&frame.services())),
        ],
        lists: refs::lists(history.downgrade(), frame.clone()),
        pages: Some(page::pages(history, panel, frame, cx)),
        actions: vec![(
            Box::new(GoToBranches),
            Rc::new(move |_, cx| goes.go_space(artoo_branches_api::SPACE, cx)),
        )],
        // A side panel section folded (a snapshot run never saves it).
        snapshots: vec![
            (
                "fold-versions",
                Rc::new(move |_, cx| {
                    versions.update(cx, |h, _| h.sections.versions = true).ok();
                }),
            ),
            (
                "fold-commits",
                Rc::new(move |_, cx| {
                    commits.update(cx, |h, _| h.sections.commits = true).ok();
                }),
            ),
        ],
        ..Default::default()
    }
}
