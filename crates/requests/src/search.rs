//! Merge requests' scope of search everywhere (⌘⇧R, `artoo_shell_api::search`): a request by words of its title or
//! its author, or by its number (`!398`, `#12`); in the All tab by its title or its number. A pick opens its page.

use artoo_core::rank::word_matches;
use artoo_core::route::Place;
use artoo_requests_api::{RequestRoute, Requests};
use artoo_shell_api::Services;
use artoo_shell_api::search::{AllOf, Ask, Find, Opens, Search};
use artoo_ui_kit::words;
use gpui::App;
use std::rc::{Rc, Weak};

/// The scope in a tab, reading its requests through their handle there.
pub fn scope(services: Weak<Services>) -> Search {
    Search {
        id: artoo_requests_api::SEARCH,
        label: "Requests",
        placeholder: "Merge requests — by title or number",
        go: "shell.search-requests",
        order: 40,
        all: AllOf {
            few: 3,
            order: 40,
            prefix: None,
            alone: false,
        },
        find: Rc::new(move |ask, cx| {
            let Some(services) = services.upgrade() else {
                return Vec::new();
            };
            find(ask, &services, cx)
        }),
        opened: None,
    }
}

fn find(ask: Ask, services: &Services, cx: &App) -> Vec<Find> {
    let Some(requests) = services.get::<Rc<dyn Requests>>() else {
        return Vec::new();
    };
    let words: Vec<String> = ask.text.split_whitespace().map(str::to_lowercase).collect();
    let matches = |hay: &str| words.iter().all(|w| word_matches(w, hay));
    let number = ask.text.trim_start_matches(['!', '#']);
    requests
        .list(cx)
        .into_iter()
        .filter(|m| {
            let by_number = m.iid.to_string() == number;
            match ask.all {
                true => matches(&m.title) || by_number,
                false => words.is_empty() || matches(&m.title) || matches(&m.author) || by_number,
            }
        })
        .take(60)
        .map(|m| {
            let open = Opens::Route(RequestRoute { number: m.iid }.route());
            let label = format!("{} {}", words::number(m.iid), m.title);
            Find::new("Merge requests", label, m.author.clone(), open)
        })
        .collect()
}
