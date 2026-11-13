//! Settings → Appearance (#12, `docs/specs/themes.md`; #171 phase 4; #204): Theme and Code theme, one row each
//! dropping every theme in one list — built in, the plugins', yours —, a theme edited, Your own (your themes' folder
//! and the file icons) — and Set up's Look drawn from the same two rows. The section is this crate's; the window
//! root's rows in it (Fonts and size, Several projects, Panes, Animations) stand between the themes and Your own by
//! their order (40…70).

mod appearance;
mod cards;
mod themes;

use appearance::{Appearance, INPUT};
use artoo_shell_api::settings::row_view;
use artoo_shell_api::settings::{Rows, Section};
use artoo_shell_api::{Action, Frame, Heading, Input, Parts, Typed, when};
use gpui::{App, AppContext, Entity, Window};
use std::rc::Rc;

gpui::actions!(artoo, [SaveTheme, PickTheme]);

/// The section's id, which the window root puts its rows under.
const SECTION: &str = "appearance";

/// Offers the section, ⌘S and each tab's parts.
pub fn install(cx: &mut App) {
    artoo_shell_api::add_settings_section(
        cx,
        Section {
            id: SECTION,
            title: "Appearance",
            order: 20,
        },
    );
    // What Settings' search and ⌘K find here (#203).
    for named in [
        artoo_shell_api::settings::Named::new(SECTION, "Theme")
            .words("window colour color palette dark light"),
        artoo_shell_api::settings::Named::new(SECTION, "Code theme")
            .words("colour color syntax highlight"),
        artoo_shell_api::settings::Named::new(SECTION, "Your themes")
            .words("themes folder json file icons own"),
    ] {
        artoo_shell_api::add_setting(cx, named);
    }
    artoo_shell_api::add_action(
        cx,
        Action::new("appearance.save-theme", SaveTheme).key("cmd-s", when::TAB),
    );
    artoo_shell_api::add_action(
        cx,
        Action::new("appearance.theme", PickTheme)
            .heading(Heading::View)
            .words("Theme — try each on the whole window")
            .order(380),
    );
    artoo_shell_api::add_parts(cx, parts);
}

/// A tab's Appearance: the cards (or the editor, filling the section while a theme is edited) over the window
/// root's rows, your themes and the file icons under them; the hex field; ⌘S; what a snapshot opens.
fn parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let app = cx.new(|_| Appearance::new(frame.clone()));
    // Drawn inside the tab: a change draws the tab again.
    let redraw = frame.clone();
    cx.observe(&app, move |_, cx| redraw.redraw(cx)).detach();
    let themes = row_view(app.clone(), Appearance::render_themes, cx);
    let yours = row_view(app.clone(), Appearance::render_yours, cx);
    let short = row_view(app.clone(), Appearance::render_short, cx);
    // Each setting alone, as Settings' search shows it (#203).
    let theme_hit = row_view(app.clone(), Appearance::render_theme_row, cx);
    let code_hit = row_view(app.clone(), Appearance::render_code_row, cx);
    let yours_hit = row_view(app.clone(), Appearance::render_own, cx);
    let (fills, escape, closed, save) = (
        app.downgrade(),
        app.downgrade(),
        app.downgrade(),
        app.downgrade(),
    );
    let (tries, palette) = (frame.clone(), frame.clone());
    // Edit on a theme's row in its list: Settings → Appearance, the theme in the editor (#204).
    let (editor, settings) = (app.downgrade(), frame.clone());
    let edit: themes::EditTheme = Rc::new(move |id, cx| {
        settings.open_settings(SECTION, cx);
        editor.update(cx, |a, cx| a.start_edit(id, cx)).ok();
    });
    Parts {
        settings: vec![
            Rows::new(SECTION, 10, themes)
                .hit("Theme", theme_hit)
                .hit("Code theme", code_hit)
                .short(short)
                .fills(move |cx| fills.upgrade().is_some_and(|a| a.read(cx).editing()))
                .escape(move |cx| escape.update(cx, |a, cx| a.escape(cx)).unwrap_or(false))
                .closed(move |cx| {
                    closed
                        .update(cx, |a, cx| {
                            if a.editing() {
                                a.close_edit(cx);
                            }
                        })
                        .ok();
                }),
            Rows::new(SECTION, 80, yours).hit("Your themes", yours_hit),
        ],
        actions: vec![
            (
                Box::new(SaveTheme),
                Rc::new(move |_, cx| {
                    // ⌘S is the theme's while one is edited; otherwise it goes on, to a plugin's card or field.
                    if save.upgrade().is_some_and(|a| a.read(cx).editing()) {
                        save.update(cx, |a, cx| a.save(cx)).ok();
                    } else {
                        cx.propagate();
                    }
                }),
            ),
            (
                Box::new(PickTheme),
                Rc::new(move |_, cx| tries.search(themes::THEMES, cx)),
            ),
        ],
        inputs: vec![input(&app)],
        lists: vec![themes::list(edit.clone()), themes::code_list(edit)],
        snapshots: {
            let mut snapshots = snapshots(&app);
            // The themes to try, on the one in force.
            let open: artoo_shell_api::Snap =
                Rc::new(move |_, cx| palette.search(themes::THEMES, cx));
            snapshots.push(("theme-palette", open));
            snapshots
        },
        ..Default::default()
    }
}

/// The hex field as the tab's input: its keys edit the token's value, ↵ and esc let the keyboard go.
fn input(app: &Entity<Appearance>) -> Input {
    let (open, key, left, selected) = (
        app.downgrade(),
        app.downgrade(),
        app.downgrade(),
        app.downgrade(),
    );
    Input {
        id: INPUT,
        open: Rc::new(move |cx| open.upgrade().is_some_and(|a| a.read(cx).typing())),
        key: Rc::new(move |event, _, cx| {
            key.update(cx, |a, cx| a.key(&event.keystroke, cx))
                .unwrap_or(Typed::Kept)
        }),
        left: Rc::new(move |cx| {
            left.update(cx, |a, cx| a.leave_field(cx)).ok();
        }),
        selected: Rc::new(move |cx| selected.upgrade()?.read(cx).selected()),
        caret: Rc::new(|_| true),
    }
}

/// Themes as a snapshot draws them, none kept: `theme-edit:night` in the editor, `theme-token:3` its row open,
/// `theme-bases` its Extends ▾, `theme-raise` every text colour raised to 4.5:1.
fn snapshots(app: &Entity<Appearance>) -> Vec<(&'static str, artoo_shell_api::Snap)> {
    let (edit, token, bases, raise) = (
        app.downgrade(),
        app.downgrade(),
        app.downgrade(),
        app.downgrade(),
    );
    vec![
        (
            "theme-edit:",
            Rc::new(move |id, cx| {
                edit.update(cx, |a, cx| a.start_edit(id, cx)).ok();
            }),
        ),
        (
            "theme-token:",
            Rc::new(move |row, cx| {
                if let Ok(row) = row.parse() {
                    token.update(cx, |a, cx| a.pick_token(row, cx)).ok();
                }
            }),
        ),
        (
            "theme-bases",
            Rc::new(move |_, cx| {
                let at = gpui::point(gpui::px(560.), gpui::px(118.));
                bases.update(cx, |a, cx| a.open_bases(at, cx)).ok();
            }),
        ),
        (
            "theme-raise",
            Rc::new(move |_, cx| {
                raise.update(cx, |a, cx| a.raise_all(cx)).ok();
            }),
        ),
    ]
}
