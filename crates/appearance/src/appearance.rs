//! Settings → Appearance (Design #12 A + C, `docs/specs/themes.md`, flow 09): the Window themes as
//! cards three to a row — tabs down the left, the code as bars in the theme's syntax colours and
//! diff tints —, the Code themes as syntax samples with "Follow the window" first, and under the
//! window root's rows your themes (the folder, Copy One Dark as JSON, each file's problems) and the
//! file icons. A theme opens in the editor: its tokens by group, contrast per text token with a fix,
//! and beside them a preview — a request's header, a piece of diff and a discussion in the theme
//! being edited, while the window keeps the chosen themes; editing a built-in theme makes a new one
//! that extends it.

use crate::cards::{
    ROWS, card, is_text, own, row_name, rows_of, sample_diff, small_button, tag_of, theme_preview,
    value, window_face,
};
use crate::themes as lists;
use artoo_core::themes::{self, TOKENS, Theme};
use artoo_shell_api::{Frame, MenuItem, Typed};
use artoo_ui_kit::controls::{setting, subhead};
use artoo_ui_kit::input::{self, Area, Edit};
use artoo_ui_kit::motion::{self, Folds, Marks};
use artoo_ui_kit::room::Heights;
use artoo_ui_kit::{ROW_RADIUS, file_icons, format, hover, icons, keys, px, theme};
use gpui::{
    AnyElement, ClickEvent, Context, Div, FontWeight, Keystroke, Pixels, Point, Window, div,
    prelude::*, rgba,
};
use std::rc::Rc;

/// A theme's swatch (#204): its background with two of its code colours on it.
fn swatch(t: &Theme) -> Div {
    use artoo_core::themes::T;
    let c = |tok: T| rgba(t.get(tok));
    div()
        .flex_none()
        .size(px(16.))
        .rounded(px(4.))
        .border_1()
        .border_color(theme::island_border())
        .bg(c(T::Bg3))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(2.))
        .child(div().size(px(4.)).rounded_full().bg(c(T::Code1)))
        .child(div().size(px(4.)).rounded_full().bg(c(T::Code3)))
}

/// The hex field's id as the tab's input.
pub(crate) const INPUT: &str = "appearance.value";

/// A theme being edited.
pub struct ThemeEdit {
    pub theme: Theme,
    /// As it was when the editor opened: Revert goes back to it.
    start: Theme,
    /// What it extends, for "Use One Dark's".
    base: Theme,
    /// A copy made from a built-in theme (named `from`).
    from: Option<String>,
    /// The token whose value is open.
    pub selected: Option<usize>,
    /// Its colour as the picker holds it: hue, saturation, value, alpha — kept apart from the
    /// value, so a grey keeps the hue it was dragged to.
    hsva: [f32; 4],
    /// The last save's outcome, said beside Save.
    said: Option<Result<String, String>>,
}

/// A tab's Appearance: the theme being edited, the hex field of its token open, how they move.
pub(crate) struct Appearance {
    frame: Rc<dyn Frame>,
    edit: Option<ThemeEdit>,
    /// The hex field's text, and the field as typed in.
    value: String,
    area: Area,
    /// Whether the hex field is open (a token picked, not left).
    typing: bool,
    motion: Marks,
    folds: Folds,
    heights: Heights,
}

impl Appearance {
    pub(crate) fn new(frame: Rc<dyn Frame>) -> Self {
        Self {
            frame,
            edit: None,
            value: String::new(),
            area: Area::default(),
            typing: false,
            motion: Marks::default(),
            folds: Folds::default(),
            heights: Heights::default(),
        }
    }

    pub(crate) fn editing(&self) -> bool {
        self.edit.is_some()
    }

    pub(crate) fn typing(&self) -> bool {
        self.typing
    }

    /// The hex field's selected text, for Copy.
    pub(crate) fn selected(&self) -> Option<String> {
        self.area.edit.selected_text().map(str::to_owned)
    }

    /// A key while the hex field has the keyboard: a whole colour typed is applied; ↵ and esc let it go.
    pub(crate) fn key(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> Typed {
        let typed = match input::edit(&mut self.value, &mut self.area.edit, key, cx) {
            Edit::Changed => {
                self.value_changed(cx);
                Typed::Kept
            }
            Edit::Enter { .. } | Edit::Escape => {
                self.typing = false;
                Typed::Leave
            }
            Edit::Selected | Edit::Ignored => Typed::Kept,
        };
        cx.notify();
        typed
    }

    /// The hex field let the keyboard go (a press outside it, another page): it closes, its token still open.
    pub(crate) fn leave_field(&mut self, cx: &mut Context<Self>) {
        if self.typing {
            self.typing = false;
            cx.notify();
        }
    }

    /// The hex field takes the keyboard; a field new to it starts with its caret at the end.
    fn start_field(&mut self, cx: &mut Context<Self>) {
        if !self.typing {
            self.area = Area::default();
        }
        self.typing = true;
        self.frame.take_keyboard(INPUT, cx);
        cx.notify();
    }

    /// Motion `key` marked now: drawn again once it is over, so it is drawn still.
    fn mark_motion(&mut self, key: &str, cx: &mut Context<Self>) {
        self.motion.mark(key);
        motion::redraw_after(cx);
    }

    /// `content` folding open or shut under `key`, while motion `moving` is fresh.
    fn folding(
        &self,
        key: impl Into<String>,
        moving: &str,
        open: bool,
        content: impl IntoElement + 'static,
    ) -> Option<AnyElement> {
        motion::folding(
            &self.folds,
            &self.heights,
            self.motion.recent(moving),
            key,
            open,
            content,
        )
    }

    /// The Window themes as cards, the chosen one framed; a card's Edit opens the editor
    /// (Settings → Appearance, and Set up's Look).
    pub(crate) fn window_cards(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let choice = theme::choice();
        theme::all()
            .iter()
            .filter(|t| t.window)
            .enumerate()
            .map(|(i, t)| {
                let id = t.id.clone();
                let edit = t.id.clone();
                let edit_button = div()
                    .id(("win-edit", i))
                    .px(px(6.))
                    .rounded(px(4.))
                    .text_size(px(11.))
                    .text_color(theme::accent())
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::selected()))
                    .child("Edit")
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.start_edit(&edit, cx);
                    }))
                    .into_any_element();
                card(
                    ("win-theme", i),
                    window_face(t),
                    t.label.clone(),
                    tag_of(t).map(|(word, bad)| (word, bad, None)),
                    choice.window == t.id,
                    Some(edit_button),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    let mut c = theme::choice();
                    c.window = id.clone();
                    this.choose(c, cx);
                }))
                .into_any_element()
            })
            .collect()
    }

    /// Its rows over the window root's (#204): Theme and Code theme, each one row opening every theme in one list
    /// (`themes.rs`) — ↑↓ tries it live —, Edit beside the theme in force, and the sample in both; or the editor,
    /// filling the section.
    pub(crate) fn render_themes(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.edit.is_some() {
            return self.render_editor(cx);
        }
        let (window, code) = in_force();
        div()
            .flex()
            .flex_col()
            .child(self.render_theme_row(cx))
            .child(self.render_code_row(cx))
            .child(
                div()
                    .mt(px(10.))
                    .rounded(px(8.))
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme::island_border())
                    .child(sample_diff(&window, code.as_ref().unwrap_or(&window))),
            )
            .into_any_element()
    }

    /// The Theme row: Edit, and the theme in force opening the list (here, and Settings' search, #203).
    pub(crate) fn render_theme_row(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let (window, _) = in_force();
        let edit = window.id.clone();
        let theme_row = div()
            .flex()
            .items_center()
            .gap_2()
            .child(small_button("theme-edit", "Edit").on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.start_edit(&edit, cx)),
            ))
            .child(self.theme_picker(
                "theme-pick",
                &window,
                window.label.clone(),
                lists::THEMES,
                cx,
            ));
        setting("Theme", "", theme_row).into_any_element()
    }

    /// The Code theme row: the code's theme, or the window's, opening its list (here, and Settings' search, #203).
    pub(crate) fn render_code_row(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let (window, code) = in_force();
        let code_row = self.theme_picker(
            "code-theme-pick",
            code.as_ref().unwrap_or(&window),
            code.as_ref()
                .map_or(lists::SAME.to_owned(), |t| t.label.clone()),
            lists::CODE_THEMES,
            cx,
        );
        setting("Code theme", "", code_row).into_any_element()
    }

    /// A theme row's button: the theme's swatch and name, ▾; a click opens `list` (#204).
    fn theme_picker(
        &self,
        id: &'static str,
        t: &Theme,
        label: String,
        list: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .id(id)
            .min_w(px(190.))
            .h(px(28.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(ROW_RADIUS))
            .bg(theme::hover())
            .cursor_pointer()
            .hover(|s| s.bg(theme::selected()))
            .child(swatch(t))
            .child(div().flex_1().text_size(px(13.)).child(label))
            .child(
                icons::icon("chevron")
                    .size(px(12.))
                    .text_color(theme::muted()),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                let frame = this.frame.clone();
                frame.search(list, cx);
            }))
    }

    /// Its rows under the window root's: your themes, then the file icons.
    pub(crate) fn render_yours(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let all = theme::all();
        let folder = themes::folder()
            .map(|f| format::home_relative(&f))
            .unwrap_or_default();
        let troubled: Vec<&Theme> = all
            .iter()
            .filter(|t| !t.built_in && !t.problems.is_empty())
            .collect();
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(subhead("Your themes", "JSON files in the themes folder."))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        small_button("themes-folder", "Open themes folder").on_click(cx.listener(
                            |_, _: &ClickEvent, _, cx| {
                                if let Some(dir) = themes::folder() {
                                    let _ = std::fs::create_dir_all(&dir);
                                    cx.reveal_path(&dir);
                                }
                            },
                        )),
                    )
                    .child(
                        small_button("themes-copy", "Copy One Dark as JSON").on_click(cx.listener(
                            |_, _: &ClickEvent, _, cx| {
                                let json = theme::all()[0].to_json(true);
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(json));
                            },
                        )),
                    )
                    .child(
                        hover::cut("themes-folder", folder)
                            .font_family(theme::code_font())
                            .text_size(px(11.))
                            .text_color(theme::faint()),
                    ),
            )
            .children(troubled.into_iter().map(|t| {
                div()
                    .px(px(12.))
                    .py(px(8.))
                    .rounded(px(ROW_RADIUS))
                    .bg(theme::danger_bg())
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .text_size(px(12.))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(format!(
                        "{} — {}",
                        t.label,
                        tag_of(t).unwrap_or_default().0
                    )))
                    .children(t.problems.iter().map(|p| div().child(format!("· {p}"))))
            }))
            .child(self.render_file_icons(cx))
            .into_any_element()
    }

    /// What Set up's Look draws of it: the Window themes as Settings draws them, five to a row, then Code colours.
    pub(crate) fn render_short(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let code = theme::choice()
            .code
            .as_deref()
            .and_then(theme::find)
            .map_or(lists::SAME.to_owned(), |t| t.label);
        let code_picker =
            div()
                .id("setup-code-theme")
                .min_w(px(190.))
                .h(px(28.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(ROW_RADIUS))
                .bg(theme::hover())
                .cursor_pointer()
                .hover(|s| s.bg(theme::selected()))
                .child(div().flex_1().text_size(px(13.)).child(code))
                .child(
                    icons::icon("chevron")
                        .size(px(12.))
                        .text_color(theme::muted()),
                )
                .on_click(cx.listener(|this, ev: &ClickEvent, _, cx| {
                    this.open_code_themes(ev.position(), cx)
                }));
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(rows_of(5, self.window_cards(cx)))
            .child(setting(
                "Code colours",
                "Syntax and the diff's tints.",
                code_picker,
            ))
            .into_any_element()
    }

    /// Code colours' menu: Follow the window, then every code theme, the one in use checked.
    fn open_code_themes(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let current = theme::choice().code;
        let me = cx.weak_entity();
        let choose = move |id: Option<String>| {
            let me = me.clone();
            move |_: &mut Window, cx: &mut gpui::App| {
                me.update(cx, |this, cx| {
                    let mut choice = theme::choice();
                    choice.code = id.clone();
                    this.choose(choice, cx);
                })
                .ok();
            }
        };
        let follow = MenuItem::new(lists::SAME, choose(None)).checked(current.is_none());
        let themes = theme::all()
            .into_iter()
            .filter(|t| t.code)
            .map(|t| {
                let on = current.as_deref() == Some(t.id.as_str());
                MenuItem::new(t.label, choose(Some(t.id))).checked(on)
            })
            .collect();
        self.frame
            .titled_menu(at, vec![(None, vec![follow]), (Some("Code"), themes)], cx);
    }

    /// File icons (#95): what they are, a few of them, the user's folder and what it gives.
    fn render_file_icons(&self, cx: &mut Context<Self>) -> AnyElement {
        let (svgs, rules) = file_icons::counts();
        let problems = file_icons::problems();
        let folder = file_icons::folder()
            .map(|d| format::home_relative(&d))
            .unwrap_or_default();
        let samples = [
            ("LoginViewModel.kt", false),
            ("build.gradle.kts", false),
            ("Main.java", false),
            ("package.json", false),
            ("README.md", false),
            ("Dockerfile", false),
            ("core", true),
            (".github", true),
        ];
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(subhead(
                "File icons",
                "Yours in the icons folder: Dockerfile.svg, kt.svg or folder.svg, _dark for dark themes; folders.txt and files.txt: a path's regular expression, then the icon — (^|/)src/test$ testRoot.",
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x(px(14.))
                    .gap_y(px(6.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .children(samples.into_iter().map(|(name, is_folder)| {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(if is_folder {
                                icons::folder_icon(name, 16.)
                            } else {
                                icons::file_icon(name, 16.)
                            })
                            .child(name)
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        small_button("icons-folder", "Open icons folder").on_click(cx.listener(
                            |_, _: &ClickEvent, _, cx| {
                                if let Some(dir) = file_icons::folder() {
                                    file_icons::start_folder(&dir);
                                    cx.reveal_path(&dir);
                                }
                            },
                        )),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(11.))
                            .text_color(theme::muted())
                            .child(format!(
                                "{} · {}",
                                format::plural(svgs, "icon"),
                                format::plural(rules, "rule")
                            )),
                    )
                    .child(
                        hover::cut("icons-folder", folder)
                            .font_family(theme::code_font())
                            .text_size(px(11.))
                            .text_color(theme::faint()),
                    ),
            )
            .children((!problems.is_empty()).then(|| {
                div()
                    .px(px(12.))
                    .py(px(8.))
                    .rounded(px(ROW_RADIUS))
                    .bg(theme::danger_bg())
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .text_size(px(12.))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Icon rules"))
                    .children(problems.into_iter().map(|p| div().child(format!("· {p}"))))
            }))
            .into_any_element()
    }

    /// Draws with `choice` in every window and keeps it.
    pub(crate) fn choose(&mut self, choice: theme::Choice, cx: &mut Context<Self>) {
        theme::choose(choice);
        cx.refresh_windows();
        cx.notify();
    }

    /// Opens theme `id` in the editor; a built-in theme is copied first.
    pub(crate) fn start_edit(&mut self, id: &str, cx: &mut Context<Self>) {
        let all = theme::all();
        let Some(t) = all.iter().find(|t| t.id == id).cloned() else {
            return;
        };
        let (theme, from) = if t.built_in {
            let mut name = format!("my-{}", t.id);
            let mut n = 2;
            while all.iter().any(|o| o.id == name) {
                name = format!("my-{}-{n}", t.id);
                n += 1;
            }
            (
                Theme {
                    id: name.clone(),
                    label: name,
                    built_in: false,
                    extends: Some(t.id.clone()),
                    own: Vec::new(),
                    file: None,
                    ..t.clone()
                },
                Some(t.label.clone()),
            )
        } else {
            (t.clone(), None)
        };
        let base = theme
            .extends
            .as_deref()
            .and_then(|e| all.iter().find(|o| o.id == e))
            .unwrap_or(&all[0])
            .clone();
        self.edit = Some(ThemeEdit {
            start: theme.clone(),
            theme,
            base,
            from,
            selected: None,
            hsva: [0., 0., 0., 1.],
            said: None,
        });
        cx.notify();
    }

    /// Leaves the editor.
    /// Esc in the theme editor: the token being edited lets go, then the editor closes — Settings
    /// stays (the developer, 2026-10-23). Whether it was in the editor.
    pub(crate) fn escape(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(edit) = &mut self.edit else {
            return false;
        };
        if edit.selected.take().is_some() {
            self.mark_motion("token-open", cx);
            self.typing = false;
            cx.notify();
        } else {
            self.close_edit(cx);
        }
        true
    }

    pub(crate) fn close_edit(&mut self, cx: &mut Context<Self>) {
        self.edit = None;
        self.typing = false;
        cx.notify();
    }

    /// The themes the one being edited may extend: any but itself and those built on it.
    fn bases(&self) -> Vec<Theme> {
        let Some(edit) = &self.edit else {
            return Vec::new();
        };
        let all = theme::all();
        let me = edit.theme.id.as_str();
        // Whether `t` is `me` or extends it, however far down.
        let built_on_me = |t: &Theme| {
            let mut at = Some(t);
            for _ in 0..all.len() + 1 {
                let Some(t) = at else {
                    return false;
                };
                if t.id == me {
                    return true;
                }
                at = t
                    .extends
                    .as_deref()
                    .and_then(|e| all.iter().find(|o| o.id == e));
            }
            true
        };
        all.iter().filter(|t| !built_on_me(t)).cloned().collect()
    }

    /// The extends ▾ menu: the themes it may be built on, the current one checked.
    pub(crate) fn open_bases(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let me = cx.weak_entity();
        let current = self
            .edit
            .as_ref()
            .map(|e| e.base.id.clone())
            .unwrap_or_default();
        let (built_in, yours): (Vec<Theme>, Vec<Theme>) =
            self.bases().into_iter().partition(|t| t.built_in);
        let entries = |themes: Vec<Theme>| {
            themes
                .into_iter()
                .map(|t| {
                    let (id, me) = (t.id.clone(), me.clone());
                    MenuItem::new(t.label.clone(), move |_, cx| {
                        me.update(cx, |this, cx| this.set_base(&id, cx)).ok();
                    })
                    .checked(t.id == current)
                })
                .collect::<Vec<_>>()
        };
        self.frame.titled_menu(
            at,
            vec![
                (Some("Extends"), entries(built_in)),
                (Some("Your themes"), entries(yours)),
            ],
            cx,
        );
    }

    /// The theme being edited is built on `id` now: what it does not set comes from there.
    pub(crate) fn set_base(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(base) = theme::find(id) else {
            return;
        };
        let Some(edit) = &mut self.edit else {
            return;
        };
        for i in 0..TOKENS.len() {
            if !edit.theme.own.contains(&i) {
                edit.theme.colors[i] = base.colors[i];
            }
        }
        for r in 0..themes::ROLES {
            if !edit.theme.own_roles.contains(&r) {
                edit.theme.roles[r] = base.roles[r];
            }
        }
        edit.theme.extends = Some(base.id.clone());
        edit.base = base;
        edit.said = None;
        self.show_value();
        cx.notify();
    }

    pub(crate) fn pick_token(&mut self, i: usize, cx: &mut Context<Self>) {
        self.mark_motion("token-open", cx);
        let Some(edit) = &mut self.edit else {
            return;
        };
        if edit.selected == Some(i) {
            edit.selected = None;
            self.typing = false;
            return cx.notify();
        }
        edit.selected = Some(i);
        self.show_value();
        self.start_field(cx);
    }

    /// The selected token's value into the hex field and the picker, after it changed elsewhere.
    fn show_value(&mut self) {
        let Some(edit) = &mut self.edit else {
            return;
        };
        if let Some(i) = edit.selected {
            let c = value(&edit.theme, i);
            self.value = themes::hex(c);
            edit.hsva = themes::to_hsva(c);
        }
    }

    /// The picker moved: the token takes its colour, the hex field follows.
    fn pick_color(&mut self, hsva: [f32; 4], cx: &mut Context<Self>) {
        let Some(edit) = &mut self.edit else {
            return;
        };
        let Some(i) = edit.selected else {
            return;
        };
        edit.hsva = hsva;
        let c = themes::from_hsva(hsva);
        self.value = themes::hex(c);
        self.set_token(i, c, cx);
    }

    /// Token `i` of the theme being edited becomes `color`, drawn at once.
    fn set_token(&mut self, i: usize, color: u32, cx: &mut Context<Self>) {
        let Some(edit) = &mut self.edit else {
            return;
        };
        match i.checked_sub(TOKENS.len()) {
            None => {
                edit.theme.colors[i] = color;
                if !edit.theme.own.contains(&i) {
                    edit.theme.own.push(i);
                }
            }
            Some(r) => {
                edit.theme.roles[r] = Some(color);
                if !edit.theme.own_roles.contains(&r) {
                    edit.theme.own_roles.push(r);
                }
            }
        }
        edit.said = None;
        cx.notify();
    }

    /// Row `i` back to what the theme it extends has.
    fn reset_token(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(edit) = &mut self.edit else {
            return;
        };
        match i.checked_sub(TOKENS.len()) {
            None => {
                edit.theme.colors[i] = edit.base.colors[i];
                edit.theme.own.retain(|&o| o != i);
            }
            Some(r) => {
                edit.theme.roles[r] = edit.base.roles[r];
                edit.theme.own_roles.retain(|&o| o != r);
            }
        }
        edit.said = None;
        self.show_value();
        cx.notify();
    }

    /// The hex field changed: a whole colour is applied.
    pub(crate) fn value_changed(&mut self, cx: &mut Context<Self>) {
        let Some(i) = self.edit.as_ref().and_then(|e| e.selected) else {
            return;
        };
        if let Some(color) = themes::parse_hex(&self.value) {
            if let Some(edit) = &mut self.edit {
                edit.hsva = themes::to_hsva(color);
            }
            self.set_token(i, color, cx);
        }
    }

    /// Every text token under 4.5:1 raised to it.
    pub(crate) fn raise_all(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = &self.edit else {
            return;
        };
        let low: Vec<(usize, u32)> = edit
            .theme
            .low_contrast()
            .into_iter()
            .map(|(t, _, _)| (t as usize, themes::raise(&edit.theme, edit.theme.get(t))))
            .collect();
        for (i, color) in low {
            self.set_token(i, color, cx);
        }
        self.show_value();
    }

    /// Writes the theme to its file in the themes folder and draws with it (Save, ⌘S).
    pub(crate) fn save(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = &mut self.edit else {
            return;
        };
        let Some(dir) = themes::folder() else {
            return;
        };
        let file = edit
            .theme
            .file
            .clone()
            .unwrap_or_else(|| dir.join(format!("{}.json", edit.theme.id)));
        let json = edit.theme.to_json(false);
        // A snapshot run draws a state; it never writes the user's files.
        let snapshot = artoo_core::storage::snapshot_run();
        let result = if snapshot {
            Ok(())
        } else {
            std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&file, json))
        };
        match result {
            Ok(()) => {
                edit.theme.file = Some(file.clone());
                edit.start = edit.theme.clone();
                edit.from = None;
                edit.said = Some(Ok(format!("Saved {}", format::home_relative(&file))));
                let id = edit.theme.id.clone();
                theme::reload();
                // Drawn with where it is offered: a theme of code only is the Code choice.
                let mut choice = theme::choice();
                if edit.theme.window {
                    choice.window = id;
                } else {
                    choice.code = Some(id);
                }
                theme::choose(choice);
            }
            Err(e) => edit.said = Some(Err(format!("Not saved: {e}"))),
        }
        cx.refresh_windows();
        cx.notify();
    }

    fn revert(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = &mut self.edit else {
            return;
        };
        edit.theme = edit.start.clone();
        edit.said = None;
        self.show_value();
        cx.notify();
    }

    fn render_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(edit) = &self.edit else {
            return div().into_any_element();
        };
        let t = &edit.theme;
        let low = t.low_contrast();
        let worst = |i: usize| {
            themes::SURFACES
                .iter()
                .map(|&s| themes::contrast(value(t, i), t.get(s)))
                .fold(f32::MAX, f32::min)
        };
        let mut list: Vec<AnyElement> = Vec::new();
        let mut group = "";
        for i in 0..ROWS {
            let g = TOKENS.get(i).map_or("Code roles", |(_, g, _)| g);
            let (name, what) = row_name(i);
            if g != group {
                group = g;
                list.push(
                    div()
                        .pt(px(12.))
                        .pb(px(4.))
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::muted())
                        .child(g.to_uppercase())
                        .into_any_element(),
                );
            }
            let own = own(t, i);
            let selected = edit.selected == Some(i);
            let ratio = is_text(i).then(|| worst(i));
            list.push(
                div()
                    .id(("theme-token", i))
                    .h(px(30.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .rounded(px(ROW_RADIUS))
                    .text_size(px(12.))
                    .cursor_pointer()
                    // The token being edited is framed in the focus colour, its panel under it.
                    .border_1()
                    .border_color(if selected {
                        theme::focus()
                    } else {
                        gpui::transparent_black().into()
                    })
                    .when(selected, |s| s.bg(theme::selected()))
                    .when(!selected, |s| s.hover(|s| s.bg(theme::hover())))
                    .child(
                        div()
                            .size(px(18.))
                            .flex_none()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(theme::island_border())
                            .bg(rgba(value(t, i))),
                    )
                    .child(
                        div()
                            .w(px(150.))
                            .flex_none()
                            .font_family(theme::code_font())
                            .text_color(if own { theme::text() } else { theme::faint() })
                            .child(name),
                    )
                    .child(
                        div()
                            .w(px(90.))
                            .flex_none()
                            .font_family(theme::code_font())
                            .text_color(if own { theme::text() } else { theme::faint() })
                            .child(themes::hex(value(t, i))),
                    )
                    .child(
                        hover::cut(("token-what", i), what)
                            .flex_1()
                            .text_color(theme::muted()),
                    )
                    .children(ratio.map(|r| {
                        let ok = r >= 4.5;
                        div()
                            .flex_none()
                            .text_size(px(11.))
                            .text_color(if ok { theme::muted() } else { theme::removed() })
                            .when(!ok, |s| s.font_weight(FontWeight::SEMIBOLD))
                            .child(if ok {
                                format!("✓ {r:.1}:1")
                            } else {
                                format!("{r:.1}:1")
                            })
                    }))
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_token(i, cx)),
                    )
                    .into_any_element(),
            );
            // The token's panel opens under it and folds back, in size and alpha (#56, 14; #110).
            // Not shrunk by the scrolling column: in it, it was squeezed to nothing.
            if selected || self.motion.recent("token-open") {
                list.extend(self.folding(
                    format!("token-open-{i}"),
                    "token-open",
                    selected,
                    div().flex_none().child(self.render_token_open(i, cx)),
                ));
            }
        }
        let set = t.own.len() + t.own_roles.len();
        let from_label = edit.base.label.clone();
        let file = t
            .file
            .clone()
            .or_else(|| themes::folder().map(|d| d.join(format!("{}.json", t.id))))
            .map(|f| format::home_relative(&f))
            .unwrap_or_default();
        let open_json = t.file.clone();
        // The tokens scroll on their own, the preview stays beside them (Design: sticky).
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(t.label.clone()),
                    )
                    .child(
                        div()
                            .id("theme-extends")
                            .px(px(8.))
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .rounded(px(ROW_RADIUS))
                            .bg(theme::hover())
                            .text_size(px(12.))
                            .text_color(theme::muted())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::selected()))
                            .child(format!("extends {from_label}"))
                            .child(
                                icons::icon("chevron")
                                    .size(px(12.))
                                    .text_color(theme::muted()),
                            )
                            .on_click(cx.listener(|this, ev: &ClickEvent, _, cx| {
                                this.open_bases(ev.position(), cx)
                            })),
                    )
                    .child(div().flex_1())
                    .children(open_json.map(|f| {
                        small_button("theme-open-json", "Open JSON")
                            .on_click(move |_: &ClickEvent, _, cx| cx.reveal_path(&f))
                    }))
                    .child(
                        small_button("theme-revert", "Revert")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.revert(cx))),
                    )
                    .child(
                        div()
                            .id("theme-save")
                            .px(px(12.))
                            .h(px(26.))
                            .flex()
                            .items_center()
                            .rounded(px(ROW_RADIUS))
                            .bg(theme::accent())
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::on_accent())
                            .cursor_pointer()
                            .gap(px(6.))
                            .child("Save")
                            .child(
                                div()
                                    .font_weight(FontWeight::NORMAL)
                                    .child(keys::text("⌘S").into_owned()),
                            )
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx))),
                    )
                    .child(
                        small_button("theme-done", "Done").on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| this.close_edit(cx)),
                        ),
                    ),
            )
            .children(edit.from.as_ref().map(|from| {
                div()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(format!("A copy of {from}"))
            }))
            .children(edit.said.as_ref().map(|said| {
                let (text, bad) = match said {
                    Ok(t) => (t.clone(), false),
                    Err(e) => (e.clone(), true),
                };
                div()
                    .text_size(px(12.))
                    .text_color(if bad {
                        theme::removed()
                    } else {
                        theme::added()
                    })
                    .child(text)
            }))
            // Always one line of the same height, problems or none: a colour dragged across
            // 4.5:1 changes its words, never the page under it (the developer, 2026-10-23).
            .child({
                let bad = low.first().copied();
                div()
                    .h(px(36.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(12.))
                    .rounded(px(ROW_RADIUS))
                    .bg(if bad.is_some() {
                        theme::danger_bg()
                    } else {
                        theme::hover()
                    })
                    .text_size(px(12.))
                    .child(
                        hover::cut(
                            "contrast-line",
                            match bad {
                                Some((tok, ratio, on)) => format!(
                                    "{} — {} is {ratio:.1}:1 on {}; text needs 4.5:1.",
                                    match low.len() {
                                        1 => "1 problem".to_owned(),
                                        n => format!("{n} problems"),
                                    },
                                    TOKENS[tok as usize].0,
                                    TOKENS[on as usize].0
                                ),
                                None => "Every text colour reads at 4.5:1 or more.".to_owned(),
                            },
                        )
                        .flex_1(),
                    )
                    .children(bad.is_some().then(|| {
                        small_button("theme-raise-all", "Raise to 4.5:1")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.raise_all(cx)))
                    }))
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap(px(20.))
                    .child(
                        div()
                            .id("theme-tokens")
                            .flex_1()
                            .min_w(px(340.))
                            .max_w(px(470.))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .children(list),
                    )
                    .child(
                        div()
                            .id("theme-preview")
                            .flex_1()
                            .min_w(px(300.))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                                    .child(format!("Preview — the app in {}", t.label)),
                            )
                            .child(theme_preview(t)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .text_size(px(11.))
                    .text_color(theme::faint())
                    .child(format!("{set} set · {} from {from_label}", ROWS - set))
                    .child(div().flex_1())
                    .child(div().font_family(theme::code_font()).child(file)),
            )
            .into_any_element()
    }

    /// Under the selected token: its value to type, its contrast on each surface, the fixes.
    fn render_token_open(&self, i: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(edit) = &self.edit else {
            return div().into_any_element();
        };
        let t = &edit.theme;
        let text = is_text(i);
        let active = self.typing;
        let low = text
            && themes::SURFACES
                .iter()
                .any(|&s| themes::contrast(value(t, i), t.get(s)) < 4.5);
        let base_label = edit.base.label.clone();
        // Under the framed token, on the list's own ground: a line in the focus colour joins it
        // to its token, no box of its own (the developer, 2026-10-23).
        div()
            .ml(px(17.))
            .mb(px(8.))
            .pl(px(19.))
            .pr(px(8.))
            .pt(px(10.))
            .pb(px(4.))
            .border_l_2()
            .border_color(theme::focus())
            .flex()
            .flex_col()
            .gap(px(8.))
            .text_size(px(12.))
            // A panel folding away shows its own colour, not the one now picked.
            .child(self.color_picker(
                if edit.selected == Some(i) {
                    edit.hsva
                } else {
                    themes::to_hsva(value(t, i))
                },
                cx,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(artoo_shell_api::input_box(
                        div()
                            .id("theme-value")
                            .w(px(130.))
                            .h(px(26.))
                            .px_2()
                            .flex()
                            .items_center()
                            .rounded(px(ROW_RADIUS))
                            .border_2()
                            .border_color(if active {
                                theme::focus()
                            } else {
                                theme::island_border()
                            })
                            .font_family(theme::code_font())
                            .cursor_text()
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.start_field(cx)),
                            )
                            .child(self.area.field_look_of(
                                &self.value,
                                active,
                                "#rrggbb",
                                Default::default(),
                                |a: &mut Appearance| Some(&mut a.area),
                                cx.entity().downgrade(),
                            )),
                        INPUT,
                        &self.frame,
                    ))
                    .child(
                        div()
                            .text_color(theme::faint())
                            .child("#rgb, #rrggbb or #rrggbbaa — applied as you type"),
                    ),
            )
            .children(text.then(|| {
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(14.))
                    .children(themes::SURFACES.iter().map(|&s| {
                        let r = themes::contrast(value(t, i), t.get(s));
                        div()
                            .flex()
                            .gap(px(4.))
                            .text_color(theme::muted())
                            .child(format!("on {}", TOKENS[s as usize].0))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(if r >= 4.5 {
                                        theme::text()
                                    } else {
                                        theme::removed()
                                    })
                                    .child(format!("{r:.1}:1")),
                            )
                    }))
            }))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .children(low.then(|| {
                        small_button("theme-raise", "Raise to 4.5:1").on_click(cx.listener(
                            move |this, _: &ClickEvent, _, cx| {
                                let Some(edit) = &this.edit else {
                                    return;
                                };
                                let c = themes::raise(&edit.theme, value(&edit.theme, i));
                                this.set_token(i, c, cx);
                                this.show_value();
                            },
                        ))
                    }))
                    .child(
                        small_button("theme-base", format!("Use {base_label}'s")).on_click(
                            cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.reset_token(i, cx);
                            }),
                        ),
                    ),
            )
            .into_any_element()
    }

    /// The picker over the hex field: a square of the hue, saturation across and value down, and
    /// the hue and the alpha as strips beside it. A press sets it, a drag follows the pointer
    /// even past the edge.
    fn color_picker(&self, [h, s, v, a]: [f32; 4], cx: &mut Context<Self>) -> AnyElement {
        use gpui::{linear_color_stop as stop, linear_gradient};
        let solid = |hsva| rgba(themes::from_hsva(hsva));
        let white = rgba(0xffffffff);
        // A ring or a bar over a strip, where it stands.
        let bar = |y: f32| {
            div()
                .absolute()
                .left(px(-2.))
                .top(px(y * PICK_H - 3.))
                .w(px(STRIP + 4.))
                .h(px(6.))
                .rounded(px(3.))
                .border_2()
                .border_color(white)
                .shadow_sm()
        };
        let square = self
            .picker_area(Pick::Square, SQUARE, cx)
            .rounded(px(4.))
            .bg(solid([h, 1., 1., 1.]))
            .child(
                div()
                    .absolute()
                    .size_full()
                    .rounded(px(4.))
                    .bg(linear_gradient(
                        90.,
                        stop(white, 0.),
                        stop(rgba(0xffffff00), 1.),
                    )),
            )
            .child(
                div()
                    .absolute()
                    .size_full()
                    .rounded(px(4.))
                    .bg(linear_gradient(
                        180.,
                        stop(rgba(0x00000000), 0.),
                        stop(rgba(0x000000ff), 1.),
                    )),
            )
            .child(
                div()
                    .absolute()
                    .left(px(s * SQUARE - 6.))
                    .top(px((1. - v) * PICK_H - 6.))
                    .size(px(12.))
                    .rounded_full()
                    .border_2()
                    .border_color(white)
                    .shadow_sm(),
            );
        let hue = self
            .picker_area(Pick::Hue, STRIP, cx)
            .flex()
            .flex_col()
            .children((0..6).map(|k| {
                let k = k as f32;
                div()
                    .flex_1()
                    .w_full()
                    .when(k == 0., |d| d.rounded_t(px(3.)))
                    .when(k == 5., |d| d.rounded_b(px(3.)))
                    .bg(linear_gradient(
                        180.,
                        stop(solid([k * 60., 1., 1., 1.]), 0.),
                        stop(solid([(k + 1.) * 60., 1., 1., 1.]), 1.),
                    ))
            }))
            .child(bar(h / 360.));
        // Over squares, so what shows through is seen.
        let alpha = self
            .picker_area(Pick::Alpha, STRIP, cx)
            .rounded(px(3.))
            .bg(white)
            .child(
                div()
                    .absolute()
                    .size_full()
                    .rounded(px(3.))
                    .bg(gpui::checkerboard(rgba(0xc8c8c8ff), 6.)),
            )
            .child(
                div()
                    .absolute()
                    .size_full()
                    .rounded(px(3.))
                    .bg(linear_gradient(
                        180.,
                        stop(solid([h, s, v, 1.]), 0.),
                        stop(solid([h, s, v, 0.]), 1.),
                    )),
            )
            .child(bar(1. - a));
        div()
            .flex()
            .gap(px(10.))
            .child(square)
            .child(hue)
            .child(alpha)
            .into_any_element()
    }

    /// One of the picker's areas: a press there, or a drag from it, sets what it holds.
    fn picker_area(&self, which: Pick, w: f32, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        use gpui::{Bounds, DragMoveEvent, MouseButton, MouseDownEvent, Pixels};
        let area: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>> = Default::default();
        let found = area.clone();
        div()
            .id(("color-pick", which as usize))
            .relative()
            .flex_none()
            .w(px(w))
            .h(px(PICK_H))
            .cursor_crosshair()
            .child(
                gpui::canvas(move |b, _, _| found.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    if let Some(b) = area.get() {
                        this.pick_at(which, fraction(b, e.position), cx);
                    }
                }),
            )
            .on_drag(PickDrag(which), |_, _, _, cx| cx.new(|_| hover::NoTip))
            .on_drag_move(
                cx.listener(move |this, e: &DragMoveEvent<PickDrag>, _, cx| {
                    if e.drag(cx).0 == which {
                        this.pick_at(which, fraction(e.bounds, e.event.position), cx);
                    }
                }),
            )
    }

    /// The pointer at `(x, y)` (0–1 across and down) in area `which`.
    fn pick_at(&mut self, which: Pick, (x, y): (f32, f32), cx: &mut Context<Self>) {
        let Some(edit) = &self.edit else {
            return;
        };
        let [h, s, v, a] = edit.hsva;
        let hsva = match which {
            Pick::Square => [h, x, 1. - y, a],
            Pick::Hue => [y * 360., s, v, a],
            Pick::Alpha => [h, s, v, 1. - y],
        };
        self.pick_color(hsva, cx);
    }
}

/// The colour picker's square, its height, and its strips' width.
const SQUARE: f32 = 200.;
const PICK_H: f32 = 120.;
const STRIP: f32 = 14.;

/// The picker's areas.
#[derive(Clone, Copy, PartialEq)]
enum Pick {
    Square,
    Hue,
    Alpha,
}

/// A drag in the picker, by the area it started in.
struct PickDrag(Pick);

/// Where `p` is in `b`: 0–1 across and down, held at the edges.
fn fraction(b: gpui::Bounds<gpui::Pixels>, p: gpui::Point<gpui::Pixels>) -> (f32, f32) {
    let f = |p: gpui::Pixels, o: gpui::Pixels, l: gpui::Pixels| {
        if l > px(0.) {
            ((p - o) / l).clamp(0., 1.)
        } else {
            0.
        }
    };
    (
        f(p.x, b.origin.x, b.size.width),
        f(p.y, b.origin.y, b.size.height),
    )
}

/// The window's theme in force, and the code's when it has its own.
fn in_force() -> (Theme, Option<Theme>) {
    let all = theme::all();
    let choice = theme::choice();
    let window = (all.iter())
        .find(|t| t.id == choice.window)
        .unwrap_or(&all[0])
        .clone();
    let code = (choice.code.as_deref()).and_then(|id| all.iter().find(|t| t.id == id).cloned());
    (window, code)
}
