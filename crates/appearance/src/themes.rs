//! The themes to try (Theme…, `artoo_shell_api::search::List`), opened from Settings → Appearance's two rows too
//! (#204): every window theme, or Same as Theme and every code theme, grouped Built in · From plugins · Yours; ↑↓
//! tries each on the whole window (the code's colours on the diff), ↵ keeps it, esc puts back the one chosen. A code
//! theme's contrast on the window's diff is a muted fact on its row, not a warning on the page.

use artoo_core::rank::word_matches;
use artoo_core::themes::{self, TOKENS};
use artoo_shell_api::search::{Find, List, Opens, Said};
use artoo_ui_kit::format::plural;
use artoo_ui_kit::theme;
use gpui::App;
use std::rc::Rc;

/// The list's id, which Theme… and the Theme row open.
pub const THEMES: &str = "themes";
/// The code themes' list, which the Code theme row opens.
pub const CODE_THEMES: &str = "code-themes";

/// What a code theme follows when it has none of its own (#204).
pub const SAME: &str = "Same as Theme";

/// The window's themes: yours after the ones built in, opening on the one in force.
pub fn list() -> List {
    list_of(false)
}

/// The code's themes: Same as Theme first.
pub fn code_list() -> List {
    list_of(true)
}

fn list_of(code: bool) -> List {
    List {
        id: if code { CODE_THEMES } else { THEMES },
        says: Rc::new(move |_| {
            let n = (theme::all().iter())
                .filter(|t| if code { t.code } else { t.window })
                .count();
            Said {
                title: Some(format!(
                    "{} · {n}",
                    if code { "Code theme" } else { "Theme" }
                )),
                placeholder: if code {
                    "Filter the code themes — ↑↓ tries it on the diff, ↵ keeps, esc goes back"
                } else {
                    "Filter the themes — ↑↓ tries it on the whole window, ↵ keeps, esc goes back"
                },
                empty: "No theme by that name.",
            }
        }),
        find: Rc::new(move |ask, _| {
            let words: Vec<String> = ask.text.split_whitespace().map(str::to_lowercase).collect();
            let choice = theme::choice();
            let all = theme::all();
            let window = (all.iter())
                .find(|t| t.id == choice.window)
                .or(all.first())
                .cloned();
            let mut finds = Vec::new();
            if code && words.iter().all(|w| word_matches(w, SAME)) {
                let open = Opens::Run(Rc::new(|cx| code_theme(None, true, cx)));
                let detail = if choice.code.is_none() { "current" } else { "" };
                finds.push(
                    Find::new("", SAME.to_owned(), detail.to_owned(), open)
                        .current(choice.code.is_none())
                        .tried(Rc::new(|cx| code_theme(None, false, cx))),
                );
            }
            finds.extend(
                (all.iter())
                    .filter(|t| if code { t.code } else { t.window })
                    .filter(|t| words.iter().all(|w| word_matches(w, &t.label)))
                    .map(|t| {
                        let current = match code {
                            true => choice.code.as_deref() == Some(t.id.as_str()),
                            false => t.id == choice.window,
                        };
                        // Plugins bring no themes yet: built in, or yours.
                        let group = if t.built_in { "Built in" } else { "Yours" };
                        let low = match (&window, code) {
                            (Some(w), true) => themes::code_on(t, w),
                            _ => Vec::new(),
                        };
                        let detail = if current {
                            "current".to_owned()
                        } else if !t.problems.is_empty() && !t.built_in {
                            plural(t.problems.len(), "problem")
                        } else if !low.is_empty() {
                            // Which, by the token's name: `2 under 4.5:1 · Comment 3.9`.
                            let worst = (low.iter())
                                .map(|(tok, r)| format!("{} {r:.1}", TOKENS[*tok as usize].0))
                                .next()
                                .unwrap_or_default();
                            format!("{} under 4.5:1 · {worst}", low.len())
                        } else if t.light() {
                            "light".to_owned()
                        } else {
                            String::new()
                        };
                        let (tried, kept) = (t.id.clone(), t.id.clone());
                        let keep = Opens::Run(Rc::new(move |cx| match code {
                            true => code_theme(Some(&kept), true, cx),
                            false => window_theme(&kept, true, cx),
                        }));
                        Find::new(group, t.label.clone(), detail, keep)
                            .current(current)
                            .tried(Rc::new(move |cx| match code {
                                true => code_theme(Some(&tried), false, cx),
                                false => window_theme(&tried, false, cx),
                            }))
                    }),
            );
            finds
        }),
        opened: None,
        closed: Some(Rc::new(|cx| {
            theme::restore();
            cx.refresh_windows();
        })),
        back: None,
        tab: None,
    }
}

/// Theme `id` on the whole window: tried, or kept (`chosen`).
fn window_theme(id: &str, chosen: bool, cx: &mut App) {
    let mut choice = theme::choice();
    choice.window = id.to_owned();
    apply(choice, chosen, cx);
}

/// Code theme `id` (None: Same as Theme): tried, or kept.
fn code_theme(id: Option<&str>, chosen: bool, cx: &mut App) {
    let mut choice = theme::choice();
    choice.code = id.map(str::to_owned);
    apply(choice, chosen, cx);
}

fn apply(choice: theme::Choice, chosen: bool, cx: &mut App) {
    match chosen {
        true => theme::choose(choice),
        false => theme::preview(&choice),
    }
    cx.refresh_windows();
}
