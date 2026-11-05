//! Settings → Appearance (#12, `docs/specs/themes.md`; #171 phase 4): the Window and Code themes as cards, a theme
//! edited, your themes' folder and the file icons — and Set up's Look drawn from the same cards. The section is
//! this crate's; the window root's rows in it (Fonts and size, Several projects, Panes, Animations) stand between
//! the cards and your themes by their order (40…70).

mod appearance;
mod cards;

use appearance::{Appearance, Drawn, INPUT};
use artoo_shell_api::settings::{Rows, Section};
use artoo_shell_api::{Action, Frame, Input, Parts, Typed, when};
use gpui::{App, AppContext, Entity, Window};
use std::rc::Rc;

gpui::actions!(artoo, [SaveTheme]);

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
    artoo_shell_api::add_action(
        cx,
        Action::new("appearance.save-theme", SaveTheme).key("cmd-s", when::TAB),
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
    let themes = Drawn::new(&app, Appearance::render_themes, cx);
    let yours = Drawn::new(&app, Appearance::render_yours, cx);
    let short = Drawn::new(&app, Appearance::render_short, cx);
    let (fills, escape, closed, save) = (
        app.downgrade(),
        app.downgrade(),
        app.downgrade(),
        app.downgrade(),
    );
    Parts {
        settings: vec![
            Rows::new(SECTION, 10, themes)
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
            Rows::new(SECTION, 80, yours),
        ],
        actions: vec![(
            Box::new(SaveTheme),
            Rc::new(move |_, cx| {
                save.update(cx, |a, cx| a.save(cx)).ok();
            }),
        )],
        inputs: vec![input(&app)],
        snapshots: snapshots(&app),
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
