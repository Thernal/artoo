//! The themes to try (Theme…, `artoo_shell_api::search::List`): every window theme, ↑↓ trying each on the whole
//! window, ↵ keeping it, esc putting back the one chosen.

use artoo_core::rank::word_matches;
use artoo_shell_api::search::{Find, List, Opens, Said};
use artoo_ui_kit::format::plural;
use artoo_ui_kit::theme;
use gpui::App;
use std::rc::Rc;

/// The list's id, which Theme… opens.
pub const THEMES: &str = "themes";

/// The list: your themes after the ones built in, opening on the one in force.
pub fn list() -> List {
    List {
        id: THEMES,
        says: Rc::new(|_| Said {
            title: None,
            placeholder: "Theme — ↑↓ tries it on the whole window, ↵ keeps, esc goes back",
            empty: "No branch, tag or commit by that name.",
        }),
        find: Rc::new(|ask, _| {
            let words: Vec<String> = ask.text.split_whitespace().map(str::to_lowercase).collect();
            let current = theme::choice().window;
            (theme::all().into_iter())
                .filter(|t| t.window && words.iter().all(|w| word_matches(w, &t.label)))
                .map(|t| {
                    let group = if t.built_in { "Themes" } else { "Your themes" };
                    let detail = if t.id == current {
                        "current".to_owned()
                    } else if !t.problems.is_empty() {
                        plural(t.problems.len(), "problem")
                    } else if t.light() {
                        "light".to_owned()
                    } else {
                        String::new()
                    };
                    let (tried, kept) = (t.id.clone(), t.id.clone());
                    let keep = Opens::Run(Rc::new(move |cx| window_theme(&kept, true, cx)));
                    Find::new(group, t.label, detail, keep)
                        .current(t.id == current)
                        .tried(Rc::new(move |cx| window_theme(&tried, false, cx)))
                })
                .collect()
        }),
        opened: None,
        closed: Some(Rc::new(|cx| {
            theme::restore();
            cx.refresh_windows();
        })),
        back: None,
    }
}

/// Theme `id` on the whole window: tried, or kept (`chosen`).
fn window_theme(id: &str, chosen: bool, cx: &mut App) {
    let mut choice = theme::choice();
    choice.window = id.to_owned();
    match chosen {
        true => theme::choose(choice),
        false => theme::preview(&choice),
    }
    cx.refresh_windows();
}
