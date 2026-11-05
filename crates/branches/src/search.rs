//! The branches' scope of search everywhere (⌘⇧B, `artoo_shell_api::search`): the branches the list shows, ranked by
//! their names; `#` narrows the All tab to them. A pick opens the branch's page.

use artoo_branches_api::{BranchRoute, Branches};
use artoo_core::rank::rank;
use artoo_core::route::Place;
use artoo_shell_api::Services;
use artoo_shell_api::search::{AllOf, Ask, Find, Search};
use artoo_ui_kit::format;
use gpui::App;
use std::rc::{Rc, Weak};

/// The scope in a tab, reading its branches through their handle there.
pub fn scope(services: Weak<Services>) -> Search {
    Search {
        id: artoo_branches_api::SEARCH,
        label: "Branches",
        placeholder: "Branches",
        go: "shell.search-branches",
        order: 30,
        all: AllOf {
            few: 3,
            order: 10,
            prefix: Some('#'),
        },
        find: Rc::new(move |ask, cx| {
            let Some(services) = services.upgrade() else {
                return Vec::new();
            };
            find(ask, &services, cx)
        }),
    }
}

fn find(ask: Ask, services: &Services, cx: &App) -> Vec<Find> {
    let Some(branches) = services.get::<Rc<dyn Branches>>() else {
        return Vec::new();
    };
    rank(branches.listed(cx), ask.text, |(_, b)| &b.name)
        .into_iter()
        .take(60)
        .map(|(_, b)| Find {
            group: "Branches",
            label: b.name.clone(),
            detail: format::short(b.tip),
            open: BranchRoute { refname: b.refname }.route(),
        })
        .collect()
}
