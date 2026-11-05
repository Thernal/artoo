//! The branches' scopes of search everywhere (`artoo_shell_api::search`): the branches the list shows, ranked by
//! their names, `#` narrowing the All tab to them (⌘⇧B); and the commits of what is open, by words of the message,
//! the author or the id (⌘⇧C). A pick opens the branch's page, or the commit's.

use artoo_branches_api::{BranchRoute, Branches, Selection};
use artoo_core::rank::{rank, word_matches};
use artoo_core::route::Place;
use artoo_shell_api::Services;
use artoo_shell_api::search::{AllOf, Ask, Find, Search};
use artoo_ui_kit::format;
use gpui::App;
use std::rc::{Rc, Weak};

/// The branches' scope in a tab, reading them through their handle there.
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
        find: finds(services, find),
    }
}

/// The commits' scope in a tab: the commits listed of the branch open, or of the request.
pub fn commits(services: Weak<Services>) -> Search {
    Search {
        id: artoo_branches_api::SEARCH_COMMITS,
        label: "Commits",
        placeholder: "Commits — by words of the message, the author or the id",
        go: "shell.search-commits",
        order: 20,
        all: AllOf {
            few: 4,
            order: 30,
            prefix: None,
        },
        find: finds(services, find_commits),
    }
}

fn finds(
    services: Weak<Services>,
    find: fn(Ask, &dyn Branches, &App) -> Vec<Find>,
) -> artoo_shell_api::search::Finds {
    Rc::new(move |ask, cx| {
        let Some(services) = services.upgrade() else {
            return Vec::new();
        };
        match services.get::<Rc<dyn Branches>>() {
            Some(branches) => find(ask, &*branches, cx),
            None => Vec::new(),
        }
    })
}

fn find(ask: Ask, branches: &dyn Branches, cx: &App) -> Vec<Find> {
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

/// Every commit until something is typed (the All tab asks only once it is).
fn find_commits(ask: Ask, branches: &dyn Branches, cx: &App) -> Vec<Find> {
    let words: Vec<String> = ask.text.split_whitespace().map(str::to_lowercase).collect();
    let matches = |hay: &str| words.iter().all(|w| word_matches(w, hay));
    let keep = |c: &artoo_core::git::CommitInfo| {
        matches(&c.summary)
            || matches(&c.author)
            || words
                .first()
                .is_some_and(|w| c.id.to_string().starts_with(w.as_str()))
    };
    branches
        .commits_where(&keep, 60, cx)
        .into_iter()
        .filter_map(|c| {
            // Its page, as a click on it in the list opens.
            let at = branches.commit_of(c.id, cx)?;
            Some(Find {
                group: "Commits",
                label: c.summary.clone(),
                detail: format!("{} · {}", format::short(c.id), c.author),
                open: branches.route_of(Selection::Commit(at), cx)?,
            })
        })
        .collect()
}
