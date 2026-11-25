//! The themes to try (Theme…, `artoo_shell_api::search::List`), opened from Settings → Appearance's two rows too
//! (#204): every window theme, or Same as Theme and every code theme, grouped Built in · From plugins · Yours; ↑↓
//! tries each on the whole window (the code's colours on the diff), ↵ keeps it, esc puts back the one chosen. A code
//! theme's contrast on the window's diff is a muted fact on its row, not a warning on the page.

use artoo_core::rank::word_matches;
use artoo_core::themes::{self, T, Theme};
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

/// What Edit on a theme's row does: the theme by its id in the editor (Settings → Appearance).
pub type EditTheme = Rc<dyn Fn(&str, &mut App)>;

/// The window's themes: built in, the plugins', yours, opening on the one in force.
pub fn list(edit: EditTheme) -> List {
    list_of(false, edit)
}

/// The code's themes: Same as Theme first.
pub fn code_list(edit: EditTheme) -> List {
    list_of(true, edit)
}

/// A theme's swatch (#204): its ground and two of its code colours, as the design draws them.
fn swatch_of(t: &Theme) -> [u32; 3] {
    [t.get(T::Bg3), t.get(T::Code1), t.get(T::Code2)]
}

fn list_of(code: bool, edit: EditTheme) -> List {
    let offered = move |t: &Theme| if code { t.code } else { t.window };
    List {
        id: if code { CODE_THEMES } else { THEMES },
        says: Rc::new(move |_| {
            let n = theme::all().iter().filter(|t| offered(t)).count();
            Said {
                // No heading over the filter: its placeholder counts them, the groups theirs.
                title: None,
                placeholder: format!("Filter {n} themes").into(),
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
                let mut same = Find::new("", SAME.to_owned(), String::new(), open)
                    .current(choice.code.is_none())
                    .tried(Rc::new(|cx| code_theme(None, false, cx)));
                if let Some(w) = &window {
                    same = same.swatch(swatch_of(w));
                }
                finds.push(same);
            }
            finds.extend(
                (all.iter())
                    .filter(|t| offered(t))
                    // Its group's words find it too: `plugins`, `light`.
                    .filter(|t| {
                        let hay = format!(
                            "{} {} {}",
                            t.label,
                            group_of(t),
                            if t.light() { "light" } else { "" }
                        );
                        words.iter().all(|w| word_matches(w, &hay))
                    })
                    .map(|t| {
                        let current = match code {
                            true => choice.code.as_deref() == Some(t.id.as_str()),
                            false => t.id == choice.window,
                        };
                        let low = match (&window, code) {
                            (Some(w), true) => themes::code_on(t, w),
                            _ => Vec::new(),
                        };
                        // A fact, a glyph on its row with its sentence as tooltip: its problems, its contrast on the
                        // window's diff, light.
                        let badge = if !t.problems.is_empty() && !t.built_in {
                            Some((
                                "triangle-alert",
                                Some(t.problems.len()),
                                plural(t.problems.len(), "problem"),
                            ))
                        } else if !low.is_empty() {
                            let n = low.len();
                            let tip = format!("{} under 4.5:1 contrast", plural(n, "colour"));
                            Some(("contrast", Some(n), tip))
                        } else if t.light() {
                            Some(("sun", None, "A light theme".to_owned()))
                        } else {
                            None
                        };
                        let (tried, kept, edited) = (t.id.clone(), t.id.clone(), t.id.clone());
                        let keep = Opens::Run(Rc::new(move |cx| match code {
                            true => code_theme(Some(&kept), true, cx),
                            false => window_theme(&kept, true, cx),
                        }));
                        let edit = edit.clone();
                        let mut find = Find::new(group_of(t), t.label.clone(), String::new(), keep)
                            .current(current)
                            .swatch(swatch_of(t))
                            .aside(
                                "pencil",
                                format!("Edit {}", t.label),
                                Rc::new(move |cx| edit(&edited, cx)),
                            )
                            .tried(Rc::new(move |cx| match code {
                                true => code_theme(Some(&tried), false, cx),
                                false => window_theme(&tried, false, cx),
                            }));
                        if let Some((glyph, number, tip)) = badge {
                            find = find.badge(glyph, number, tip);
                        }
                        find
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

/// The group a theme is listed under (#204): shipped with the app, a plugin's, or the themes folder's.
fn group_of(t: &Theme) -> &'static str {
    if t.built_in {
        "Built in"
    } else if t.plugin.is_some() {
        "From plugins"
    } else {
        "Yours"
    }
}

/// Theme `id` on the whole window: tried, or kept (`chosen`).
pub(crate) fn window_theme(id: &str, chosen: bool, cx: &mut App) {
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
