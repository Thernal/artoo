//! The Settings page: how changed words are marked, which mode diffs open in, and whether the
//! window refreshes itself. Designed in `../Design/mockups/settings/a-general.html`. Its frame — the sections, the
//! heading, Done — is drawn by the shell (`artoo_shell_api::settings`).

use super::px;
use super::{ISLAND_RADIUS, Workspace, theme};
use crate::editor::Editor;
use crate::storage::{DiffMode, MarkStyle};
use artoo_ui_kit::controls::{chip, group};
use gpui::{App, ClickEvent, Context, WeakEntity, Window, div, prelude::*};
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

    pub(super) fn render_settings(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let s = self.settings.clone();
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
        let editors = group().children(installed.iter().map(|&e| {
            chip(e.key, e.label, current == Some(e)).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.change_settings(|this| this.settings.editor = Some(e.key.to_owned()), cx)
                },
            ))
        }));
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
            |this, _: &ClickEvent, _, cx| {
                this.change_settings(
                    |this| this.settings.animations = !this.settings.animations,
                    cx,
                );
                super::motion::set(this.settings.animations, cx);
            },
        ));
        let refresh = switch("refresh", s.auto_refresh).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.change_settings(
                    |this| this.settings.auto_refresh = !this.settings.auto_refresh,
                    cx,
                )
            },
        ));

        let fold = switch("fold-generated", s.fold_generated).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.rv().clear_unfolded(cx);
                this.change_settings(
                    |this| this.settings.fold_generated = !this.settings.fold_generated,
                    cx,
                );
                this.refresh_rows(cx);
                this.build_stack(cx);
            },
        ));
        let scrolled = switch("auto-reviewed", s.auto_reviewed).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.change_settings(
                    |this| this.settings.auto_reviewed = !this.settings.auto_reviewed,
                    cx,
                );
            },
        ));

        let rich = switch("rich-comments", s.rich_comments)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_rich_comments(cx)));

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
        let update_rows = self.settings_rows(GENERAL);

        let section = self.settings_section;
        // Rows that fill the section take the page's height: the theme editor's tokens scroll beside a preview that
        // stays.
        let mut editing = false;
        let column = || div().flex().flex_col().gap_5();
        let content: gpui::AnyElement = match section {
            DIFF => column()
                .child(item(
                    "Changed words",
                    "In Words and Structural modes.",
                    marks,
                ))
                .child(
                    div()
                        .rounded(px(ISLAND_RADIUS))
                        .overflow_hidden()
                        .bg(theme::editor())
                        .border_1()
                        .border_color(theme::island_border())
                        .children(self.review_api().map(|r| r.mark_preview(s.mark_style))),
                )
                .child(item("Default mode", "", modes))
                .child(item(
                    "Fold generated files",
                    "Lock files and generated code that change by more than 40 lines.",
                    fold,
                ))
                .child(item("Mark reviewed as I scroll past", "", scrolled))
                .child(item(
                    "Format comments as markdown",
                    "⌥M, or Raw / Rich beside a comment, switches it.",
                    rich,
                ))
                .into_any_element(),
            GENERAL => column()
                .child(item(
                    "Open files in",
                    "",
                    if installed.is_empty() {
                        div()
                            .text_size(px(12.))
                            .text_color(theme::faint())
                            .child("Android Studio, Zed or VS Code not found")
                            .into_any_element()
                    } else {
                        editors.into_any_element()
                    },
                ))
                .child(item("Open a repository in", "", open_in))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(item(
                            "Check for updates",
                            "At start and every 6 hours.",
                            updates,
                        ))
                        .children(
                            update_rows
                                .into_iter()
                                .map(|row| div().pt(px(6.)).child(row.view)),
                        ),
                )
                .into_any_element(),
            REPOSITORY => column()
                .child(item("Refresh when the repository changes", "", refresh))
                .child(item(
                    "Fetch in the background",
                    "Every 5 minutes and when the window comes back · this repository only.",
                    fetch,
                ))
                .into_any_element(),
            KEYBOARD => self.render_keyboard_settings(cx).into_any_element(),
            super::notice_settings::NOTIFICATIONS => self.render_notice_settings(cx),
            // A feature's section: the parts' rows and the window root's in their places (Settings → Agents,
            // Appearance), or the rows that fill it now (the theme editor).
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
                        // Appearance holds the faces and sizes, the projects, the panes and motion between the
                        // themes' cards (10) and your themes (80).
                        let mut placed: Vec<(u16, gpui::AnyElement)> = if other == APPEARANCE {
                            vec![
                                (40, self.render_fonts(cx)),
                                (
                                    50,
                                    item("Several projects", "", projects).into_any_element(),
                                ),
                                (60, item("Panes", "", panes).into_any_element()),
                                (70, item("Animations", "", animations).into_any_element()),
                            ]
                        } else {
                            Vec::new()
                        };
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
            },
            Rc::new(Glue(cx.weak_entity())),
            cx,
        )
    }
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

use artoo_shell_api::settings::{self, Section};

/// Appearance's section (`artoo-appearance`'s, order 20): the window root's rows go into it.
pub(super) const APPEARANCE: &str = "appearance";

/// The window root's sections of Settings, by id: the tab draws them (`render_settings`). A feature's section holds
/// its parts' rows: Merge requests the hosts' (50), the agents' between it and Keyboard (60).
pub(super) const DIFF: &str = "diff";
pub(super) const GENERAL: &str = "general";
pub(super) const REPOSITORY: &str = "repository";
pub(super) const REQUESTS: &str = artoo_hosts_api::connect::SETTINGS;
pub(super) const KEYBOARD: &str = "keyboard";

/// Offers the window root's sections, in their places in the left column (`run`).
pub(super) fn install(cx: &mut App) {
    for (id, title, order) in [
        (DIFF, "Diff", 10),
        (GENERAL, "General", 30),
        (super::notice_settings::NOTIFICATIONS, "Notifications", 35),
        (REPOSITORY, "Repository", 40),
        (KEYBOARD, "Keyboard", 70),
    ] {
        artoo_shell_api::add_settings_section(cx, Section { id, title, order });
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

    fn render_keyboard_settings(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        use super::discussion::{Kind, button};
        use super::keymap::{self, Keymap};
        use super::keys::{self, On};
        let query = self.keys_filter.trim().to_lowercase();
        let changed_only = self.keys_changed_only;
        let map = cx.global::<Keymap>();
        let (preset, changed_count) = (map.preset(), map.changed_count());
        // The menu's and ⌘K's names, not code words (#193); `Activate` is not one to change.
        let named = key_names(cx);
        let mut entries: Vec<_> = map
            .entries()
            .into_iter()
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
            .filter(|e| !changed_only || e.changed)
            .collect();
        // Under headings, Move · Comment · Find · View · Requests · App, by name within each.
        entries.sort_by(|a, b| {
            (heading(a.name, &named), &a.label).cmp(&(heading(b.name, &named), &b.label))
        });
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
                    .child(HEADINGS[under])
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
                .hover(|s| s.bg(theme::hover()))
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
                        // Change on the row pointed at alone (#193).
                        .child(
                            div()
                                .when(!waiting, |s| {
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
        div()
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
                    .child("Keyboard helpers")
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::muted())
                            .child(keys_said(
                                "The keys of what is on screen while ⌘ is held · ⌃⌘ the spaces'",
                            )),
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

/// Settings → Keyboard's headings (#193), in their order.
const HEADINGS: [&str; 6] = ["Move", "Comment", "Find", "View", "Requests", "App"];

/// A command as Settings → Keyboard names it: the menu's or ⌘K's words, the name the keymap gives it
/// (`Approve request`, what a clash says), its id (its heading).
struct Named {
    said: String,
    was: String,
    id: &'static str,
}

/// Every command's `Named`, by its keymap name (`ApproveRequest`).
fn key_names(cx: &App) -> std::collections::HashMap<&'static str, Named> {
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
                Named {
                    said,
                    was,
                    id: a.id,
                },
            )
        })
        .collect()
}

/// Which heading a command goes under (`HEADINGS`); one not offered as an action, the app's.
fn heading(name: &str, named: &std::collections::HashMap<&'static str, Named>) -> usize {
    named.get(name).map_or(5, |n| heading_of(n.id))
}

/// The heading of the command with id `id`: what comments, what moves (a request's next one too), what finds,
/// what shows, a request's acts, the rest the app's.
fn heading_of(id: &str) -> usize {
    const COMMENT: [&str; 9] = [
        "review.comment-line",
        "requests.reply",
        "requests.resolve",
        "requests.edit",
        "requests.react",
        "requests.submit",
        "agents.send",
        "requests.next-draft",
        "requests.previous-draft",
    ];
    let (domain, verb) = id.split_once('.').unwrap_or(("", id));
    // By the id's words (`next-change`: next, change), never a part of one (`rendered` is not `end`).
    let words: Vec<&str> = verb.split('-').collect();
    let has = |these: &[&str]| these.iter().any(|w| words.contains(w));
    if COMMENT.contains(&id) {
        1
    } else if domain == "shell" && has(&["new", "close", "reopen", "pin"]) {
        5
    } else if has(&[
        "find",
        "search",
        "filter",
        "match",
        "recent",
        "bookmarks",
        "structure",
    ]) || id == "shell.switch"
    {
        2
    } else if id == "shell.switch-branch"
        || matches!(
            id,
            "requests.overview" | "requests.commits" | "requests.changes"
        )
        || has(&[
            "next", "previous", "back", "forward", "start", "end", "page", "zone", "focus", "tab",
            "item", "section", "up", "down",
        ])
    {
        0
    } else if domain == "requests" {
        4
    } else if domain == "review"
        || has(&[
            "zoom",
            "fold",
            "side",
            "diff",
            "shortcuts",
            "whole",
            "theme",
        ])
    {
        3
    } else {
        5
    }
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
    fn a_command_goes_under_its_heading_by_its_words() {
        assert_eq!(heading_of("review.next-change"), 0);
        assert_eq!(heading_of("requests.next"), 0);
        assert_eq!(heading_of("requests.resolve"), 1);
        assert_eq!(heading_of("review.find"), 2);
        assert_eq!(heading_of("review.next-match"), 2);
        assert_eq!(heading_of("shell.new-tab"), 5);
        // `rendered` holds `end`, not the word.
        assert_eq!(heading_of("review.rendered"), 3);
        assert_eq!(heading_of("requests.approve"), 4);
        assert_eq!(heading_of("shell.quit"), 5);
        assert_eq!(sentence("Next Open Item"), "Next open item");
        assert_eq!(sentence("Open in GitLab"), "Open in GitLab");
    }
}
