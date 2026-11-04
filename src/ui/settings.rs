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
        self.setup = None;
        self.settings_open = true;
        cx.notify();
    }

    /// Settings → Merge requests, its ways looked for (#156): where the connected toast leads.
    pub(super) fn open_requests_settings(&mut self, cx: &mut Context<Self>) {
        self.open_settings(cx);
        self.settings_section = Section::Requests;
        self.look_for_token(cx);
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

    fn change_settings(&mut self, change: impl FnOnce(&mut Self), cx: &mut Context<Self>) {
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
        let update_rows = self.settings_rows(artoo_shell_api::Section::General);
        let system = switch("system-notifications", s.system_notifications).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                let on = !this.settings.system_notifications;
                this.change_settings(|this| this.settings.system_notifications = on, cx);
                // Turned on: the system asks for permission where it has one (macOS), and says
                // where they will come.
                if on {
                    super::system_notes::identify(cx);
                    cx.show_system_notification(gpui::SystemNotification {
                        tag: "system-notifications-on".into(),
                        title: "Notifications are on".into(),
                        body: "Artoo tells you here what comes while it is in the background."
                            .into(),
                        actions: Vec::new(),
                    });
                }
            },
        ));

        // The host's ways in, as a list to pick from (#156), hosts' connect panel.
        let gitlab = self
            .connecting()
            .and_then(|connecting| connecting.settings(self.here(cx), cx));
        let section = self.settings_section;
        // The theme editor takes the page's height: its tokens scroll beside a preview that stays.
        let editing = section == Section::Appearance && self.theme_edit.is_some();
        let column = || div().flex().flex_col().gap_5();
        let content: gpui::AnyElement = match section {
            Section::Diff => column()
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
                .child(item(
                    "Default mode",
                    "",
                    modes,
                ))
                .child(item(
                    "Fold generated files",
                    "Lock files and generated code that change by more than 40 lines.",
                    fold,
                ))
                .child(item(
                    "Mark reviewed as I scroll past",
                    "",
                    scrolled,
                ))
                .child(item(
                    "Format comments as markdown",
                    "⌥M, or Raw / Rich beside a comment, switches it.",
                    rich,
                ))
                .into_any_element(),
            Section::Appearance => self.render_appearance(
                item(
                    "Several projects",
                    "",
                    projects,
                )
                .into_any_element(),
                item(
                    "Panes",
                    "",
                    panes,
                )
                .into_any_element(),
                item(
                    "Animations",
                    "",
                    animations,
                )
                .into_any_element(),
                cx,
            ),
            Section::General => column()
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
                .child(item(
                    "Open a repository in",
                    "",
                    open_in,
                ))
                .child(item(
                    "System notifications",
                    "While Artoo is in the background: the Inbox's news, and an agent's.",
                    system,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(item(
                            "Check for updates",
                            "At start and every 6 hours.",
                            updates,
                        ))
                        .children(update_rows.into_iter().map(|row| div().pt(px(6.)).child(row))),
                )
                .into_any_element(),
            Section::Repository => column()
                .child(item(
                    "Refresh when the repository changes",
                    "",
                    refresh,
                ))
                .child(item(
                    "Fetch in the background",
                    "Every 5 minutes and when the window comes back · this repository only.",
                    fetch,
                ))
                .into_any_element(),
            Section::Requests => gitlab.unwrap_or_else(|| {
                div()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child("This repository has no GitLab or GitHub remote, so there are no merge requests to set up.")
                    .into_any_element()
            }),
            // The agents' own rows (a part's, Settings → Agents).
            Section::Agents => div()
                .flex()
                .flex_col()
                .children(self.settings_rows(artoo_shell_api::Section::Agents))
                .into_any_element(),
            Section::Keyboard => self.render_keyboard_settings(cx).into_any_element(),
        };
        settings::page(
            settings::Page {
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
    fn section(&self, section: Section, _: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |this, cx| {
                this.settings_section = section;
                if section == Section::Requests {
                    this.look_for_token(cx);
                }
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

use artoo_shell_api::settings;
pub use artoo_shell_api::settings::Section;
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
        let entries: Vec<_> = map
            .entries()
            .into_iter()
            .filter(|e| matches_keys(e, &query, self.keys_exact))
            .filter(|e| !changed_only || e.changed)
            .collect();
        let presets = self.key_presets(cx);
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
                button("keys-export", "Export", None, Kind::Plain).on_click(cx.listener(
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
        let rows = entries.into_iter().enumerate().map(|(ix, e)| {
            let name = e.name;
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
            let keys = e.keys.iter().map(|k| {
                let mine = clash
                    .as_ref()
                    .filter(|(key, _)| key == k)
                    .map(|(_, other)| {
                        gpui::SharedString::from(format!("Also {other} — the one you set wins"))
                    });
                div()
                    .id(gpui::SharedString::from(format!("key-{name}-{k}")))
                    .p(px(2.))
                    .rounded(px(5.))
                    .when(mine.is_some(), |s| {
                        s.border_1().border_color(theme::removed())
                    })
                    .child(keys::caps(&keymap::symbols(k), On::Plain))
                    .when_some(mine, |s, tip| {
                        s.tooltip_show_delay(crate::ui::hover::LONG)
                            .tooltip(move |_, cx| cx.new(|_| super::TipOwned(tip.clone())).into())
                    })
            });
            div()
                .flex()
                .items_center()
                .gap_3()
                .min_h(px(44.))
                .border_b_1()
                .border_color(theme::island_border())
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
                        .w(px(200.))
                        .flex()
                        .justify_end()
                        .items_center()
                        .gap(px(6.))
                        .when(waiting, |s| {
                            s.child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::accent())
                                    .child("Press a key…"),
                            )
                        })
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
                        .child(
                            button(
                                ("key-change", ix),
                                if waiting { "Cancel" } else { "Change" },
                                None,
                                Kind::Plain,
                            )
                            .on_click(cx.listener(
                                move |this, _: &ClickEvent, _, cx| {
                                    this.recording = if waiting { None } else { Some(name) };
                                    cx.notify();
                                },
                            )),
                        ),
                )
        });
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .pb(px(12.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child("Start from a preset and change any key · esc cancels, ⌫ leaves it without one"),
            )
            .child(presets)
            .child(tools)
            .child(
                div()
                    .pb(px(8.))
                    .child(self.render_field(
                        "keys-filter",
                        super::lists::Field::Keys,
                        if crate::platform::MAC {
                            "Filter by name or key — mark, ⌘B, cmd-b"
                        } else {
                            "Filter by name or key — mark, Ctrl+B, ctrl-b"
                        },
                        cx,
                    )),
            )
            .children(rows)
            .children((count == 0).then(|| {
                div()
                    .py(px(12.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child("No action by that name or key.")
            }))
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
        let menus = super::menus(&self.view_options(cx), cx);
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(50))
                .await;
            cx.update(|cx| cx.set_menus(menus));
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
}
