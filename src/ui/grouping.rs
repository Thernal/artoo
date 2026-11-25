//! The tab's side of the groups of open pages (#246, `groups`): Move to group ▸ in a page's menu, a row dropped on
//! another, a group folded and named, and the toast with Undo when the last page leaves one.

use super::Workspace;
use super::groups::{Group, Groups, NEW, Unit};
use super::menu::{Act, Entry};
use super::nav::Opened;
use artoo_core::route::Route;
use artoo_shell_api::spaces::{DropAt, Space};
use artoo_ui_kit::toast::Words;
use gpui::{App, Context, Pixels, Point};
use std::cell::Cell;
use std::rc::Rc;

/// What stood before a change that may end a group: the groups, and the open pages with their places.
pub(super) struct Before {
    groups: Groups,
    open: Vec<Opened>,
}

impl Workspace {
    /// What stands now, before `item` leaves its group or the menu; none when it is in no group.
    pub(super) fn grouping_before(&self, item: &Route) -> Option<Before> {
        self.nav.groups.of(item)?;
        Some(Before {
            groups: self.nav.groups.clone(),
            open: self.nav.open.clone(),
        })
    }

    /// After a change: a group no page is left in ends, said with Undo.
    pub(super) fn groups_left(&mut self, before: Option<Before>, cx: &mut Context<Self>) {
        let ended = self.nav.groups.ended();
        let Some(before) = before else {
            return;
        };
        for group in ended {
            // As it was, with its last page and where that stood.
            let Some(was) = before.groups.get(group.id).cloned() else {
                continue;
            };
            let pages: Vec<(usize, Opened)> = (before.open.iter().enumerate())
                .filter(|(_, o)| was.items.contains(&o.item))
                .map(|(at, o)| (at, o.clone()))
                .collect();
            let words = Words::new("Group ended").about(was.name.clone());
            let (tab, back) = (cx.weak_entity(), Cell::new(Some((was, pages))));
            let undo = Rc::new(move |cx: &mut App| {
                if let Some((group, pages)) = back.take() {
                    tab.update(cx, |this, cx| this.restore_group(group, pages, cx))
                        .ok();
                }
            });
            self.toasts
                .update(cx, |t, cx| t.show_with(words, "Undo", undo, cx));
        }
    }

    /// Undo: the group is back, its last page where it was, opened again when it was closed.
    fn restore_group(&mut self, group: Group, pages: Vec<(usize, Opened)>, cx: &mut Context<Self>) {
        for (at, page) in pages {
            match self.nav.open.iter().position(|o| o.item == page.item) {
                Some(now) => {
                    let page = self.nav.open.remove(now);
                    self.nav.open.insert(at.min(self.nav.open.len()), page);
                }
                None => self.nav.open.insert(at.min(self.nav.open.len()), page),
            }
        }
        self.nav.groups.restore(group);
        self.nav.groups.ended();
        self.gather_open();
        self.save_nav();
        cx.notify();
    }

    /// The open pages in the order of `items`.
    fn reorder_open(&mut self, items: &[Route]) {
        let at = |item: &Route| items.iter().position(|i| i == item).unwrap_or(usize::MAX);
        self.nav.open.sort_by_key(|o| at(&o.item));
    }

    fn open_order(&self) -> Vec<Route> {
        self.nav.open.iter().map(|o| o.item.clone()).collect()
    }

    /// Each group's pages brought together.
    pub(super) fn gather_open(&mut self) {
        let mut order = self.open_order();
        self.nav.groups.gather(&mut order);
        self.reorder_open(&order);
    }

    /// A change of the groups and the order, `change` told both: kept, and a group left empty said.
    fn regroup<T>(
        &mut self,
        item: Option<&Route>,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Groups, &mut Vec<Route>) -> T,
    ) -> T {
        let before = item.and_then(|item| self.grouping_before(item));
        let mut order = self.open_order();
        let out = change(&mut self.nav.groups, &mut order);
        self.reorder_open(&order);
        // A page put somewhere by hand stays: no longer only looked at (#194).
        for open in &mut self.nav.open {
            if self.nav.groups.of(&open.item).is_some() || Some(&open.item) == item {
                open.preview = false;
            }
        }
        self.groups_left(before, cx);
        self.save_nav();
        cx.notify();
        out
    }

    /// A row of `space` dropped on another: onto a page, a group of both, or into its group; onto a head, into the
    /// group; at an edge, its place there.
    pub(super) fn drop_row(
        &mut self,
        space: Space,
        from: &str,
        to: &str,
        at: DropAt,
        cx: &mut Context<Self>,
    ) {
        let (Some(from), Some(to)) = (Unit::read(from), Unit::read(to)) else {
            return;
        };
        // Pages of several spaces in one group wait on their design: a page moves among its own space's.
        let here = |unit: &Unit| match unit {
            Unit::Page(item) => self.item_space(item) == Some(space),
            Unit::Group(id) => (self.nav.groups.get(*id))
                .is_some_and(|g| g.items.iter().any(|i| self.item_space(i) == Some(space))),
        };
        if from == to || !here(&from) || !here(&to) {
            return;
        }
        let moved = match &from {
            Unit::Page(item) => Some(item.clone()),
            Unit::Group(_) => None,
        };
        match (at, moved.as_ref(), &to) {
            (DropAt::Onto, Some(item), Unit::Group(id)) => {
                self.regroup(moved.as_ref(), cx, |g, order| g.join(order, item, *id));
            }
            (DropAt::Onto, Some(item), Unit::Page(target)) => {
                match self.nav.groups.of(target).map(|g| g.id) {
                    Some(id) => {
                        self.regroup(moved.as_ref(), cx, |g, order| g.join(order, item, id));
                    }
                    None => {
                        let target = target.clone();
                        let id = self.regroup(moved.as_ref(), cx, |g, order| {
                            g.start(order, &[target.clone(), item.clone()])
                        });
                        if let Some(open) = self.nav.open.iter_mut().find(|o| o.item == target) {
                            open.preview = false;
                        }
                        self.start_group_rename(id, cx);
                    }
                }
            }
            (DropAt::Onto, None, _) => {}
            (at, _, _) => {
                self.regroup(moved.as_ref(), cx, |g, order| {
                    g.place(order, &from, &to, at == DropAt::After)
                });
            }
        }
    }

    /// `Move to group ▸` in the menu of open page `item`: its entry, which opens the groups at `at`.
    pub(super) fn move_to_group_entry(&self, item: &Route, at: Point<Pixels>) -> Entry {
        let item = item.clone();
        Entry::new(
            "Move to group",
            Act::new(move |this, _, cx| this.group_menu(item.clone(), at, cx)),
        )
        .quiet("▸")
    }

    /// The groups of the page's space, the one it is in checked, and New group….
    fn group_menu(&mut self, item: Route, at: Point<Pixels>, cx: &mut Context<Self>) {
        let space = self.item_space(&item);
        let own = self.nav.groups.of(&item).map(|g| g.id);
        let groups: Vec<Entry> = (self.nav.groups.list.iter())
            .filter(|g| g.items.iter().any(|i| self.item_space(i) == space))
            .map(|g| {
                let (id, item) = (g.id, item.clone());
                let entry = Entry::new(
                    g.name.clone(),
                    Act::new(move |this, _, cx| {
                        if own != Some(id) {
                            this.regroup(Some(&item), cx, |g, order| g.join(order, &item, id));
                        }
                    }),
                );
                match own == Some(id) {
                    true => entry.checked(true),
                    false => entry,
                }
            })
            .collect();
        let new = Entry::new(
            "New group\u{2026}",
            Act::new(move |this, _, cx| {
                let id = this.regroup(Some(&item), cx, |g, order| {
                    g.start(order, std::slice::from_ref(&item))
                });
                this.start_group_rename(id, cx);
            }),
        );
        let mut menu = vec![vec![new]];
        if !groups.is_empty() {
            menu.insert(0, groups);
        }
        self.open_menu(at, menu, cx);
    }

    /// A click on a group's head: folded, or open again.
    pub(super) fn fold_group(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(group) = self.nav.groups.get_mut(id) {
            group.shut = !group.shut;
            self.save_nav();
            cx.notify();
        }
    }

    /// The group's head becomes a field its name is typed in.
    pub(super) fn start_group_rename(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(name) = self.nav.groups.get(id).map(|g| g.name.clone()) else {
            return;
        };
        self.renaming = None;
        self.renaming_group = Some(id);
        self.label_name = name;
        self.start_field(super::lists::Field::Rename, cx);
    }

    /// The name typed for a group, kept; empty, it is a new group's.
    pub(super) fn save_group_name(&mut self) {
        let Some(id) = self.renaming_group else {
            return;
        };
        let name = self.label_name.trim().to_owned();
        if let Some(group) = self.nav.groups.get_mut(id) {
            group.name = if name.is_empty() {
                NEW.to_owned()
            } else {
                name
            };
            self.save_nav();
        }
    }
}
