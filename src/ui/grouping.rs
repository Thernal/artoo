//! The tab's side of the groups of open pages (#246, Design rounds 46 and 48; the rules in `groups`): the Groups tab,
//! Move to ▸ in a page's and a group's menu, a row dropped on another, on a tab or at the menu's end, a group folded,
//! named, made inside another, ungrouped, its pages closed, a row moved by ⌃⌥↑ ⌃⌥↓, and the toast with Undo when a
//! group goes to Groups, the last page leaves one, or the Groups tab ends.

use super::Workspace;
use super::groups::{At, Groups, List, Plan, Target, Unit};
use super::menu::{Act, Entry};
use super::nav::Opened;
use super::spaces::GROUPS;
use artoo_core::route::Route;
use artoo_shell_api::spaces::{DropAt, Drops, Space};
use artoo_ui_kit::toast::Words;
use gpui::{App, Context, Pixels, Point};
use std::cell::Cell;
use std::rc::Rc;

/// What stood before a change of the groups: the tree, and the open pages with their places — what Undo puts back.
pub(super) struct Before {
    groups: Groups,
    open: Vec<Opened>,
}

/// The tab of each open page, as `Groups` asks it.
pub(super) struct Tabs(Vec<(Route, String)>);

impl Tabs {
    pub fn of(&self, item: &Route) -> String {
        (self.0.iter())
            .find(|(o, _)| o == item)
            .map_or_else(String::new, |(_, t)| t.clone())
    }
}

/// The list a space's row stands for: a tab's, the Groups tab's.
pub(super) fn list_of(space: Space) -> List {
    match space {
        GROUPS => List::Groups,
        tab => List::Tab(tab.name().to_owned()),
    }
}

impl From<DropAt> for At {
    fn from(at: DropAt) -> At {
        match at {
            DropAt::Before => At::Before,
            DropAt::After => At::After,
            DropAt::Onto => At::Onto,
        }
    }
}

impl Workspace {
    /// The open pages, each with its tab.
    pub(super) fn open_tabs(&self) -> Tabs {
        Tabs(
            (self.nav.open.iter())
                .map(|o| {
                    let tab = self.item_space(&o.item);
                    (
                        o.item.clone(),
                        tab.map_or_else(String::new, |s| s.name().to_owned()),
                    )
                })
                .collect(),
        )
    }

    /// The tree as the open pages are now, without changing the kept one: what the menu draws.
    pub(super) fn synced_groups(&self) -> Groups {
        let mut groups = self.nav.groups.clone();
        groups.sync(&self.open_tabs().0);
        groups
    }

    /// The kept tree told of the pages opened and closed since.
    pub(super) fn sync_groups(&mut self) {
        let open = self.open_tabs();
        self.nav.groups.sync(&open.0);
    }

    pub(super) fn before_groups(&self) -> Before {
        Before {
            groups: self.nav.groups.clone(),
            open: self.nav.open.clone(),
        }
    }

    /// After a change from `before`: a group no page is left in ends, the order follows the tree, and it is kept.
    /// What changed more than an order is said with Undo: a group gone to Groups, a group ended, the Groups tab gone.
    pub(super) fn regrouped(&mut self, before: Before, cx: &mut Context<Self>) {
        self.sync_groups();
        let ended = self.nav.groups.ended(&before.groups);
        self.gather_open();
        let was = &before.groups;
        let came = (self.nav.groups.all(&List::Groups).into_iter())
            .find(|(_, g)| {
                (was.find(&Unit::Group(g.id)))
                    .and_then(|(list, _)| was.home(&list))
                    .is_some_and(|home| home != List::Groups)
            })
            .map(|(_, g)| g.name.clone());
        let gone = !was.shared.is_empty() && self.nav.groups.shared.is_empty();
        let words = match (came, ended.first()) {
            (Some(name), _) => Some(Words::new(format!("{name} moved to Groups"))),
            (None, Some(name)) => Some(Words::new("Group ended").about(name.clone())),
            (None, None) if gone => Some(Words::new("Groups closed")),
            _ => None,
        };
        if let Some(words) = words {
            let (tab, back) = (cx.weak_entity(), Cell::new(Some(before)));
            let undo = Rc::new(move |cx: &mut App| {
                if let Some(before) = back.take() {
                    tab.update(cx, |this, cx| this.undo_groups(before, cx)).ok();
                }
            });
            self.toasts
                .update(cx, |t, cx| t.show_with(words, "Undo", undo, cx));
        }
        self.save_nav();
        cx.notify();
    }

    /// Undo: the tree as it was, and the pages closed since opened again where they stood.
    fn undo_groups(&mut self, before: Before, cx: &mut Context<Self>) {
        for (at, page) in before.open.iter().enumerate() {
            if self.nav.opened(&page.item).is_none() {
                let at = at.min(self.nav.open.len());
                self.nav.open.insert(at, page.clone());
            }
        }
        self.nav.groups = before.groups;
        self.sync_groups();
        self.gather_open();
        self.save_nav();
        cx.notify();
    }

    /// The open pages in the tree's order: ⌥⌘→ and the menu's shift-pick go as the menu draws them.
    pub(super) fn gather_open(&mut self) {
        let order = self.nav.groups.order();
        let at = |item: &Route| order.iter().position(|i| i == item).unwrap_or(usize::MAX);
        self.nav.open.sort_by_key(|o| at(&o.item));
    }

    /// A change of the tree, `change` told it and each page's tab; a page put somewhere by hand stays, no longer
    /// only looked at (#194).
    fn regroup<T>(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Groups, &Tabs) -> T,
    ) -> T {
        self.sync_groups();
        let before = self.before_groups();
        let tabs = self.open_tabs();
        let out = change(&mut self.nav.groups, &tabs);
        let groups = &self.nav.groups;
        for open in &mut self.nav.open {
            let unit = Unit::Page(open.item.clone());
            if groups.of(&open.item).is_some() || groups.find(&unit) != before.groups.find(&unit) {
                open.preview = false;
            }
        }
        self.regrouped(before, cx);
        out
    }

    /// What `from` let go on `to` would do; none where it may not go.
    fn drop_plan(&self, groups: &Groups, tabs: &Tabs, from: &Unit, to: Target) -> Option<Plan> {
        groups.plan(from, to, &|item| tabs.of(item))
    }

    /// What a drop of the row being dragged does on row `row`, as the menu is drawn during the drag.
    pub(super) fn row_drops(
        &self,
        groups: &Groups,
        tabs: &Tabs,
        dragged: Option<&Unit>,
        row: &Unit,
    ) -> Drops {
        let Some(from) = dragged.filter(|d| *d != row) else {
            return Drops::default();
        };
        let plan = |at| self.drop_plan(groups, tabs, from, Target::Row(row.clone(), at));
        let onto = plan(At::Onto);
        Drops {
            before: plan(At::Before).is_some(),
            // Below a group's head stands what is in it: its After is not drawn.
            after: matches!(row, Unit::Page(_)) && plan(At::After).is_some(),
            onto: onto.is_some(),
            hint: onto.and_then(|p| p.hint),
        }
    }

    /// What a drop of the row being dragged does on the row of `space`, or at the menu's end (`GROUPS`).
    pub(super) fn list_drops(
        &self,
        groups: &Groups,
        tabs: &Tabs,
        dragged: Option<&Unit>,
        space: Space,
    ) -> Drops {
        let Some(from) = dragged else {
            return Drops::default();
        };
        let plan = self.drop_plan(groups, tabs, from, Target::List(list_of(space)));
        Drops {
            onto: plan.is_some(),
            hint: plan.and_then(|p| p.hint),
            ..Drops::default()
        }
    }

    /// The row of key `from` let go on `to`: done as `Groups::plan` says; a group a drop made has its name typed.
    pub(super) fn drop_unit(&mut self, from: &str, to: Target, cx: &mut Context<Self>) {
        let Some(from) = Unit::read(from) else {
            return;
        };
        let (groups, tabs) = (self.synced_groups(), self.open_tabs());
        let Some(plan) = self.drop_plan(&groups, &tabs, &from, to) else {
            return;
        };
        if let Some(id) = self.regroup(cx, |g, _| g.apply(&from, &plan)) {
            self.start_group_rename(id, cx);
        }
    }

    /// `Move to ▸` in the menu of a page or a group: its entry, which opens where it may go at `at`.
    pub(super) fn move_to_entry(&self, unit: Unit, at: Point<Pixels>) -> Entry {
        Entry::new(
            "Move to",
            Act::new(move |this, _, cx| this.move_menu(unit.clone(), at, cx)),
        )
        .quiet("▸")
    }

    /// The tops of the menu in its order: each tab's list, and the Groups tab's, with their names.
    fn menu_tops(&self, cx: &App) -> Vec<(List, &'static str)> {
        let offered = self.offered_spaces(cx);
        (artoo_shell_api::spaces::ordered(&self.layout.spaces, &offered).into_iter())
            .map(|s| (list_of(s.space), s.label))
            .collect()
    }

    /// Where `unit` may go, in the menu's order: the groups of each tab and of Groups, each in by how deep it is,
    /// the one it is in checked; for a group the tops it may go to (its tab, Groups); for a page New group… last.
    fn move_menu(&mut self, unit: Unit, at: Point<Pixels>, cx: &mut Context<Self>) {
        let (groups, tabs) = (self.synced_groups(), self.open_tabs());
        let here = groups.find(&unit).map(|(list, _)| list);
        let goes = |list: &List| {
            self.drop_plan(&groups, &tabs, &unit, Target::List(list.clone()))
                .is_some()
        };
        let mut menu: Vec<Vec<Entry>> = Vec::new();
        for (top, label) in self.menu_tops(cx) {
            let mut entries = Vec::new();
            if matches!(unit, Unit::Group(_)) && here.as_ref() != Some(&top) && goes(&top) {
                entries.push(self.move_entry(&unit, top.clone(), label.to_owned()));
            }
            for (depth, group) in groups.all(&top) {
                let into = List::Group(group.id);
                let name = format!("{}{}", "\u{2003}".repeat(depth), group.name);
                if here.as_ref() == Some(&into) {
                    entries.push(Entry::new(name, Act::new(|_, _, _| {})).checked(true));
                } else if goes(&into) {
                    entries.push(self.move_entry(&unit, into, name));
                }
            }
            if !entries.is_empty() {
                menu.push(entries);
            }
        }
        if let (Unit::Page(item), Some(list)) = (&unit, here) {
            let item = item.clone();
            menu.push(vec![Entry::new(
                "New group\u{2026}",
                Act::new(move |this, _, cx| {
                    let id = this.regroup(cx, |g, _| g.start(&list, std::slice::from_ref(&item)));
                    this.start_group_rename(id, cx);
                }),
            )]);
        }
        self.open_menu(at, menu, cx);
    }

    fn move_entry(&self, unit: &Unit, into: List, name: String) -> Entry {
        let unit = unit.clone();
        Entry::new(
            name,
            Act::new(move |this, _, cx| {
                this.drop_unit(&unit.key(), Target::List(into.clone()), cx)
            }),
        )
    }

    /// A group's head right-clicked: New subgroup…, Rename…, Move to ▸, then Ungroup and Close pages.
    pub(super) fn group_context(&mut self, id: u64, at: Point<Pixels>, cx: &mut Context<Self>) {
        let sub = Entry::new(
            "New subgroup\u{2026}",
            Act::new(move |this, _, cx| {
                if let Some(new) = this.regroup(cx, |g, _| g.subgroup(id)) {
                    if let Some(group) = this.nav.groups.get_mut(id) {
                        group.shut = false;
                    }
                    this.start_group_rename(new, cx);
                }
            }),
        );
        let rename = Entry::new(
            "Rename\u{2026}",
            Act::new(move |this, _, cx| this.start_group_rename(id, cx)),
        )
        .key("or double-click");
        let ungroup = Entry::new(
            "Ungroup",
            Act::new(move |this, _, cx| {
                this.regroup(cx, |g, tabs| g.ungroup(id, &|item| tabs.of(item)));
            }),
        );
        let close = Entry::new(
            "Close pages",
            Act::new(move |this, _, cx| this.close_group(id, cx)),
        );
        let menu = vec![
            vec![sub, rename, self.move_to_entry(Unit::Group(id), at)],
            vec![ungroup, close],
        ];
        self.open_menu(at, menu, cx);
    }

    /// Close pages: every page under group `id` closes, and the group ends, said with Undo.
    fn close_group(&mut self, id: u64, cx: &mut Context<Self>) {
        self.sync_groups();
        let before = self.before_groups();
        for item in self.nav.groups.pages(id) {
            self.close_one(&item, cx);
        }
        self.regrouped(before, cx);
    }

    /// ⌃⌥↑ ⌃⌥↓: the row on screen in the menu one place up or down its list — the page shown, or the head of the
    /// folded group it is under. Whether there was one.
    pub(super) fn step_row(&mut self, up: bool, cx: &mut Context<Self>) -> bool {
        let Some(item) = self.nav_page().and_then(|p| self.item_of(p)) else {
            return false;
        };
        if self.nav.opened(&item).is_none() {
            return false;
        }
        self.sync_groups();
        let groups = &self.nav.groups;
        let mut unit = Unit::Page(item);
        let mut cur = unit.clone();
        while let Some((List::Group(id), _)) = groups.find(&cur) {
            if groups.get(id).is_some_and(|g| g.shut) {
                unit = Unit::Group(id);
            }
            cur = Unit::Group(id);
        }
        self.regroup(cx, |g, _| g.step(&unit, up));
        true
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
                super::groups::NEW.to_owned()
            } else {
                name
            };
            self.save_nav();
        }
    }
}
