//! Controls a feature's view shares with the frame: a bar's button, a tool window's foot and its list's pieces (a
//! group's heading, a comment as an entry), rows breathing while a list loads, what follows the pointer while
//! something is dragged, the words for showing a file where it lies; a root list's row pieces (its unread dot, how
//! far its review is), its preview's pills and buttons, a page's message when it has nothing to show, an on/off
//! switch, a row of a list to choose from.

use super::{ROW_RADIUS, hover, icons, keys, motion, px, theme, words};
use artoo_core::platform;
use gpui::{
    AnyElement, Context, Div, FontWeight, Rgba, SharedString, div, prelude::FluentBuilder,
    prelude::*,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Finder with it selected, as the rail's project menu words it off macOS.
pub const REVEAL: &str = if platform::MAC {
    "Reveal in Finder"
} else {
    "Show in its folder"
};

/// What follows the pointer while a space or a tab is dragged: its name.
pub struct DragLabel(pub String);

impl Render for DragLabel {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(10.))
            .py(px(4.))
            .rounded(px(ROW_RADIUS))
            .bg(theme::selected())
            .border_1()
            .border_color(theme::island_border())
            .text_size(px(13.))
            .text_color(theme::text())
            .child(self.0.clone())
    }
}

/// A button of the bar: its label and its key as caps; the primary one filled.
pub fn bar_button(
    id: &'static str,
    label: &str,
    key: &'static str,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .h(px(28.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap_2()
        .rounded(px(ROW_RADIUS))
        .cursor_pointer()
        .text_size(px(13.))
        .when(primary, |s| {
            s.bg(theme::accent())
                .text_color(theme::on_accent())
                .font_weight(FontWeight::MEDIUM)
                .hover(|s| s.opacity(0.9))
        })
        .when(!primary, |s| {
            s.border_1()
                .border_color(theme::island_border())
                .hover(|s| s.bg(theme::hover()))
        })
        .child(words::say(label).into_owned())
        .when(!key.is_empty(), |s| {
            s.child(keys::caps(
                key,
                if primary {
                    keys::On::Filled
                } else {
                    keys::On::Plain
                },
            ))
        })
}

/// A labelled row of Settings: the name, and a line only where the name and the control do not say enough, on the
/// left; the control on the right. 44 px, rows touching (#203).
pub fn setting(
    title: &'static str,
    detail: impl Into<SharedString>,
    control: impl IntoElement,
) -> impl IntoElement {
    setting_row(title, title, detail, control, None)
}

/// What a click on a row's Reset does (`setting_row`).
pub type Reset = Box<dyn Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static>;

/// A row of Settings (`setting`) with its words as given; one off its default carries a dot before its name and
/// `Reset` on hover, `reset` what that does (#203). While Settings searches, what was typed is marked in its words and
/// a word it answers to beyond them is said under them; a row a door led to is lit, the keyboard's has a ring
/// (`settings_look`).
pub fn setting_row(
    id: impl Into<SharedString>,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    control: impl IntoElement,
    reset: Option<Reset>,
) -> gpui::Stateful<Div> {
    let id: SharedString = id.into();
    let group: SharedString = format!("setting-{id}").into();
    let (title, detail): (SharedString, SharedString) = (title.into(), detail.into());
    let (shown, line) = (
        keys::text(&title).into_owned(),
        keys::text(&detail).into_owned(),
    );
    let why = found_by(&title, &format!("{shown} {line}"));
    setting_frame(&title, div().id(gpui::ElementId::Name(group.clone())))
        .group(group.clone())
        .flex()
        .items_center()
        .gap_3()
        .min_h(px(44.))
        .py(px(5.))
        .border_b_1()
        .border_color(theme::island_border())
        .children(reset.is_some().then(|| {
            div()
                .flex_none()
                .size(px(6.))
                .rounded_full()
                .bg(theme::accent())
        }))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(found_marked(shown))
                .children((!line.is_empty()).then(|| {
                    div()
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(found_marked(line))
                }))
                .children(why.map(|word| {
                    div()
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(found_marked(word))
                })),
        )
        .children(reset.map(|reset| {
            div()
                .id(gpui::ElementId::Name(format!("{group}-reset").into()))
                .flex_none()
                .px(px(8.))
                .h(px(24.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .border_1()
                .border_color(theme::island_border())
                .text_size(px(12.))
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .invisible()
                .group_hover(group.clone(), |s| s.visible())
                .child("Reset")
                .on_click(reset)
        }))
        .child(div().flex_none().child(control))
}

/// How Settings' rows are drawn this time (#203), set by the window root as it draws Settings (`settings_look`).
#[derive(Clone, Default)]
pub struct SettingsLook {
    /// The search's words, as typed; none while it is empty.
    pub query: Vec<String>,
    /// What each setting answers to beyond its name and line, and its section's title, by its name: a row found by
    /// one of these says it (`In Notifications` for the section's).
    pub words: Rc<HashMap<SharedString, (&'static str, &'static str)>>,
    /// The row a door led to (⌘K's pick), by its name: lit until another section, the search or a click.
    pub lit: Option<SharedString>,
    /// The row the keyboard is on (⇥), by its name.
    pub focus: Option<SharedString>,
}

/// What a key does on the keyboard's row (`setting_keys`): space flips it, ← → step its choice.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingKey {
    Space,
    Left,
    Right,
}

/// A row's answer to a key (`setting_keys`); true when it did something.
pub type SettingAct = Rc<dyn Fn(SettingKey, &mut gpui::Window, &mut gpui::App) -> bool>;

thread_local! {
    static LOOK: RefCell<SettingsLook> = RefCell::new(SettingsLook::default());
    /// The rows of Settings drawn this time, by name, in the order they are on screen.
    static DRAWN: RefCell<Vec<SharedString>> = const { RefCell::new(Vec::new()) };
    /// What each row does with a key, by name, as last built.
    static ACTS: RefCell<HashMap<SharedString, SettingAct>> = RefCell::new(HashMap::new());
}

/// Settings is drawn with `look` (the window root, each time it draws Settings); the rows drawn are counted anew.
pub fn settings_look(look: SettingsLook) {
    LOOK.with(|l| *l.borrow_mut() = look);
    DRAWN.with(|d| d.borrow_mut().clear());
}

/// Settings is not drawn: a setting's row elsewhere (Set up's) marks nothing and is lit by nothing. The rows ⇥ walks
/// stay those Settings drew last.
pub fn settings_look_off() {
    LOOK.with(|l| *l.borrow_mut() = SettingsLook::default());
}

/// The rows of Settings drawn last, by name, in the order they are on screen: what ⇥ walks.
pub fn settings_drawn() -> Vec<SharedString> {
    DRAWN.with(|d| d.borrow().clone())
}

/// What row `name` does with keys on the keyboard's row; set as it is built.
pub fn setting_keys(name: impl Into<SharedString>, act: SettingAct) {
    ACTS.with(|a| a.borrow_mut().insert(name.into(), act));
}

/// Row `name`'s answer to `key`, if it has one.
pub fn setting_act(name: &str) -> Option<SettingAct> {
    ACTS.with(|a| a.borrow().get(name).cloned())
}

/// A row of Settings drawn its own way (an agent's, a host's) framed as `setting_row` frames its own: lit when a door
/// led to it, ringed when the keyboard is on it, and counted among the rows ⇥ walks.
pub fn setting_frame<E: Styled + ParentElement + FluentBuilder>(name: &SharedString, row: E) -> E {
    let (lit, focus) = LOOK.with(|l| {
        let l = l.borrow();
        (l.lit.as_ref() == Some(name), l.focus.as_ref() == Some(name))
    });
    let counted = name.clone();
    // Lit or ringed, the row reaches 8 px past its column on each side, its words where they were; the bar and the
    // ring are drawn over it, so the row's own borders stay.
    row.relative()
        .when(lit || focus, |r| r.mx(px(-8.)).px(px(8.)))
        .when(lit, |r| {
            r.bg(theme::info_bg()).child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(2.))
                    .bg(theme::accent()),
            )
        })
        .when(focus, |r| {
            r.child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(px(ROW_RADIUS))
                    .border_2()
                    .border_color(theme::focus()),
            )
        })
        .child(
            gpui::canvas(
                move |_, _, _| {
                    DRAWN.with(|d| {
                        let mut d = d.borrow_mut();
                        if !d.contains(&counted) {
                            d.push(counted);
                        }
                    })
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_0(),
        )
}

/// `text` with the search's words marked (#203), as the lists' filters mark theirs.
pub fn found_marked(text: impl Into<SharedString>) -> gpui::StyledText {
    let text: SharedString = text.into();
    let query = LOOK.with(|l| l.borrow().query.clone());
    let low = text.to_ascii_lowercase();
    let mut spans: Vec<std::ops::Range<usize>> = (query.iter())
        .filter(|w| !w.is_empty())
        .flat_map(|w| {
            let w = w.to_ascii_lowercase();
            (low.match_indices(w.as_str()))
                .map(|(at, m)| at..at + m.len())
                .collect::<Vec<_>>()
        })
        .collect();
    spans.sort_by_key(|r| r.start);
    let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
    for r in spans {
        match merged.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => merged.push(r),
        }
    }
    let mark = gpui::HighlightStyle {
        color: Some(theme::text().into()),
        background_color: Some(theme::warning_bg().into()),
        ..Default::default()
    };
    gpui::StyledText::new(text).with_highlights(merged.into_iter().map(|r| (r, mark)))
}

/// The word setting `name` was found by when what was typed is not in `seen` (its name and line): the first of its
/// words holding it (`dark` finds Theme by `dark`).
pub fn found_by(name: &str, seen: &str) -> Option<String> {
    LOOK.with(|l| {
        let l = l.borrow();
        let seen = seen.to_lowercase();
        let missing: Vec<String> = (l.query.iter())
            .map(|w| w.to_lowercase())
            .filter(|w| !w.is_empty() && !seen.contains(w.as_str()))
            .collect();
        let (words, section) = l.words.get(name)?;
        let answers = |text: &str| {
            missing
                .iter()
                .any(|w| text.to_lowercase().contains(w.as_str()))
        };
        (words.split_whitespace())
            .find(|word| answers(word))
            .map(str::to_owned)
            .or_else(|| answers(section).then(|| format!("In {section}")))
    })
}

/// A theme's swatch (#204): its ground, a short line and a long one in two of its code colours, 0xRRGGBBAA; 26 × 16
/// on a row's button, 22 × 14 in a list.
pub fn swatch([ground, short, long]: [u32; 3], small: bool) -> Div {
    div()
        .flex_none()
        .w(px(if small { 22. } else { 26. }))
        .h(px(if small { 14. } else { 16. }))
        .px(px(4.))
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(1.))
        .rounded(px(2.))
        .border_1()
        .border_color(theme::island_border())
        .bg(gpui::rgba(ground))
        .child(
            div()
                .w(gpui::relative(0.7))
                .h(px(2.))
                .rounded(px(1.))
                .bg(gpui::rgba(short)),
        )
        .child(
            div()
                .w_full()
                .h(px(2.))
                .rounded(px(1.))
                .bg(gpui::rgba(long)),
        )
}

/// A part's title over its rows, with a line under it if there is one (Settings → Appearance's Window, Fonts and
/// size).
pub fn subhead(title: &'static str, sub: &'static str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(
            div()
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .children((!sub.is_empty()).then(|| {
            div()
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(sub)
        }))
}

/// The foot of a tool window, under its body that scrolls: where its buttons are.
pub fn tool_foot() -> Div {
    div()
        .flex_none()
        .px(px(14.))
        .py(px(10.))
        .border_t_1()
        .border_color(theme::island_border())
}

/// A group's heading in a tool window's list; `keys` says how to walk it.
pub fn group_heading(text: String, keys: Option<&'static str>) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .px(px(14.))
        .pt(px(10.))
        .pb(px(4.))
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::muted())
        .child(div().flex_1().child(text.to_uppercase()))
        .children(keys.map(|k| keys::caps(k, keys::On::Plain)))
}

/// A comment as one entry of a tool window's list: a dot in its colour, where it is, two lines of it.
pub fn comment_item(
    id: (&'static str, usize),
    tone: gpui::Rgba,
    where_: String,
    body: String,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .mx(px(6.))
        .px(px(8.))
        .py(px(4.))
        .overflow_hidden()
        .rounded(px(ROW_RADIUS))
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().flex_none().size(px(6.)).rounded_full().bg(tone))
                .child(
                    hover::cut("where", where_)
                        .flex_1()
                        .font_family(theme::code_font())
                        .text_size(px(11.))
                        .text_color(theme::faint()),
                ),
        )
        .child(
            div().pl(px(14.)).child(
                hover::cut_to("body", body, 2)
                    .text_size(px(12.))
                    .text_color(theme::muted()),
            ),
        )
}

/// Rows still loading: two grey bars each, breathing as the diff's shimmer does.
pub fn skeleton(id: &'static str, rows: usize) -> gpui::AnyElement {
    const LONG: [f32; 6] = [62., 48., 70., 55., 66., 44.];
    const SHORT: [f32; 6] = [40., 52., 34., 46., 38., 50.];
    motion::breathe(
        id,
        div()
            .flex()
            .flex_col()
            .gap(px(22.))
            .children((0..rows).map(|n| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(7.))
                    .child(bar(LONG[n % 6], 10.))
                    .child(bar(SHORT[n % 6], 7.))
            })),
    )
}

fn bar(percent: f32, height: f32) -> Div {
    div()
        .w(gpui::relative(percent / 100.))
        .h(px(height))
        .rounded_full()
        .bg(theme::hover())
}

/// A small toggle in the diff toolbar.
pub fn chip(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    active: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_2()
        .py(px(2.))
        .rounded(px(ROW_RADIUS))
        .text_size(px(12.))
        .cursor_pointer()
        .when(active, |s| {
            s.bg(theme::selected()).text_color(theme::text())
        })
        .when(!active, |s| {
            s.text_color(theme::muted())
                .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
        })
        .child(label.into())
}

/// Chips that belong together, on a shared rounded track.
pub fn group() -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(2.))
        .p(px(2.))
        .rounded(px(ROW_RADIUS + 2.))
        .bg(theme::base())
}

/// The blue dot of a row not seen yet: a request you have never opened, or one of yours a thread
/// waits on; a blank of its width otherwise.
pub fn unread_dot(on: bool) -> Div {
    div()
        .flex_none()
        .mt(px(6.))
        .size(px(7.))
        .rounded_full()
        .when(on, |s| s.bg(theme::accent()))
}

/// One message centred in the content: an icon, what is the case, and the way on; `text` only where it changes what
/// you do next (a filter's reach, what failed), none when empty (#185).
pub fn empty_state(
    icon: &'static str,
    title: String,
    text: String,
    action: Option<AnyElement>,
) -> Div {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .px(px(24.))
        .child(icons::icon(icon).size(px(24.)).text_color(theme::muted()))
        .child(
            div()
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child(title),
        )
        // Room for three lines, so the box stays put as its words change (the developer,
        // 2026-10-23).
        .when(!text.is_empty(), |s| {
            s.child(
                div()
                    .max_w(px(400.))
                    .min_h(px(19. * 3.))
                    .text_size(px(13.))
                    .line_height(px(19.))
                    .text_center()
                    .text_color(theme::muted())
                    .child(text),
            )
        })
        .children(action.map(|a| div().pt(px(6.)).child(a)))
}

/// A small rounded label in a preview's head: the pipeline, the approvals.
/// A small label in a thin ring of `color`, beside a row's title: a version's role, a commit's ref, a state.
pub fn tag(text: impl Into<SharedString>, color: Rgba) -> Div {
    div()
        .px(px(6.))
        .rounded_full()
        .border_1()
        .border_color(color)
        .text_size(px(10.))
        .text_color(color)
        .child(text.into())
}

pub fn pill(text: String, color: Rgba) -> Div {
    div()
        .flex_none()
        .px(px(8.))
        .h(px(22.))
        .flex()
        .items_center()
        .rounded_full()
        .border_1()
        .border_color(color)
        .text_size(px(12.))
        .text_color(color)
        .child(text)
}

/// The filled button of a preview, with its key.
pub fn primary_button(id: &'static str, label: &str, key: &'static str) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .h(px(28.))
        .px(px(12.))
        .flex()
        .items_center()
        .gap_2()
        .rounded(px(ROW_RADIUS))
        .bg(theme::accent())
        .text_color(theme::on_accent())
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(|s| s.opacity(0.9))
        .child(words::say(label).into_owned())
        .child(keys::caps(key, keys::On::Filled))
}

/// A plain outlined button.
pub fn outline_button(label: &str) -> Div {
    div()
        .flex_none()
        .h(px(28.))
        .px(px(10.))
        .flex()
        .items_center()
        .rounded(px(ROW_RADIUS))
        .border_1()
        .border_color(theme::island_border())
        .cursor_pointer()
        .text_size(px(13.))
        .hover(|s| s.bg(theme::hover()))
        .child(words::say(label).into_owned())
}

/// "3/7" beside a branch or request whose review started: files reviewed of the whole diff.
pub fn progress_badge(done: u32, total: u32) -> gpui::Div {
    div()
        .flex_none()
        .px(px(5.))
        .rounded_full()
        .bg(theme::hover())
        .text_size(px(11.))
        .text_color(theme::muted())
        .child(format!("{done}/{total}"))
}

/// A small pill on a section's header (the side panel's Commits: the working tree, the search), lit while `on`.
pub fn section_chip(id: &'static str, on: bool) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .h(px(22.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(5.))
        .rounded(px(ROW_RADIUS))
        .text_size(px(12.))
        .text_color(if on { theme::text() } else { theme::muted() })
        .cursor_pointer()
        .when(on, |s| s.bg(theme::selected()))
        .hover(|s| s.bg(theme::hover()))
}

/// An on/off switch.
pub fn switch(id: &'static str, on: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .w(px(34.))
        .h(px(20.))
        .rounded_full()
        .flex()
        .items_center()
        .px(px(2.))
        .cursor_pointer()
        .bg(if on { theme::accent() } else { theme::hover() })
        .when(on, |s| s.justify_end())
        .child(div().size(px(16.)).rounded_full().bg(if on {
            theme::base()
        } else {
            theme::muted()
        }))
}

/// A row of a list to choose from (the ways to connect a host, Set up's): a pick mark (`picked`), its icon, the name
/// with whose it is, what it does; the caller adds what goes on its right.
#[allow(clippy::too_many_arguments)]
pub fn choice_row(
    id: impl Into<gpui::ElementId>,
    picked: Option<bool>,
    icon: &'static str,
    name: String,
    who: Option<String>,
    what: String,
    on: bool,
    dim: bool,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(14.))
        .py(px(10.))
        .when(on, |r| r.bg(theme::hover()))
        .when(dim, |r| r.opacity(0.6))
        .children(picked.map(radio))
        .child(
            icons::icon(icon)
                .flex_none()
                .size(px(18.))
                .text_color(theme::muted()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .min_w_0()
                        .child(
                            hover::cut("name", name)
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD),
                        )
                        .children(who.map(|w| {
                            div()
                                .flex_none()
                                .text_size(px(12.))
                                .text_color(theme::muted())
                                .child(w)
                        })),
                )
                .child(
                    hover::cut("what", what)
                        .text_size(px(12.))
                        .text_color(theme::muted()),
                ),
        )
}

/// A choice row's pick mark.
fn radio(on: bool) -> Div {
    div()
        .flex_none()
        .size(px(14.))
        .rounded_full()
        .border_1()
        .border_color(if on {
            theme::accent()
        } else {
            theme::selected()
        })
        .when(on, |d| d.border_4())
}
