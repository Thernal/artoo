//! The Changes section drawn (A9, A11, #22, #79, #157): its header in the side panel (`changes_section`: how far
//! the review is, Tree · List, the count), how far the review is with Mark all (X59), the path filter, the files'
//! total, and the files as a folder tree or a flat list — a file's box, icon, name, words and +/−, a folder's box
//! and fold, a group's header (#22). What the tab puts on it comes in a `SectionLook` and a `ChangesLook`; a click
//! goes to the host (`ChangesEvent`), which also fades a tree's rows in and says the version a mark was made at.

use crate::changes::{FileItem, Group, folder_of};
use crate::diff_view::{change_color, file_tag};
use crate::review::{ChangesEvent, Host, Review};
use crate::reviewed::State;
use artoo_core::git::{ChangeKind, FileDiff};
use artoo_ui_kit::controls::{self, chip};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::{self, CutInTip, Hover, LONG, Tip, TipOwned};
use artoo_ui_kit::panel::{self, Head, count, open_share};
use artoo_ui_kit::{icons, motion, px, row, theme};
use gpui::{
    AnyElement, App, ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent, div, prelude::*,
};
use std::collections::HashSet;
use std::rc::Rc;

/// What the tab puts on the Changes list.
pub struct ChangesLook {
    /// What measures the list's width for the tab.
    pub measure: AnyElement,
    /// The path filter's field.
    pub filter: AnyElement,
    /// A file's +/− shows: the list has 240 px (#79).
    pub counts: bool,
    /// The file whose comment being written waits without the keyboard: a pencil on its row (#168).
    pub draft: Option<String>,
}

/// What the Changes section needs of the tab besides the review.
pub struct SectionLook {
    /// Folded in the tab's layout.
    pub folded: bool,
    /// Its fold moves now: its rows stay drawn while it folds (#56, 1).
    pub moving: bool,
    /// The keyboard's mark on its title (" · ⌃3").
    pub tag: String,
    /// What goes on its list.
    pub list: ChangesLook,
}

impl Review {
    /// The side panel's Changes section while the diff has files: its header (how far the review is or the one
    /// file, Tree · List, the count) over the list; one file's does not open. With it whether it is open.
    pub fn changes_section(
        &self,
        look: SectionLook,
        host: Rc<dyn Host>,
        cx: &mut Context<Self>,
    ) -> Option<(AnyElement, bool)> {
        let diff = self.diff.as_ref().filter(|d| !d.files.is_empty())?;
        let one = diff.files.len() == 1;
        let folded = one || look.folded;
        let summary = if one {
            diff.files[0].path().to_owned()
        } else {
            match self.review_progress() {
                Some((done, total)) => format!("{done} / {total} reviewed"),
                None => plural(diff.file_count(), "file"),
            }
        };
        let list = self.options.list_files;
        let mut end: Vec<AnyElement> = Vec::new();
        if !folded {
            let (tree, flat) = (host.clone(), host.clone());
            end.push(
                controls::group()
                    // A chip is not the heading: a click on it does not fold the section.
                    .child(chip("files-tree", "Tree", !list).on_click(
                        move |_: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            tree.changes(ChangesEvent::ListFiles(false), cx)
                        },
                    ))
                    .child(chip("files-list", "List", list).on_click(
                        move |_: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            flat.changes(ChangesEvent::ListFiles(true), cx)
                        },
                    ))
                    .into_any_element(),
            );
        }
        end.push(count(diff.file_count().to_string()));
        let header = Head {
            title: format!(
                "Changes{}{}",
                if self.all_reviewed() { " ✓" } else { "" },
                look.tag
            ),
            summary: Some(summary),
            end,
            folded,
            opens: !one,
        };
        let rows = !folded || (!one && look.moving);
        let (press, fold) = (host.clone(), host.clone());
        let section = div()
            .flex()
            .flex_col()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, _, cx| {
                press.changes(ChangesEvent::Pressed, cx)
            })
            .child(panel::head("fold-changes", header, move |cx| {
                fold.changes(ChangesEvent::Section, cx)
            }))
            .when(rows, |s| s.child(self.render_changes(look.list, host, cx)));
        Some((
            open_share(section, "changes-open", !folded).into_any_element(),
            !folded,
        ))
    }

    /// The Changes section's body: how far the review is, the path filter, the files.
    pub fn render_changes(
        &self,
        look: ChangesLook,
        host: Rc<dyn Host>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = self.items().len();
        let (counts, draft) = (look.counts, look.draft);
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(look.measure)
            .children(self.render_review_progress(&host))
            .child(look.filter)
            .children(self.render_changes_total(counts))
            .child(if count == 0 {
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child("No changed file matches.")
                    .into_any_element()
            } else {
                // The files list proper: one uniform list, all rows of one height.
                gpui::uniform_list(
                    "files",
                    count,
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        let items = this.items();
                        range
                            .filter_map(|i| items.get(i))
                            .map(|item| {
                                let path = match item {
                                    FileItem::File { ix, .. } => {
                                        this.file_key(*ix).unwrap_or_default()
                                    }
                                    FileItem::Dir { path, .. } => path.clone(),
                                    FileItem::Group { group, .. } => group.key().to_owned(),
                                };
                                let row = this.render_file_item(
                                    item,
                                    counts,
                                    draft.as_deref(),
                                    &host,
                                    cx,
                                );
                                host.tree_row(&path, row, cx)
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.file_scroll)
                .flex_1()
                .pb(px(6.))
                .into_any_element()
            })
            .into_any_element()
    }

    /// Over the files: how many, and their lines' total in the column of each file's own + and −
    /// (#157); with a path typed, the files shown of all, and theirs.
    fn render_changes_total(&self, counts: bool) -> Option<gpui::Div> {
        let diff = self.diff.as_ref()?;
        let all = diff.files.len();
        if all == 0 {
            return None;
        }
        let shown: Vec<usize> = if self.pfilter.trim().is_empty() {
            (0..all).collect()
        } else {
            self.file_order()
        };
        let (added, removed) = shown.iter().fold((0, 0), |(a, r), &ix| {
            (a + diff.files[ix].added, r + diff.files[ix].removed)
        });
        // A file partly staged is in both groups (#22): it is one file.
        let files = |ixs: &mut dyn Iterator<Item = usize>| {
            ixs.map(|ix| diff.files[ix].path())
                .collect::<HashSet<_>>()
                .len()
        };
        let (shown, all) = (files(&mut shown.into_iter()), files(&mut (0..all)));
        let label = if shown == all {
            plural(all, "file")
        } else {
            format!("{shown} of {}", plural(all, "file"))
        };
        Some(
            div()
                .flex_none()
                .h(px(24.))
                .px(px(14.))
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(11.))
                .text_color(theme::muted())
                .child(hover::cut("changes-total-label", label).flex_1())
                .when(counts, |s| s.child(line_counts(added, removed))),
        )
    }

    fn render_file_item(
        &self,
        item: &FileItem,
        counts: bool,
        draft: Option<&str>,
        host: &Rc<dyn Host>,
        cx: &App,
    ) -> AnyElement {
        let Some(diff) = &self.diff else {
            return div().into_any_element();
        };
        let tree = !self.options.list_files && self.pfilter.trim().is_empty();
        // A file's row: its status and name always, its +/− while the list has 240 px (#79); its
        // tooltip the whole path.
        match item {
            FileItem::File { ix, depth } if tree => {
                let file = &diff.files[*ix];
                let ix = *ix;
                let name = file
                    .path()
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                let done = self.review_state(ix) == Some(State::Done);
                let (menu, pick) = (host.clone(), host.clone());
                div()
                    .w_full()
                    .h(px(28.))
                    .px(px(6.))
                    .py(px(1.))
                    .child(
                        row(("file", ix), self.file == ix)
                            .h(px(26.))
                            .gap_2()
                            .pl(px(8. + *depth as f32 * 14.))
                            .when(done, |s| s.text_color(theme::muted()))
                            .children(self.review_box(ix, host, cx))
                            .child(icons::file_icon(file.path(), 16.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .cut_in_tip()
                                    .text_color(change_color(file, done))
                                    .child(name),
                            )
                            .children(draft_mark(file.path(), draft))
                            .children(generated_tag(file))
                            .children(file_tag(diff.tags.row_word(ix)))
                            .when(counts, |s| s.child(stats(file)))
                            .tip(change_tip(file))
                            .on_mouse_down(
                                MouseButton::Right,
                                move |event: &MouseDownEvent, _, cx| {
                                    menu.changes(
                                        ChangesEvent::Menu {
                                            ix,
                                            at: event.position,
                                        },
                                        cx,
                                    )
                                },
                            )
                            .on_click(move |_, _, cx| pick.changes(ChangesEvent::Pick(ix), cx)),
                    )
                    .into_any_element()
            }
            FileItem::File { ix, .. } => self.render_file_row(*ix, counts, draft, host, cx),
            FileItem::Group {
                group,
                count,
                folded,
            } => self.render_group(*group, *count, *folded, tree, host),
            FileItem::Dir {
                path,
                name,
                depth,
                count,
                collapsed,
            } => {
                let (target, host) = (path.clone(), host.clone());
                div()
                    .w_full()
                    .h(px(28.))
                    .px(px(6.))
                    .py(px(1.))
                    .child(
                        row(
                            ("dir", path.len() * 31 + *depth),
                            self.fcursor.as_deref() == Some(path.as_str()),
                        )
                        .h(px(26.))
                        .gap_2()
                        .pl(px(8. + *depth as f32 * 14.))
                        .text_color(theme::muted())
                        .children(self.folder_box(folder_of(path), &host))
                        .child(
                            icons::icon(if *collapsed {
                                "chevron-right"
                            } else {
                                "chevron"
                            })
                            .size(px(16.))
                            .text_color(theme::muted()),
                        )
                        .child(icons::folder_icon(folder_of(path), 16.))
                        .child(
                            hover::cut(
                                gpui::SharedString::from(format!("dir-name-{path}")),
                                name.clone(),
                            )
                            .flex_1(),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child(count.to_string()),
                        )
                        // A double click folds or opens it once, as one click does.
                        .on_click(move |e: &ClickEvent, _, cx| {
                            if e.click_count() < 2 {
                                host.changes(ChangesEvent::Fold(target.clone()), cx)
                            }
                        }),
                    )
                    .into_any_element()
            }
        }
    }

    /// A group's header (#22), as a section's of the side panel: its fold, its name and how many
    /// files it holds; a click folds or opens it. The flat list's rows are taller, and so is it.
    fn render_group(
        &self,
        group: Group,
        count: usize,
        folded: bool,
        tree: bool,
        host: &Rc<dyn Host>,
    ) -> AnyElement {
        let (key, host) = (group.key(), host.clone());
        div()
            .w_full()
            .h(px(if tree { 28. } else { 42. }))
            .px(px(6.))
            .py(px(1.))
            .flex()
            .items_end()
            .child(
                row(
                    ("group", group as usize),
                    self.fcursor.as_deref() == Some(key),
                )
                .h(px(26.))
                .gap(px(6.))
                .pl(px(2.))
                .pr(px(4.))
                // The chevron turns as it folds (#56).
                .child(motion::toward(
                    icons::icon("chevron")
                        .size(px(14.))
                        .text_color(theme::faint()),
                    ("group-fold", group as usize),
                    !folded,
                    false,
                    motion::FAST,
                    |chevron, v| {
                        chevron.with_transformation(gpui::Transformation::rotate(gpui::radians(
                            -std::f32::consts::FRAC_PI_2 * (1. - v),
                        )))
                    },
                ))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::muted())
                        .child(group.name().to_uppercase()),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(count.to_string()),
                )
                // A double click folds or opens it once, as one click does.
                .on_click(move |e: &ClickEvent, _, cx| {
                    if e.click_count() < 2 {
                        host.changes(ChangesEvent::Fold(key.to_owned()), cx)
                    }
                }),
            )
            .into_any_element()
    }

    /// A file of the flat list: its name over its folder, its +/− when `counts`.
    fn render_file_row(
        &self,
        ix: usize,
        counts: bool,
        draft: Option<&str>,
        host: &Rc<dyn Host>,
        cx: &App,
    ) -> AnyElement {
        let Some(file) = self.diff.as_ref().and_then(|d| d.files.get(ix)) else {
            return div().into_any_element();
        };
        let tag = self.diff.as_ref().and_then(|d| d.tags.row_word(ix));
        let done = self.review_state(ix) == Some(State::Done);
        let path = file.path();
        let (dir, name) = match path.rsplit_once('/') {
            Some((dir, name)) => (Some(dir.to_owned()), name.to_owned()),
            None => (None, path.to_owned()),
        };
        let (menu, pick) = (host.clone(), host.clone());
        let item = row(("file", ix), self.file == ix)
            .h(px(40.))
            .gap_2()
            .when(done, |s| s.text_color(theme::muted()))
            .children(self.review_box(ix, host, cx))
            .child(icons::file_icon(path, 16.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .cut_in_tip()
                            .text_color(change_color(file, done))
                            .child(name),
                    )
                    .children(dir.map(|d| {
                        div()
                            .cut_in_tip()
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .child(d)
                    })),
            )
            .children(draft_mark(path, draft))
            .children(generated_tag(file))
            .children(file_tag(tag))
            .when(counts, |s| s.child(stats(file)))
            .tip(change_tip(file))
            .on_mouse_down(MouseButton::Right, move |event: &MouseDownEvent, _, cx| {
                menu.changes(
                    ChangesEvent::Menu {
                        ix,
                        at: event.position,
                    },
                    cx,
                )
            })
            .on_click(move |_, _, cx| pick.changes(ChangesEvent::Pick(ix), cx));
        div()
            .w_full()
            .h(px(42.))
            .px(px(6.))
            .py(px(1.))
            .child(item)
            .into_any_element()
    }

    /// The box of a folder: empty, part-marked or all marked; a click marks or unmarks the folder.
    fn folder_box(&self, path: &str, host: &Rc<dyn Host>) -> Option<impl IntoElement + use<>> {
        self.review_scope.as_ref()?;
        let ixs = self.files_under(path);
        let done = ixs
            .iter()
            .filter(|&&ix| self.review_state(ix) == Some(State::Done))
            .count();
        let all = done == ixs.len() && done > 0;
        let (border, fill, glyph) = if all {
            (theme::added(), Some(theme::added()), "✓")
        } else if done > 0 {
            (theme::added(), None, "–")
        } else {
            (theme::faint(), None, "")
        };
        let (folder, host) = (path.to_owned(), host.clone());
        Some(
            div()
                .id(gpui::ElementId::Name(format!("folder-box-{path}").into()))
                .flex_none()
                .size(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(3.))
                .border_2()
                .border_color(border)
                .when_some(fill, |s, c| s.bg(c))
                .text_size(px(10.))
                .font_weight(FontWeight::BOLD)
                .text_color(if all { theme::base() } else { theme::added() })
                .cursor_pointer()
                .hover(|s| s.border_color(theme::text()))
                .tooltip_show_delay(LONG)
                .tooltip(move |_, cx| {
                    cx.new(|_| {
                        Tip(if all {
                            "Folder reviewed — click to unmark it"
                        } else {
                            "Mark the whole folder reviewed"
                        })
                    })
                    .into()
                })
                .child(glyph)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_: &ClickEvent, _, cx| {
                    let folder = folder.clone();
                    host.changes(ChangesEvent::FolderReviewed { folder, done: !all }, cx)
                }),
        )
    }

    /// What a file's box says on a long hover: when it was marked, at which version (X59).
    fn review_said(&self, ix: usize, state: State, host: &Rc<dyn Host>, cx: &App) -> String {
        let when = self
            .review_scope
            .as_deref()
            .zip(self.diff.as_ref().and_then(|d| d.files.get(ix)))
            .and_then(|(scope, f)| self.reviewed.when(scope, f.path()));
        let at = when.map(|(time, head)| {
            let version = head
                .and_then(|h| host.version_of(h, cx))
                .map(|number| format!(" at v{number},"))
                .unwrap_or_default();
            format!("{version} {}", format::ago(time))
        });
        match (state, at) {
            (State::Open, _) => "Mark as reviewed  ⇧V".into(),
            (State::Done, Some(at)) => format!("Reviewed{at} — click to unmark  ⇧V"),
            (State::Done, None) => "Reviewed — click to unmark  ⇧V".into(),
            (State::Stale, Some(at)) => {
                format!("Changed since you reviewed it{at} — mark again  ⇧V")
            }
            (State::Stale, None) => "Changed since you marked it — mark again  ⇧V".into(),
        }
    }

    /// The box at the left of a file row; a click marks or unmarks without opening the file.
    fn review_box(
        &self,
        ix: usize,
        host: &Rc<dyn Host>,
        cx: &App,
    ) -> Option<impl IntoElement + use<>> {
        let state = self.review_state(ix)?;
        let said: gpui::SharedString = self.review_said(ix, state, host, cx).into();
        let (border, fill, glyph) = match state {
            State::Open => (theme::faint(), None, ""),
            State::Done => (theme::added(), Some(theme::added()), "✓"),
            State::Stale => (theme::warning(), None, "↻"),
        };
        let host = host.clone();
        Some(
            div()
                .id(("review-box", ix))
                .flex_none()
                .size(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(3.))
                .border_2()
                .border_color(border)
                .when_some(fill, |s, c| s.bg(c))
                .text_size(px(10.))
                .font_weight(FontWeight::BOLD)
                .text_color(if state == State::Done {
                    theme::base()
                } else {
                    theme::warning()
                })
                .cursor_pointer()
                .hover(|s| s.border_color(theme::text()))
                .tooltip_show_delay(LONG)
                .tooltip(move |_, cx| cx.new(|_| TipOwned(said.clone())).into())
                // A new mark pops in (#56, 12).
                .child(motion::appear(
                    div().child(glyph),
                    ("review-tick", state as usize),
                    motion::FAST,
                    |tick, v| tick.opacity(v),
                ))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_: &ClickEvent, _, cx| {
                    host.changes(ChangesEvent::ToggleReviewed(ix), cx)
                }),
        )
    }

    /// "7/17" with a bar under it, and Mark all / Unmark all.
    fn render_review_progress(&self, host: &Rc<dyn Host>) -> Option<impl IntoElement + use<>> {
        let (done, total) = self.review_progress()?;
        let filtered = !self.pfilter.trim().is_empty();
        let shown = self.file_order();
        let shown_done = shown
            .iter()
            .all(|&ix| self.review_state(ix) == Some(State::Done));
        let action = |id: &'static str, label: String| {
            div()
                .id(id)
                .flex_none()
                .px(px(6.))
                .rounded(px(4.))
                .text_size(px(11.))
                .text_color(theme::muted())
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                .child(label)
        };
        let host = host.clone();
        let toggle = action(
            "review-all",
            match (shown_done, filtered) {
                (true, false) => "Unmark all".to_owned(),
                (true, true) => format!("Unmark {} shown", shown.len()),
                (false, false) => "Mark all".to_owned(),
                (false, true) => format!("Mark {} shown", shown.len()),
            },
        )
        .tooltip_show_delay(LONG)
        .tooltip(|_, cx| {
            cx.new(|_| Tip("Mark or unmark every file in the list  ⌥⇧V"))
                .into()
        })
        .on_click(move |_: &ClickEvent, _, cx| host.changes(ChangesEvent::ReviewAll, cx));
        Some(
            div()
                .flex_none()
                .px(px(14.))
                .pb(px(6.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            // Only the numbers: the total over the files says what they count (#157).
                            hover::cut(
                                "progress-count",
                                if done == total {
                                    format!("{done}/{total} ✓")
                                } else {
                                    format!("{done}/{total}")
                                },
                            )
                            .flex_1()
                            .text_size(px(11.))
                            .text_color(if done == total {
                                theme::added()
                            } else {
                                theme::muted()
                            }),
                        )
                        .child(toggle),
                )
                .child(
                    div()
                        .h(px(4.))
                        .w_full()
                        .rounded(px(2.))
                        .bg(theme::hover())
                        // It grows to the new share (#56, 12).
                        .child(motion::toward_value(
                            div().h_full().rounded(px(2.)).bg(theme::added()),
                            "review-bar",
                            done as f32 / total.max(1) as f32,
                            motion::BASE,
                            |bar, share| bar.w(gpui::relative(share.clamp(0., 1.))),
                        )),
                ),
        )
    }
}

/// On file `path`'s row: a pencil while a comment written there (`draft`) waits without the keyboard (#168).
fn draft_mark(path: &str, draft: Option<&str>) -> Option<AnyElement> {
    (draft == Some(path)).then(|| {
        div()
            .id("draft-mark")
            .flex_none()
            .child(
                icons::icon("pencil")
                    .size(px(13.))
                    .text_color(theme::focus()),
            )
            .tooltip_show_delay(LONG)
            .tooltip(|_, cx| cx.new(|_| Tip("A comment being written")).into())
            .into_any_element()
    })
}

/// "generated" beside a lock or generated file.
fn generated_tag(file: &FileDiff) -> Option<gpui::Div> {
    artoo_core::generated::is_generated(file.path()).then(|| {
        div()
            .flex_none()
            .text_size(px(10.))
            .text_color(theme::faint())
            .child("generated")
    })
}

/// A file's added and removed line counts.
fn stats(file: &FileDiff) -> impl IntoElement + use<> {
    line_counts(file.added, file.removed)
}

/// Lines added and removed, green and red, each only when there are some.
fn line_counts(added: usize, removed: usize) -> gpui::Div {
    div()
        .flex()
        .gap_1()
        .text_size(px(11.))
        .when(added > 0, |s| {
            s.child(div().text_color(theme::added()).child(format!("+{added}")))
        })
        .when(removed > 0, |s| {
            s.child(
                div()
                    .text_color(theme::removed())
                    .child(format!("−{removed}")),
            )
        })
}

/// What a long hover on a file's name says: its change, its whole path, its +/−.
fn change_tip(file: &FileDiff) -> String {
    format!(
        "{} · {} · +{} −{}",
        status_word(file.change, file.old_path.as_deref()),
        file.path(),
        file.added,
        file.removed
    )
}

/// What a status letter stands for.
fn status_word(change: ChangeKind, old: Option<&str>) -> String {
    match change {
        ChangeKind::Added => "Added".into(),
        ChangeKind::Deleted => "Deleted".into(),
        ChangeKind::Renamed => format!("Renamed from {}", old.unwrap_or_default()),
        ChangeKind::Copied => format!("Copied from {}", old.unwrap_or_default()),
        ChangeKind::TypeChanged => "Type changed".into(),
        ChangeKind::Modified => "Modified".into(),
    }
}
