//! The Settings page: how changed words are marked, which mode diffs open in, and whether the
//! window refreshes itself. Designed in `../Design/mockups/settings/a-general.html`. Its frame — the sections, the
//! heading, Done — is drawn by the shell (`artoo_shell_api::settings`).

use super::px;
use super::{ISLAND_RADIUS, Workspace, theme};
use crate::editor::Editor;
use crate::storage::{DiffMode, MarkStyle};
use artoo_ui_kit::controls::{SettingAct, SettingKey, SettingsLook, chip, group};
use gpui::{App, ClickEvent, Context, WeakEntity, Window, div, prelude::*};
use std::collections::HashMap;
use std::rc::Rc;

/// The button of the toast saying a host connected: Settings → Merge requests, where the way in
/// changes (#156).
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = settings, no_json, no_register)]
pub(super) struct RequestsSettings;

impl Workspace {
    pub(super) fn open_settings(&mut self, cx: &mut Context<Self>) {
        if let Some(branches) = self.branches() {
            branches.drop_find(cx);
        }
        self.refilter(cx);
        // Set up's "every key in Settings → Keyboard": Settings takes its place.
        self.close_setup(cx);
        self.settings_open = true;
        self.settings_lit = None;
        self.settings_focus = None;
        cx.notify();
    }

    /// Settings → Merge requests, the hosts' (#156): where the connected toast leads.
    pub(super) fn open_requests_settings(&mut self, cx: &mut Context<Self>) {
        self.open_settings(cx);
        self.open_settings_section(REQUESTS, cx);
    }

    /// Section `section` of Settings on: its rows' owners read what they show anew (`Rows::opened`).
    pub(super) fn open_settings_section(&mut self, section: &'static str, cx: &mut Context<Self>) {
        self.settings_section = section;
        self.settings_lit = None;
        self.settings_focus = None;
        if section == super::notice_settings::NOTIFICATIONS {
            self.read_notices_allowed(cx);
        }
        let opened: Vec<_> = (self.settings_rows(section).into_iter())
            .filter_map(|r| r.opened)
            .collect();
        for read in opened {
            read(cx);
        }
    }

    /// A toast's button that opens a section of Settings, answered; false for another's.
    pub(super) fn answer_settings(
        &mut self,
        act: &dyn gpui::Action,
        cx: &mut Context<Self>,
    ) -> bool {
        let ours = act.as_any().is::<RequestsSettings>();
        if ours {
            self.open_requests_settings(cx);
        }
        ours
    }

    /// Saves the settings; the review's copy follows them (#165).
    pub(super) fn save_settings(&mut self, cx: &mut App) {
        self.settings.save();
        self.rv().set_settings(self.settings.clone(), cx);
    }

    pub(super) fn change_settings(
        &mut self,
        change: impl FnOnce(&mut Self),
        cx: &mut Context<Self>,
    ) {
        change(self);
        self.save_settings(cx);
        super::share_settings(&self.settings);
        cx.notify();
    }

    /// Settings walked by keys (#203, Design C1): ↑ ↓ the sections, ⇥ ⇧⇥ the rows, space or ↵ on the keyboard's row
    /// flips it and ← → step its choice. Heard before the keymap while Settings is open and nothing types; what the
    /// row does with the key comes back, to be done outside the tab (it acts on the tab).
    pub(super) fn settings_key(
        &mut self,
        key: &gpui::Keystroke,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<(SettingAct, SettingKey)> {
        if !self.settings_open
            || self.recording.is_some()
            || self.typing(cx).is_some()
            || self.setup_open(cx)
            || self.ctx_menu.read(cx).is_open()
            || !self.focus.contains_focused(window, cx)
        {
            return None;
        }
        let m = &key.modifiers;
        if m.control || m.alt || m.platform || m.function {
            return None;
        }
        let row_act = |key| {
            (self.settings_focus.as_deref())
                .and_then(artoo_ui_kit::controls::setting_act)
                .map(|act| (act, key))
        };
        match (key.key.as_str(), m.shift) {
            ("up" | "down", false) if self.settings_query.trim().is_empty() => {
                self.settings_step_section(key.key == "down", cx);
                cx.stop_propagation();
                None
            }
            ("tab", back) => {
                self.settings_step_row(!back, cx);
                cx.stop_propagation();
                None
            }
            ("space" | "enter", false) => row_act(SettingKey::Space),
            ("left", false) => row_act(SettingKey::Left),
            ("right", false) => row_act(SettingKey::Right),
            _ => None,
        }
    }

    /// The section above or below the one on (#203).
    pub(super) fn settings_step_section(&mut self, down: bool, cx: &mut Context<Self>) {
        let sections = artoo_shell_api::settings_sections(cx);
        let Some(at) = sections.iter().position(|s| s.id == self.settings_section) else {
            return;
        };
        let next = if down {
            (at + 1).min(sections.len() - 1)
        } else {
            at.saturating_sub(1)
        };
        if next != at {
            self.open_settings_section(sections[next].id, cx);
            cx.notify();
        }
    }

    /// The keyboard on the next row of Settings drawn (`forward`), or the one before; round from the last (#203).
    pub(super) fn settings_step_row(&mut self, forward: bool, cx: &mut Context<Self>) {
        let drawn = artoo_ui_kit::controls::settings_drawn();
        if drawn.is_empty() {
            return;
        }
        let n = drawn.len();
        let at = (self.settings_focus.as_ref()).and_then(|f| drawn.iter().position(|d| d == f));
        let next = match (at, forward) {
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
            (None, true) => 0,
            (None, false) => n - 1,
        };
        self.settings_focus = Some(drawn[next].clone());
        self.settings_lit = None;
        cx.notify();
    }

    /// The window root's rows of Settings, each with its words, in every section it draws in (#203): a row off its
    /// default carries a dot and Reset.
    fn own_rows(&self, cx: &mut Context<Self>) -> Vec<Own> {
        let s = self.settings.clone();
        let d = crate::storage::Settings::default();
        let marks = group().children(MarkStyle::ALL.map(|style| {
            chip(style_id(style), style.label(), s.mark_style == style).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.change_settings(|this| this.settings.mark_style = style, cx)
                },
            ))
        }));
        let modes = group().children(DiffMode::ALL.map(|mode| {
            chip(mode_id(mode), mode.label(), s.default_mode == mode).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.change_settings(|this| this.settings.default_mode = mode, cx);
                    // The page's choice is what the open diff shows too.
                    this.set_mode(mode, cx);
                },
            ))
        }));
        let installed: Vec<Editor> = artoo_shell_api::editors(cx)
            .iter()
            .copied()
            .filter(|e| e.installed())
            .collect();
        let current = self.editor(cx);
        // One editor found is a fact, not a switch of one (#203).
        let editors: gpui::AnyElement = match installed.as_slice() {
            [] => div()
                .text_size(px(12.))
                .text_color(theme::faint())
                .child("Android Studio, Zed or VS Code not found")
                .into_any_element(),
            [one] => div()
                .text_color(theme::muted())
                .child(one.label)
                .into_any_element(),
            _ => group()
                .children(installed.iter().map(|&e| {
                    chip(e.key, e.label, current == Some(e)).on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            this.change_settings(
                                |this| this.settings.editor = Some(e.key.to_owned()),
                                cx,
                            )
                        },
                    ))
                }))
                .into_any_element(),
        };
        let open_in = group().children(crate::storage::OpenIn::ALL.map(|o| {
            chip(o.label(), o.label(), s.open_in == o).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.change_settings(|this| this.settings.open_in = o, cx)
                },
            ))
        }));
        let projects = group().children(crate::storage::Projects::ALL.map(|p| {
            chip(p.label(), p.label(), super::projects() == p).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.change_settings(|this| this.settings.projects = p, cx)
                },
            ))
        }));
        let panes = self.panes_chips(cx);
        let animations = switch("animations", s.animations).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| this.set_animations(!this.settings.animations, cx),
        ));
        let refresh = switch("refresh", s.auto_refresh).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.change_settings(
                    |this| this.settings.auto_refresh = !this.settings.auto_refresh,
                    cx,
                )
            },
        ));
        // What the title bar hides from its right click, shown again (#205).
        let hidden = self.hidden_bar_names(cx);
        let hidden_row = (!hidden.is_empty()).then(|| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().text_color(theme::muted()).child(hidden.join(" · ")))
                .child(
                    super::discussion::button(
                        "bar-show-all",
                        "Show all",
                        None,
                        super::discussion::Kind::Plain,
                    )
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.change_settings(|this| this.settings.hidden_bar.clear(), cx)
                    })),
                )
        });
        let folder = super::discussion::button(
            "settings-folder",
            "Open settings folder",
            None,
            super::discussion::Kind::Plain,
        )
        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
            if let Some(dir) = artoo_core::platform::data_dir() {
                cx.reveal_path(&dir);
            }
        }));
        let scrolled = switch("auto-reviewed", s.auto_reviewed).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.change_settings(
                    |this| this.settings.auto_reviewed = !this.settings.auto_reviewed,
                    cx,
                );
            },
        ));
        let fetch = switch("fetch", self.fetch.enabled).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                let on = !this.fetch.enabled;
                this.set_fetch_enabled(on, cx)
            },
        ));
        let updates = switch("check-updates", s.check_updates).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.change_settings(
                    |this| this.settings.check_updates = !this.settings.check_updates,
                    cx,
                );
            },
        ));
        let preview = div()
            .my(px(8.))
            .rounded(px(ISLAND_RADIUS))
            .overflow_hidden()
            .bg(theme::editor())
            .border_1()
            .border_color(theme::island_border())
            .children(self.review_api().map(|r| r.mark_preview(s.mark_style)))
            .into_any_element();
        let [ui_face, code_face, ui_size] = self.font_rows(cx);
        // A repository's settings under its name (#203).
        let repo: gpui::SharedString = (self.root.as_ref())
            .and_then(|r| r.file_name())
            .map_or("This repository".into(), |n| {
                n.to_string_lossy().into_owned().into()
            });
        // What each row does with keys on the keyboard's row (#203): a switch flips on space, a choice steps on ← →.
        row_keys(
            cx,
            CHANGED_WORDS,
            None,
            Some(|this, right, _, cx| {
                let style = stepped(&MarkStyle::ALL, this.settings.mark_style, right);
                this.change_settings(|this| this.settings.mark_style = style, cx);
            }),
        );
        row_keys(
            cx,
            DEFAULT_MODE,
            None,
            Some(|this, right, _, cx| {
                let mode = stepped(&DiffMode::ALL, this.settings.default_mode, right);
                this.change_settings(|this| this.settings.default_mode = mode, cx);
                this.set_mode(mode, cx);
            }),
        );
        row_keys(
            cx,
            MARK_REVIEWED,
            Some(|this, _, cx| {
                this.change_settings(
                    |this| this.settings.auto_reviewed = !this.settings.auto_reviewed,
                    cx,
                )
            }),
            None,
        );
        row_keys(
            cx,
            CODE_FONT,
            None,
            Some(|this, right, _, cx| {
                let points = super::fonts::code_points(this.settings.zoom);
                let points = if right {
                    points + 1
                } else {
                    points.saturating_sub(1)
                };
                this.set_zoom(super::fonts::zoom_for(points), cx);
            }),
        );
        row_keys(
            cx,
            INTERFACE_SIZE,
            None,
            Some(|this, right, window, cx| this.ui_zoom_by(if right { 1 } else { -1 }, window, cx)),
        );
        row_keys(
            cx,
            PANES,
            None,
            Some(|this, right, _, cx| this.change_settings(|this| this.settings.flat = right, cx)),
        );
        row_keys(
            cx,
            ANIMATIONS,
            Some(|this, _, cx| this.set_animations(!this.settings.animations, cx)),
            None,
        );
        row_keys(
            cx,
            HIDDEN_BAR,
            Some(|this, _, cx| this.change_settings(|this| this.settings.hidden_bar.clear(), cx)),
            None,
        );
        row_keys(
            cx,
            OPEN_FILES,
            None,
            Some(|this, right, _, cx| {
                let installed: Vec<Editor> = (artoo_shell_api::editors(cx).iter())
                    .copied()
                    .filter(|e| e.installed())
                    .collect();
                let Some(now) = this.editor(cx) else {
                    return;
                };
                let next = stepped(&installed, now, right);
                this.change_settings(|this| this.settings.editor = Some(next.key.to_owned()), cx);
            }),
        );
        row_keys(
            cx,
            OPEN_REPO,
            None,
            Some(|this, right, _, cx| {
                let o = stepped(&crate::storage::OpenIn::ALL, this.settings.open_in, right);
                this.change_settings(|this| this.settings.open_in = o, cx);
            }),
        );
        row_keys(
            cx,
            SHOW_PROJECTS,
            None,
            Some(|this, right, _, cx| {
                let p = stepped(&crate::storage::Projects::ALL, super::projects(), right);
                this.change_settings(|this| this.settings.projects = p, cx);
            }),
        );
        row_keys(
            cx,
            REFRESH,
            Some(|this, _, cx| {
                this.change_settings(
                    |this| this.settings.auto_refresh = !this.settings.auto_refresh,
                    cx,
                )
            }),
            None,
        );
        row_keys(
            cx,
            FETCH,
            Some(|this, _, cx| {
                let on = !this.fetch.enabled;
                this.set_fetch_enabled(on, cx)
            }),
            None,
        );
        row_keys(
            cx,
            SETTINGS_FILES,
            Some(|_, _, cx| {
                if let Some(dir) = artoo_core::platform::data_dir() {
                    cx.reveal_path(&dir);
                }
            }),
            None,
        );
        row_keys(
            cx,
            SYSTEM_NOTES,
            Some(|this, _, cx| {
                this.set_system_notifications(!this.settings.system_notifications, cx)
            }),
            None,
        );
        row_keys(cx, DOCK, Some(|this, _, cx| this.flip_dock_count(cx)), None);
        row_keys(
            cx,
            HELPERS,
            Some(|this, _, cx| {
                this.change_settings(
                    |this| this.settings.key_helpers = !this.settings.key_helpers,
                    cx,
                )
            }),
            None,
        );
        row_keys(
            cx,
            UPDATES,
            Some(|this, _, cx| {
                this.change_settings(
                    |this| this.settings.check_updates = !this.settings.check_updates,
                    cx,
                )
            }),
            None,
        );
        vec![
            Own::row(
                CHANGED_WORDS,
                10,
                marks,
                (s.mark_style != d.mark_style).then(|| {
                    reset(cx, |this, _, cx| {
                        this.change_settings(
                            |this| this.settings.mark_style = MarkStyle::default(),
                            cx,
                        )
                    })
                }),
            ),
            Own::part(REVIEW, 11, preview),
            Own::row(
                DEFAULT_MODE,
                20,
                modes,
                (s.default_mode != d.default_mode).then(|| {
                    reset(cx, |this, _, cx| {
                        let mode = DiffMode::default();
                        this.change_settings(|this| this.settings.default_mode = mode, cx);
                        this.set_mode(mode, cx);
                    })
                }),
            ),
            Own::row(
                MARK_REVIEWED,
                40,
                scrolled,
                (s.auto_reviewed != d.auto_reviewed).then(|| {
                    reset(cx, |this, _, cx| {
                        this.change_settings(|this| this.settings.auto_reviewed = false, cx)
                    })
                }),
            ),
            Own::part(APPEARANCE, 40, subhead("Fonts and size")),
            Own::named(INTERFACE_FONT, 41, ui_face),
            Own::named(CODE_FONT, 42, code_face),
            Own::named(INTERFACE_SIZE, 43, ui_size),
            Own::part(APPEARANCE, 59, subhead("Window")),
            Own::row(
                PANES,
                60,
                panes,
                (s.flat != d.flat).then(|| {
                    reset(cx, |this, _, cx| {
                        this.change_settings(|this| this.settings.flat = false, cx)
                    })
                }),
            ),
            Own::row(
                ANIMATIONS,
                70,
                animations,
                (s.animations != d.animations)
                    .then(|| reset(cx, |this, _, cx| this.set_animations(true, cx))),
            ),
        ]
        .into_iter()
        .chain(hidden_row.map(|row| Own::row(HIDDEN_BAR, 75, row, None)))
        .chain(self.dock_row(cx).map(|row| Own::named(DOCK, 20, row)))
        .chain(vec![
            Own::row(
                OPEN_FILES,
                10,
                editors,
                s.editor.is_some().then(|| {
                    reset(cx, |this, _, cx| {
                        this.change_settings(|this| this.settings.editor = None, cx)
                    })
                }),
            ),
            Own::row(
                OPEN_REPO,
                20,
                open_in,
                (s.open_in != d.open_in).then(|| {
                    reset(cx, |this, _, cx| {
                        this.change_settings(|this| this.settings.open_in = Default::default(), cx)
                    })
                }),
            ),
            Own::row(
                SHOW_PROJECTS,
                30,
                projects,
                (s.projects != d.projects).then(|| {
                    reset(cx, |this, _, cx| {
                        this.change_settings(|this| this.settings.projects = Default::default(), cx)
                    })
                }),
            ),
            Own::part(PROJECTS, 40, subhead(repo)),
            Own::row(
                REFRESH,
                41,
                refresh,
                (s.auto_refresh != d.auto_refresh).then(|| {
                    reset(cx, |this, _, cx| {
                        this.change_settings(|this| this.settings.auto_refresh = true, cx)
                    })
                }),
            ),
            Own::row(
                FETCH,
                42,
                fetch,
                (!self.fetch.enabled)
                    .then(|| reset(cx, |this, _, cx| this.set_fetch_enabled(true, cx))),
            ),
            Own::row(SETTINGS_FILES, 20, folder, None),
            // The rows of the sections drawn by hand, as search shows them (#203).
            Own::named(SYSTEM_NOTES, 10, self.notice_system_row(cx)),
            Own::named(HELPERS, 10, self.key_helpers_row(cx).into_any_element()),
            Own::row(
                UPDATES,
                5,
                updates,
                (s.check_updates != d.check_updates).then(|| {
                    reset(cx, |this, _, cx| {
                        this.change_settings(|this| this.settings.check_updates = true, cx)
                    })
                }),
            ),
        ])
        .collect()
    }

    fn set_animations(&mut self, on: bool, cx: &mut Context<Self>) {
        self.change_settings(|this| this.settings.animations = on, cx);
        super::motion::set(on, cx);
    }

    /// The search's hits (#203): the window root's rows live, another owner's leading to their section; with how
    /// many each section has.
    fn settings_hits(
        &self,
        query: &str,
        cx: &mut Context<Self>,
    ) -> (gpui::AnyElement, Vec<(&'static str, usize)>) {
        let sections = artoo_shell_api::settings_sections(cx);
        let title = |id: &str| sections.iter().find(|s| s.id == id).map_or("", |s| s.title);
        let mut own = self.own_rows(cx);
        // Found in the order their section draws them.
        own.sort_by_key(|o| o.order);
        let mut hits: Vec<(&'static str, gpui::AnyElement)> = Vec::new();
        let own_named: Vec<Named> = own.iter().filter_map(|o| o.named).collect();
        for o in own {
            if o.named.is_some_and(|n| n.found(query, title(n.section))) {
                hits.push((o.section, o.view));
            }
        }
        // A feature's setting found is its own row, live, when it offers one (#203).
        let hit_rows: Vec<(&'static str, gpui::AnyView)> = (sections.iter())
            .flat_map(|s| self.settings_rows(s.id))
            .flat_map(|r| r.hits)
            .collect();
        for n in artoo_shell_api::settings_named(cx) {
            if own_named.contains(&n) || !n.found(query, title(n.section)) {
                continue;
            }
            if let Some((_, view)) = hit_rows.iter().find(|(label, _)| *label == n.label) {
                hits.push((n.section, view.clone().into_any_element()));
                continue;
            }
            let (section, label) = (n.section, n.label);
            let line = if n.detail.is_empty() {
                format!("In {}", title(section))
            } else {
                format!("{} · in {}", n.detail, title(section))
            };
            let show = super::discussion::button(
                gpui::SharedString::from(format!("settings-show-{}-{}", section, n.label)),
                "Show",
                None,
                super::discussion::Kind::Plain,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.settings_query.clear();
                this.open_settings_section(section, cx);
                this.settings_lit = Some(label.into());
                cx.notify();
            }));
            hits.push((
                section,
                artoo_ui_kit::controls::setting_row(
                    format!("door-{section}-{}", n.label),
                    n.label,
                    line,
                    show,
                    None,
                )
                .into_any_element(),
            ));
        }
        let counts: Vec<(&'static str, usize)> = (sections.iter())
            .map(|s| (s.id, hits.iter().filter(|(id, _)| *id == s.id).count()))
            .collect();
        let total = hits.len();
        let mut column = div().flex().flex_col();
        if total == 0 {
            column = column
                .child(title_line(format!("No setting matches “{query}”")))
                .child(
                    div().pt(px(8.)).child(
                        super::discussion::button(
                            "settings-clear",
                            "Clear the search",
                            None,
                            super::discussion::Kind::Plain,
                        )
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                this.settings_query.clear();
                                cx.notify();
                            },
                        )),
                    ),
                );
        } else {
            column = column.child(
                div()
                    .flex()
                    .items_baseline()
                    .gap_2()
                    .child(title_line("Search"))
                    .child(div().text_size(px(12.)).text_color(theme::muted()).child(
                        if total == 1 {
                            "1 setting".to_owned()
                        } else {
                            format!("{total} settings")
                        },
                    )),
            );
            for s in &sections {
                let mine: Vec<gpui::AnyElement> = (hits.iter_mut())
                    .filter(|(id, _)| *id == s.id)
                    .map(|(_, v)| std::mem::replace(v, div().into_any_element()))
                    .collect();
                if !mine.is_empty() {
                    column = column.child(subhead(s.title)).children(mine);
                }
            }
        }
        (column.into_any_element(), counts)
    }

    pub(super) fn render_settings(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let section = self.settings_section;
        let query = self.settings_query.trim().to_owned();
        // Every row of Settings is drawn with what was typed marked, the row a door led to lit and the keyboard's
        // ringed (#203).
        let sections = artoo_shell_api::settings_sections(cx);
        let title = |id: &str| sections.iter().find(|s| s.id == id).map_or("", |s| s.title);
        let words: HashMap<gpui::SharedString, (&'static str, &'static str)> =
            (artoo_shell_api::settings_named(cx))
                .into_iter()
                .map(|n| (n.label.into(), (n.words, title(n.section))))
                .collect();
        artoo_ui_kit::controls::settings_look(SettingsLook {
            query: query.split_whitespace().map(str::to_owned).collect(),
            words: Rc::new(words),
            lit: self.settings_lit.clone(),
            focus: self.settings_focus.clone(),
        });
        let search = div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                self.field_box(
                    "settings-search",
                    super::lists::Field::Settings,
                    "Search settings",
                    cx,
                )
                .flex_1()
                // Where it is from anywhere in Settings (#203).
                .child(artoo_ui_kit::keys::caps(
                    "⌘F",
                    artoo_ui_kit::keys::On::Plain,
                )),
            )
            .into_any_element();
        if !query.is_empty() {
            let (content, counts) = self.settings_hits(&query, cx);
            return settings::page(
                settings::Page {
                    sections: artoo_shell_api::settings_sections(cx),
                    on: section,
                    content,
                    editing: false,
                    search,
                    found: Some(counts),
                },
                Rc::new(Glue(cx.weak_entity())),
                cx,
            );
        }
        // Rows that fill the section take the page's height: the theme editor's tokens scroll beside a preview that
        // stays.
        let mut editing = false;
        let column = || div().flex().flex_col();
        let content: gpui::AnyElement = match section {
            KEYBOARD => self.render_keyboard_settings(cx).into_any_element(),
            super::notice_settings::NOTIFICATIONS => self.render_notice_settings(cx),
            // The window root's rows and the features' in their places (Review, Appearance, Projects, About), or the
            // rows that fill it now (the theme editor).
            other => {
                let rows = self.settings_rows(other);
                match rows
                    .iter()
                    .find(|r| r.fills.as_ref().is_some_and(|now| now(cx)))
                {
                    Some(filling) => {
                        editing = true;
                        filling.view.clone().into_any_element()
                    }
                    None => {
                        let mut placed: Vec<(u16, gpui::AnyElement)> = (self.own_rows(cx))
                            .into_iter()
                            .filter(|o| o.section == other)
                            .map(|o| (o.order, o.view))
                            .collect();
                        placed.extend(
                            rows.into_iter()
                                .map(|r| (r.order, r.view.into_any_element())),
                        );
                        placed.sort_by_key(|(order, _)| *order);
                        column()
                            .children(placed.into_iter().map(|(_, row)| row))
                            .into_any_element()
                    }
                }
            }
        };
        settings::page(
            settings::Page {
                sections: artoo_shell_api::settings_sections(cx),
                on: section,
                content,
                editing,
                search,
                found: None,
            },
            Rc::new(Glue(cx.weak_entity())),
            cx,
        )
    }
}

/// A row of the window root's in Settings (#203): its section, its place among the section's rows (Appearance mixes
/// them with the themes', 10, and your themes', 80), its words when it is a setting.
struct Own {
    section: &'static str,
    order: u16,
    named: Option<Named>,
    view: gpui::AnyElement,
}

impl Own {
    /// A setting's row, with its dot and Reset when off its default.
    fn row(named: Named, order: u16, control: impl IntoElement, reset: Option<Reset>) -> Self {
        Self::named(
            named,
            order,
            artoo_ui_kit::controls::setting_row(
                named.label,
                named.label,
                named.detail,
                control,
                reset,
            )
            .into_any_element(),
        )
    }

    /// A setting's row drawn already (the faces' and sizes').
    fn named(named: Named, order: u16, view: gpui::AnyElement) -> Self {
        Self {
            section: named.section,
            order,
            named: Some(named),
            view,
        }
    }

    /// What is not a setting: a heading, the changed words' preview.
    fn part(section: &'static str, order: u16, view: impl IntoElement) -> Self {
        Self {
            section,
            order,
            named: None,
            view: view.into_any_element(),
        }
    }
}

/// What `named`'s row does with keys on the keyboard's row (#203): `space` on space or ↵, `step` on ← → (true: →).
type Space = fn(&mut Workspace, &mut Window, &mut Context<Workspace>);
type Step = fn(&mut Workspace, bool, &mut Window, &mut Context<Workspace>);

fn row_keys(cx: &Context<Workspace>, named: Named, space: Option<Space>, step: Option<Step>) {
    let tab = cx.weak_entity();
    artoo_ui_kit::controls::setting_keys(
        named.label,
        Rc::new(move |key, window, cx| {
            tab.update(cx, |this, cx| match key {
                SettingKey::Space => space.map(|f| f(this, window, cx)).is_some(),
                SettingKey::Left | SettingKey::Right => (step)
                    .map(|f| f(this, key == SettingKey::Right, window, cx))
                    .is_some(),
            })
            .unwrap_or(false)
        }),
    );
}

/// The choice next to `now` among `all`, toward its end when `forward`; the ends stay.
fn stepped<T: Copy + PartialEq>(all: &[T], now: T, forward: bool) -> T {
    let at = all.iter().position(|t| *t == now).unwrap_or(0);
    let next = if forward {
        (at + 1).min(all.len().saturating_sub(1))
    } else {
        at.saturating_sub(1)
    };
    all.get(next).copied().unwrap_or(now)
}

/// A row's Reset, done by the tab.
fn reset(
    cx: &mut Context<Workspace>,
    act: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
) -> Reset {
    Box::new(cx.listener(move |this, _: &ClickEvent, window, cx| act(this, window, cx)))
}

/// A part's heading over its rows: Fonts and size, Window, a repository's name.
fn subhead(title: impl Into<gpui::SharedString>) -> impl IntoElement {
    div()
        .pt(px(18.))
        .pb(px(4.))
        .text_size(px(12.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(theme::muted())
        .child(title.into())
}

fn title_line(text: impl Into<gpui::SharedString>) -> impl IntoElement {
    div()
        .pb(px(6.))
        .text_size(px(15.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(text.into())
}

/// What a click on Settings' frame does, answered by the tab.
struct Glue(WeakEntity<Workspace>);

impl settings::SettingsHost for Glue {
    fn section(&self, section: &'static str, _: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |this, cx| {
                this.open_settings_section(section, cx);
                cx.notify();
            })
            .ok();
    }

    fn done(&self, _: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |this, cx| {
                this.settings_open = false;
                cx.notify();
            })
            .ok();
    }
}

use artoo_shell_api::settings::{self, Named, Section};
use artoo_ui_kit::controls::Reset;

/// Appearance's section (`artoo-appearance`'s, order 20): the window root's rows go into it.
pub(super) const APPEARANCE: &str = "appearance";

/// The window root's sections of Settings, by id (#203: what each holds is in its name): the tab draws them
/// (`render_settings`). A feature's section holds its parts' rows: Accounts the hosts' (50), Agents (60), Plugins
/// (80).
pub(super) const REVIEW: &str = "review";
pub(super) const PROJECTS: &str = "projects";
pub(super) const ABOUT: &str = artoo_shell_api::settings::ABOUT;
pub(super) const REQUESTS: &str = artoo_hosts_api::connect::SETTINGS;
pub(super) const KEYBOARD: &str = "keyboard";

const CHANGED_WORDS: Named = Named::new(REVIEW, "Changed words")
    .detail("In Words and Structural modes.")
    .words("diff highlight mark tinted underlined");
const DEFAULT_MODE: Named =
    Named::new(REVIEW, "Default mode").words("diff view lines words structural");
const MARK_REVIEWED: Named = Named::new(REVIEW, "Mark reviewed as I scroll past")
    .detail("With all files in one scroll.")
    .words("diff viewed");
const INTERFACE_FONT: Named =
    Named::new(APPEARANCE, "Interface").words("interface font face typeface");
const CODE_FONT: Named = Named::new(APPEARANCE, "Code")
    .detail("Grows with the interface size.")
    .words("code font face typeface monospace size points");
const INTERFACE_SIZE: Named = Named::new(APPEARANCE, "Interface size")
    .detail("⌘+ and ⌘− change it from anywhere; ⌘0 back to 100 %")
    .words("zoom scale bigger smaller");
const PANES: Named = Named::new(APPEARANCE, "Panes").words("islands flat borders gaps layout");
const ANIMATIONS: Named = Named::new(APPEARANCE, "Animations").words("motion reduce");
const OPEN_FILES: Named = Named::new(PROJECTS, "Open files in").words("general editor program");
const OPEN_REPO: Named =
    Named::new(PROJECTS, "Open a repository in").words("general window tab ask");
const SHOW_PROJECTS: Named =
    Named::new(PROJECTS, "Show projects as").words("several rail tabs menu layout");
const REFRESH: Named =
    Named::new(PROJECTS, "Refresh when the repository changes").words("repository watch files");
const FETCH: Named = Named::new(PROJECTS, "Fetch in the background")
    .detail("Every 5 minutes and when the window comes back")
    .words("repository remote origin");
const SETTINGS_FILES: Named =
    Named::new(ABOUT, "Settings files").words("export import backup folder txt");
const HIDDEN_BAR: Named =
    Named::new(APPEARANCE, "Hidden from the title bar").words("title bar buttons hide show");
const UPDATES: Named = Named::new(ABOUT, "Check for updates")
    .detail("At start and every 6 hours.")
    .words("general version");

/// The rows of the sections the window root draws by hand (#203): found live, as its other rows are.
const SYSTEM_NOTES: Named = Named::new(
    super::notice_settings::NOTIFICATIONS,
    super::notice_settings::SYSTEM_NOTIFICATIONS,
)
.words("general toast alert mac background");
const DOCK: Named = Named::new(
    super::notice_settings::NOTIFICATIONS,
    super::notice_settings::DOCK_COUNT,
)
.words("badge");
const HELPERS: Named = Named::new(KEYBOARD, "Keyboard helpers").words("keys caps hints shortcuts");

/// What those sections hold that is a list, not a row: found by their words and opened (#203).
const DRAWN: [Named; 5] = [
    SYSTEM_NOTES,
    DOCK,
    Named::new(
        super::notice_settings::NOTIFICATIONS,
        "Notices by their source",
    )
    .words("inbox toast system kinds"),
    HELPERS,
    Named::new(KEYBOARD, "Keys of every action").words("shortcuts keymap bindings"),
];

/// Offers the window root's sections, in their places in the left column, and its settings by their words.
pub(super) fn install(cx: &mut App) {
    for (id, title, order) in [
        (REVIEW, "Review", 10),
        (KEYBOARD, "Keyboard", 30),
        (super::notice_settings::NOTIFICATIONS, "Notifications", 35),
        (PROJECTS, "Projects", 40),
        (ABOUT, "About", 90),
    ] {
        artoo_shell_api::add_settings_section(cx, Section { id, title, order });
    }
    for named in [
        CHANGED_WORDS,
        DEFAULT_MODE,
        MARK_REVIEWED,
        INTERFACE_FONT,
        CODE_FONT,
        INTERFACE_SIZE,
        PANES,
        ANIMATIONS,
        OPEN_FILES,
        OPEN_REPO,
        SHOW_PROJECTS,
        REFRESH,
        FETCH,
        UPDATES,
        SETTINGS_FILES,
        HIDDEN_BAR,
    ]
    .into_iter()
    .chain(DRAWN)
    {
        artoo_shell_api::add_setting(cx, named);
    }
}
use artoo_ui_kit::controls::switch;

fn style_id(style: MarkStyle) -> &'static str {
    match style {
        MarkStyle::Tinted => "mark-tinted",
        MarkStyle::Underlined => "mark-underlined",
    }
}

fn mode_id(mode: DiffMode) -> &'static str {
    match mode {
        DiffMode::Lines => "default-lines",
        DiffMode::Words => "default-words",
        DiffMode::Structural => "default-structural",
    }
}

/// A labelled row (the kit's).
pub(super) use artoo_ui_kit::controls::setting as item;

impl Workspace {
    /// Settings → Keyboard: every action with its keys, filtered by name or key. Change waits for
    /// the next key, Reset takes the shipped ones back; a changed action has a dot, and a key two
    /// actions share is outlined in red, the other one named in its tooltip.
    /// Islands | Flat (Settings → Appearance, and Set up's Look).
    pub(super) fn panes_chips(&self, cx: &mut Context<Self>) -> gpui::Div {
        group()
            .child(
                chip("panes-islands", "Islands", !super::flat()).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.change_settings(|this| this.settings.flat = false, cx)
                    },
                )),
            )
            .child(
                chip("panes-flat", "Flat", super::flat()).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.change_settings(|this| this.settings.flat = true, cx)
                    },
                )),
            )
    }

    /// The keymaps to start from, the one in use lit (Settings → Keyboard, and Set up's Keys).
    pub(super) fn key_presets(&self, cx: &mut Context<Self>) -> gpui::Div {
        use super::keymap::{self, Keymap};
        let map = cx.global::<Keymap>();
        let (preset, changed_count) = (map.preset(), map.changed_count());
        div().flex().flex_wrap().gap(px(6.)).pb(px(12.)).children(
            keymap::Preset::ALL.into_iter().enumerate().map(|(i, p)| {
                let on = p == preset;
                div()
                    .id(("key-preset", i))
                    .px(px(12.))
                    .h(px(26.))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .border_1()
                    .border_color(if on {
                        theme::accent()
                    } else {
                        theme::island_border()
                    })
                    .when(on, |s| s.bg(theme::info_bg()))
                    .text_size(px(12.))
                    .text_color(if on { theme::text() } else { theme::muted() })
                    .cursor_pointer()
                    .child(if on && changed_count > 0 {
                        format!("{} · {changed_count} changed", p.label())
                    } else {
                        p.label().to_owned()
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        keymap::set_preset(p, cx);
                        this.refresh_menus(cx);
                        cx.notify();
                    }))
            }),
        )
    }

    /// Settings → Keyboard's rows as listed: the filter's and Changed only's, under their headings (Move · Comment ·
    /// Find · View · Requests · App), by name within each; the names they are said by.
    fn keyboard_rows(
        &self,
        cx: &App,
    ) -> (
        Vec<super::keymap::Entry>,
        std::collections::HashMap<&'static str, KeyName>,
    ) {
        let query = self.keys_filter.trim().to_lowercase();
        // The menu's and ⌘K's names, not code words (#193); `Activate` is not one to change.
        let named = key_names(cx);
        let mut entries: Vec<_> = (cx.global::<super::keymap::Keymap>().entries().into_iter())
            .filter(|e| e.name != "Activate")
            .map(|mut e| {
                if let Some(n) = named.get(e.name) {
                    e.label = n.said.clone();
                }
                if let Some((key, other)) = e.clash.take() {
                    let other = (named.values())
                        .find(|n| n.was == other)
                        .map_or(other, |n| n.said.clone());
                    e.clash = Some((key, other));
                }
                e
            })
            .filter(|e| matches_keys(e, &query, self.keys_exact))
            .filter(|e| !self.keys_changed_only || e.changed)
            .collect();
        entries.sort_by(|a, b| {
            (heading(a.name, &named), &a.label).cmp(&(heading(b.name, &named), &b.label))
        });
        (entries, named)
    }

    /// ↑ ↓ in Settings → Keyboard's filter (#193): the next row, or the one before, the keyboard's.
    pub(super) fn keys_step(&mut self, down: bool, cx: &mut Context<Self>) {
        let names: Vec<&'static str> = self.keyboard_rows(cx).0.iter().map(|e| e.name).collect();
        let at = (self.keys_sel).and_then(|s| names.iter().position(|n| *n == s));
        let next = match at {
            None => 0,
            Some(i) if down => (i + 1).min(names.len().saturating_sub(1)),
            Some(i) => i.saturating_sub(1),
        };
        self.keys_sel = names.get(next).copied();
        cx.notify();
    }

    /// ↵ in the filter: the row the keyboard is on waits for its new key.
    pub(super) fn keys_change_picked(&mut self, cx: &mut Context<Self>) {
        let rows = self.keyboard_rows(cx).0;
        let sel = (self.keys_sel)
            .filter(|s| rows.iter().any(|e| e.name == *s))
            .or(rows.first().map(|e| e.name));
        if let Some(name) = sel {
            self.keys_sel = Some(name);
            self.recording = Some(name);
            cx.notify();
        }
    }

    fn render_keyboard_settings(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        use super::discussion::{Kind, button};
        use super::keymap::{self, Keymap};
        use super::keys::{self, On};
        let changed_only = self.keys_changed_only;
        let map = cx.global::<Keymap>();
        let (preset, changed_count) = (map.preset(), map.changed_count());
        let (entries, named) = self.keyboard_rows(cx);
        // The row the keyboard is on (#193): the one picked while it is listed, else the first.
        let sel = (self.keys_sel)
            .filter(|s| entries.iter().any(|e| e.name == *s))
            .or(entries.first().map(|e| e.name));
        let presets = self.key_presets(cx);
        let helpers = self.key_helpers_row(cx);
        let by_keys = self.recording == Some(BY_KEYS);
        let tools = div()
            .flex()
            .gap(px(6.))
            .pb(px(8.))
            // The next keys pressed filter the list (Design D, X55).
            .child(
                button(
                    "keys-by-keys",
                    if by_keys {
                        "Press the keys…"
                    } else {
                        "By keys"
                    },
                    None,
                    if by_keys {
                        Kind::Primary(theme::accent())
                    } else {
                        Kind::Plain
                    },
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.recording = if by_keys { None } else { Some(BY_KEYS) };
                    cx.notify();
                })),
            )
            .child(
                button(
                    "keys-changed-only",
                    "Changed only",
                    None,
                    if changed_only {
                        Kind::Primary(theme::accent())
                    } else {
                        Kind::Plain
                    },
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.keys_changed_only = !this.keys_changed_only;
                    cx.notify();
                })),
            )
            .child(div().flex_1())
            .child(
                button("keys-export", "Copy keymap", None, Kind::Plain).on_click(cx.listener(
                    |_, _: &ClickEvent, _, cx| {
                        let text = cx.global::<Keymap>().export();
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                    },
                )),
            )
            .children((changed_count > 0).then(|| {
                button(
                    "keys-reset-all",
                    format!("Reset to {}", preset.label()),
                    None,
                    Kind::Plain,
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    keymap::reset(cx);
                    this.refresh_menus(cx);
                    cx.notify();
                }))
            }));
        let count = entries.len();
        let mut last_heading = None;
        let rows = entries.into_iter().enumerate().map(|(ix, e)| {
            let name = e.name;
            let under = heading(name, &named);
            let head = (last_heading != Some(under)).then(|| {
                last_heading = Some(under);
                div()
                    .pt(px(14.))
                    .pb(px(4.))
                    .text_size(px(12.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme::muted())
                    .child(under.words())
            });
            let was = e
                .was
                .iter()
                .map(|k| {
                    // A letter key as the caps write it: ⇧⌘O.
                    let mut s = keymap::symbols(k);
                    let last = s.chars().last().filter(char::is_ascii_lowercase);
                    let alone = s.chars().rev().nth(1).is_none_or(|c| !c.is_alphanumeric());
                    if let (Some(c), true) = (last, alone) {
                        s.pop();
                        s.push(c.to_ascii_uppercase());
                    }
                    s
                })
                .collect::<Vec<_>>()
                .join(" · ");
            let waiting = self.recording == Some(name);
            let clash = e.clash.clone();
            // A clash names the other command (#193).
            let also = clash.as_ref().map(|(_, other)| {
                div()
                    .flex_none()
                    .text_size(px(11.))
                    .text_color(theme::warning())
                    .child(format!("also {other} — yours wins"))
            });
            let keys = e.keys.iter().map(|k| {
                div()
                    .id(gpui::SharedString::from(format!("key-{name}-{k}")))
                    .p(px(2.))
                    .child(keys::caps(&keymap::symbols(k), On::Plain))
            });
            let group = gpui::SharedString::from(format!("key-row-{name}"));
            let picked = sel == Some(name);
            let row = div()
                .id(group.clone())
                .group(group.clone())
                .flex()
                .items_center()
                .gap_3()
                .min_h(px(36.))
                .px(px(10.))
                .rounded(px(6.))
                .when(waiting, |s| s.border_1().border_color(theme::focus()))
                .when(picked, |s| s.bg(theme::selected()))
                .when(!picked, |s| s.hover(|s| s.bg(theme::hover())))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.keys_sel = Some(name);
                    cx.notify();
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_none()
                                .size(px(6.))
                                .rounded_full()
                                .when(e.changed, |s| s.bg(theme::accent())),
                        )
                        .child(super::hover::cut(
                            gpui::SharedString::from(format!("key-label-{}", e.name)),
                            e.label,
                        )),
                )
                .child(
                    div()
                        .flex_none()
                        .min_w(px(200.))
                        .flex()
                        .justify_end()
                        .items_center()
                        .gap(px(6.))
                        .when(waiting, |s| {
                            s.child(div().text_size(px(12.)).text_color(theme::muted()).child(
                                keys::text("Press a key · esc cancels · ⌫ no key").into_owned(),
                            ))
                        })
                        .when(!waiting, |s| s.children(also))
                        .when(!waiting && e.changed && !was.is_empty(), |s| {
                            s.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                                    .line_through()
                                    .child(keys::text(&was).into_owned()),
                            )
                        })
                        .when(!waiting, |s| s.children(keys))
                        .when(!waiting && e.keys.is_empty(), |s| {
                            s.child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::faint())
                                    .child("no key"),
                            )
                        }),
                )
                .child(
                    div()
                        .flex_none()
                        .w(px(168.))
                        .flex()
                        .justify_end()
                        .gap(px(6.))
                        .children(e.changed.then(|| {
                            button(("key-reset", ix), "Reset", None, Kind::Quiet).on_click(
                                cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    keymap::set(name, None, cx);
                                    this.refresh_menus(cx);
                                    cx.notify();
                                }),
                            )
                        }))
                        // Change on the row pointed at, and on the keyboard's (#193).
                        .child(
                            div()
                                .when(!waiting && !picked, |s| {
                                    s.invisible().group_hover(group.clone(), |s| s.visible())
                                })
                                .child(
                                    button(
                                        ("key-change", ix),
                                        if waiting { "Cancel" } else { "Change" },
                                        None,
                                        Kind::Plain,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, _, cx| {
                                            this.recording =
                                                if waiting { None } else { Some(name) };
                                            cx.notify();
                                        },
                                    )),
                                ),
                        ),
                );
            div().children(head).child(row)
        });
        div()
            .flex()
            .flex_col()
            .child(helpers)
            .child(presets)
            .child(tools)
            .child(div().pb(px(8.)).child(self.render_field(
                "keys-filter",
                super::lists::Field::Keys,
                if crate::platform::MAC {
                    "Filter by name or key — mark, ⌘B, cmd-b"
                } else {
                    "Filter by name or key — mark, Ctrl+B, ctrl-b"
                },
                cx,
            )))
            .children(rows)
            .children((count == 0).then(|| {
                div()
                    .py(px(12.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child("No action by that name or key.")
            }))
    }

    /// Settings → Keyboard's first row (#193): the keyboard helpers, the keys over what is on screen while ⌘ is held.
    fn key_helpers_row(&self, cx: &mut Context<Self>) -> gpui::Div {
        let on = self.settings.key_helpers;
        // Framed as every row of Settings is (#203).
        artoo_ui_kit::controls::setting_frame(&HELPERS.label.into(), div())
            .mb(px(12.))
            .px(px(12.))
            .py(px(8.))
            .rounded(px(8.))
            .bg(theme::panel())
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(artoo_ui_kit::controls::found_marked(HELPERS.label))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::muted())
                            .child(keys_said(
                                "Keys while ⌘ or ⌃⌘ is held · a hint after the mouse",
                            )),
                    )
                    .children(
                        artoo_ui_kit::controls::found_by(HELPERS.label, HELPERS.label).map(
                            |word| {
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                                    .child(artoo_ui_kit::controls::found_marked(word))
                            },
                        ),
                    ),
            )
            .child(switch("key-helpers", on).on_click(cx.listener(
                |this, _: &ClickEvent, _, cx| {
                    this.change_settings(
                        |this| this.settings.key_helpers = !this.settings.key_helpers,
                        cx,
                    )
                },
            )))
    }

    /// The key pressed while Settings → Keyboard waits for one.
    pub(super) fn record_key(&mut self, event: &gpui::KeyDownEvent, cx: &mut Context<Self>) {
        let Some(name) = self.recording.take() else {
            return;
        };
        let key = &event.keystroke;
        let plain = !(key.modifiers.platform
            || key.modifiers.alt
            || key.modifiers.control
            || key.modifiers.shift);
        // By keys: what was pressed becomes the filter, as the app writes keys.
        if name == BY_KEYS {
            if !(plain && key.key == "escape") {
                // A letter key as the caps write it: ⌘B.
                let mut spec = super::keymap::symbols(&key.unparse());
                if spec.chars().last().is_some_and(|c| c.is_ascii_lowercase())
                    && spec
                        .chars()
                        .rev()
                        .nth(1)
                        .is_none_or(|c| !c.is_alphanumeric())
                {
                    let last = spec.pop().map(|c| c.to_ascii_uppercase());
                    spec.extend(last);
                }
                self.keys_filter = super::keys::text(&spec).into_owned();
                self.keys_exact = true;
            }
            return cx.notify();
        }
        match key.key.as_str() {
            "escape" if plain => {}
            "backspace" if plain => super::keymap::set(name, Some(Vec::new()), cx),
            _ => super::keymap::set(name, Some(vec![key.unparse()]), cx),
        }
        self.refresh_menus(cx);
        cx.notify();
    }

    /// Rebuilds the menu bar once the current event is over: macOS matches a menu key equivalent
    /// against the new menus at once, and the action it fires must not find the workspace still
    /// being updated (a crash when a recorded key was also a new menu shortcut).
    pub(super) fn refresh_menus(&self, cx: &mut Context<Self>) {
        let options = self.view_options(cx);
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(50))
                .await;
            cx.update(|cx| super::commands::set_menus(&options, cx));
        })
        .detach();
    }
}

/// What `Workspace::recording` holds while By keys waits for the keys to filter by.
pub(super) const BY_KEYS: &str = "\u{0}by keys";

/// Whether a keyboard row answers `query` (lower case): every word in its name, or a key written
/// as the app writes it (`⌘B`) or as GPUI does (`cmd-b`).
fn matches_keys(e: &super::keymap::Entry, query: &str, exact: bool) -> bool {
    if query.is_empty() {
        return true;
    }
    let written = |k: &String| super::keys::text(&super::keymap::symbols(k)).to_lowercase();
    if exact {
        return e.keys.iter().any(|k| written(k) == query);
    }
    let label = e.label.to_lowercase();
    query.split_whitespace().all(|w| label.contains(w))
        || e.keys
            .iter()
            .any(|k| k.contains(query) || written(k).contains(query))
}

/// A command as Settings → Keyboard names it: the menu's or ⌘K's words, the name the keymap gives it
/// (`Approve request`, what a clash says), its id (its heading).
struct KeyName {
    said: String,
    was: String,
    heading: artoo_shell_api::Heading,
}

/// Every command's `KeyName`, by its keymap name (`ApproveRequest`).
fn key_names(cx: &App) -> std::collections::HashMap<&'static str, KeyName> {
    artoo_shell_api::actions(cx)
        .iter()
        .map(|a| {
            let name = artoo_shell_api::short(a.act.name());
            let was = super::keymap::label(name);
            let said = match (a.words, &a.menu) {
                (Some(words), _) => words.trim_end_matches('…').to_owned(),
                // The menu's Title Case said as a sentence: `Next Open Item`, Next open item.
                (None, Some(m)) => sentence(m.words.trim_end_matches('…')),
                (None, None) => was.clone(),
            };
            (
                name,
                KeyName {
                    said,
                    was,
                    heading: a.heading,
                },
            )
        })
        .collect()
}

/// Which heading a command goes under: its action's (#193); one not offered as an action, the app's.
fn heading(
    name: &str,
    named: &std::collections::HashMap<&'static str, KeyName>,
) -> artoo_shell_api::Heading {
    named
        .get(name)
        .map_or(artoo_shell_api::Heading::App, |n| n.heading)
}

/// `text` with its keys in this OS's words.
fn keys_said(text: &str) -> String {
    super::keys::text(text).into_owned()
}

/// `Title Case Words` as a sentence: the first word as it is, the others lower case but a name's (`GitLab`, `PR`).
fn sentence(title: &str) -> String {
    title
        .split(' ')
        .enumerate()
        .map(|(i, w)| {
            let named = w.chars().skip(1).any(char::is_uppercase);
            if i == 0 || named {
                w.to_owned()
            } else {
                w.to_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_pressed_matches_only_that_key() {
        let entry = |keys: &[&str]| super::super::keymap::Entry {
            name: "X",
            label: "Switch branch".into(),
            keys: keys.iter().map(|k| (*k).to_owned()).collect(),
            changed: false,
            was: Vec::new(),
            clash: None,
        };
        let pressed = super::super::keys::text("⌘B").to_lowercase();
        assert!(matches_keys(&entry(&["cmd-b"]), &pressed, true));
        assert!(!matches_keys(&entry(&["cmd-shift-b"]), &pressed, true));
        // Typed, it is a part of a key or of the name: ⇧ on a Mac, shift on Windows and Linux.
        let typed = super::super::keys::text("⇧").to_lowercase();
        assert!(matches_keys(&entry(&["cmd-shift-b"]), &typed, false));
        assert!(!matches_keys(&entry(&["cmd-b"]), &typed, false));
        assert!(matches_keys(&entry(&[]), "branch", false));
    }

    #[test]
    fn a_title_is_said_as_a_sentence() {
        assert_eq!(sentence("Next Open Item"), "Next open item");
        assert_eq!(sentence("Open in GitLab"), "Open in GitLab");
    }
}
