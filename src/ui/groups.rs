//! Groups and order of the pages open under a space (#246, Design round 46): a group is a name and the pages in
//! it, by what each is (`Opened::item`), so one may come to hold pages of several spaces; today a page moves only
//! among those of its own space. The order is the open pages' own (`Nav::open`), a group's pages standing
//! together where its first one is. Kept per project in `nav.txt`; the last page out ends the group.

use artoo_core::route::Route;
use serde::{Deserialize, Serialize};

/// What a group is called until it is named.
pub(super) const NEW: &str = "New group";

/// A group's key in the menu, beside a page's route.
const KEY: &str = "group=";

/// A group of open pages.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Group {
    pub id: u64,
    pub name: String,
    /// Folded: its head alone, with the count.
    pub shut: bool,
    pub items: Vec<Route>,
}

/// A group as `nav.txt` keeps it.
#[derive(Serialize, Deserialize)]
struct Kept {
    name: String,
    #[serde(default)]
    shut: bool,
    items: Vec<String>,
}

/// What a dragged row or the row it is dropped on is.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Unit {
    Page(Route),
    Group(u64),
}

impl Unit {
    pub fn read(key: &str) -> Option<Unit> {
        match key.strip_prefix(KEY) {
            Some(id) => id.parse().ok().map(Unit::Group),
            None => Route::parse(key).map(Unit::Page),
        }
    }

    pub fn key(&self) -> String {
        match self {
            Unit::Page(item) => item.text(),
            Unit::Group(id) => format!("{KEY}{id}"),
        }
    }
}

/// The open pages' order and their groups, changed together.
#[derive(Clone, Default, Debug, PartialEq)]
pub(super) struct Groups {
    pub list: Vec<Group>,
    next: u64,
}

impl Groups {
    pub fn of(&self, item: &Route) -> Option<&Group> {
        self.list.iter().find(|g| g.items.contains(item))
    }

    pub fn get(&self, id: u64) -> Option<&Group> {
        self.list.iter().find(|g| g.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Group> {
        self.list.iter_mut().find(|g| g.id == id)
    }

    /// `item` out of whatever group holds it; the group stays, empty or not (`ended`).
    pub fn leave(&mut self, item: &Route) {
        for g in &mut self.list {
            g.items.retain(|i| i != item);
        }
    }

    /// The groups no page is left in, taken out.
    pub fn ended(&mut self) -> Vec<Group> {
        let (kept, ended) = std::mem::take(&mut self.list)
            .into_iter()
            .partition(|g| !g.items.is_empty());
        self.list = kept;
        ended
    }

    /// A group put back as it was (Undo): its pages leave where they went.
    pub fn restore(&mut self, group: Group) {
        for item in &group.items {
            self.leave(item);
        }
        self.list.retain(|g| g.id != group.id);
        self.list.push(group);
    }

    /// A new group of `items`, the first one's place its place; its id.
    pub fn start(&mut self, order: &mut Vec<Route>, items: &[Route]) -> u64 {
        for item in items {
            self.leave(item);
        }
        self.next += 1;
        let id = self.next;
        self.list.push(Group {
            id,
            name: NEW.to_owned(),
            shut: false,
            items: items.to_vec(),
        });
        // The others come to stand after the first.
        for pair in items.windows(2) {
            move_by(order, &pair[1], &pair[0], true);
        }
        self.gather(order);
        id
    }

    /// `item` into group `id`, after its last page.
    pub fn join(&mut self, order: &mut Vec<Route>, item: &Route, id: u64) {
        let Some(last) = self.get(id).and_then(|g| {
            (order.iter().rev())
                .find(|i| *i != item && g.items.contains(i))
                .cloned()
        }) else {
            return;
        };
        self.leave(item);
        if let Some(g) = self.get_mut(id) {
            g.items.push(item.clone());
        }
        move_by(order, item, &last, true);
        self.gather(order);
    }

    /// `moving` before or after `anchor` in the order. A page takes the anchor's group, or none: beside a group's
    /// head it stands outside. A group moves whole, and never into another.
    pub fn place(&mut self, order: &mut Vec<Route>, moving: &Unit, anchor: &Unit, after: bool) {
        // The page the move is measured from, and the group that goes with it.
        let (beside, into) = match anchor {
            Unit::Page(item) => (item.clone(), self.of(item).map(|g| g.id)),
            Unit::Group(id) => match self.edge(order, *id, after) {
                Some(item) => (item, None),
                None => return,
            },
        };
        match moving {
            Unit::Page(item) => {
                if *item == beside && into.is_some() {
                    return;
                }
                // Measured before it leaves: beside its own group's head it steps out.
                let beside = match (anchor, self.of(item).map(|g| g.id)) {
                    (Unit::Group(id), Some(own)) if own == *id => (order.iter())
                        .find(|i| *i != item && self.of(i).is_some_and(|g| g.id == *id))
                        .cloned()
                        .unwrap_or(beside),
                    _ => beside,
                };
                self.leave(item);
                if let Some(g) = into.and_then(|id| self.get_mut(id)) {
                    g.items.push(item.clone());
                }
                move_by(order, item, &beside, after);
            }
            Unit::Group(id) => {
                let beside = match into {
                    Some(other) if other == *id => return,
                    Some(other) => match self.edge(order, other, after) {
                        Some(item) => item,
                        None => return,
                    },
                    None => beside,
                };
                let Some(g) = self.get(*id) else {
                    return;
                };
                let block: Vec<Route> = (order.iter())
                    .filter(|i| g.items.contains(i))
                    .cloned()
                    .collect();
                if block.contains(&beside) {
                    return;
                }
                order.retain(|i| !block.contains(i));
                let Some(at) = order.iter().position(|i| *i == beside) else {
                    return;
                };
                let at = at + usize::from(after);
                order.splice(at..at, block);
            }
        }
        self.gather(order);
    }

    /// Group `id`'s first page in the order, or with `last` its last.
    fn edge(&self, order: &[Route], id: u64, last: bool) -> Option<Route> {
        let g = self.get(id)?;
        let mut members = order.iter().filter(|i| g.items.contains(i));
        match last {
            true => members.next_back(),
            false => members.next(),
        }
        .cloned()
    }

    /// Each group's pages brought together where its first one stands, their order kept.
    pub fn gather(&self, order: &mut Vec<Route>) {
        let mut out: Vec<Route> = Vec::with_capacity(order.len());
        for item in order.iter() {
            if out.contains(item) {
                continue;
            }
            match self.of(item) {
                Some(g) => out.extend(order.iter().filter(|i| g.items.contains(i)).cloned()),
                None => out.push(item.clone()),
            }
        }
        *order = out;
    }

    /// As a column of `nav.txt`; none without a group.
    pub fn line(&self) -> Option<String> {
        if self.list.is_empty() {
            return None;
        }
        let kept: Vec<Kept> = (self.list.iter())
            .map(|g| Kept {
                name: g.name.clone(),
                shut: g.shut,
                items: g.items.iter().map(Route::text).collect(),
            })
            .collect();
        serde_json::to_string(&kept)
            .ok()
            .map(|json| format!("groups={json}"))
    }

    /// `column` read back, of the pages in `open` alone; none when it is another column.
    pub fn read(column: &str, open: &[Route]) -> Option<Groups> {
        let kept: Vec<Kept> = serde_json::from_str(column.strip_prefix("groups=")?).ok()?;
        let mut groups = Groups::default();
        for k in kept {
            let items: Vec<Route> = (k.items.iter())
                .filter_map(|t| Route::parse(t))
                .filter(|i| open.contains(i) && groups.of(i).is_none())
                .collect();
            if items.is_empty() {
                continue;
            }
            groups.next += 1;
            groups.list.push(Group {
                id: groups.next,
                name: k.name,
                shut: k.shut,
                items,
            });
        }
        Some(groups)
    }
}

/// `item` moved to before or after `beside`.
fn move_by(order: &mut Vec<Route>, item: &Route, beside: &Route, after: bool) {
    if item == beside {
        return;
    }
    let Some(from) = order.iter().position(|i| i == item) else {
        return;
    };
    let moved = order.remove(from);
    match order.iter().position(|i| i == beside) {
        Some(at) => order.insert(at + usize::from(after), moved),
        None => order.insert(from, moved),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(n: u32) -> Route {
        Route {
            kind: "requests.request".into(),
            args: format!("{{\"number\":{n}}}"),
        }
    }

    fn pages(n: &[u32]) -> Vec<Route> {
        n.iter().map(|&n| page(n)).collect()
    }

    #[test]
    fn a_page_dropped_on_another_makes_a_group_of_both() {
        let (mut groups, mut order) = (Groups::default(), pages(&[1, 2, 3, 4]));
        let id = groups.start(&mut order, &[page(2), page(4)]);
        assert_eq!(order, pages(&[1, 2, 4, 3]));
        assert_eq!(groups.get(id).unwrap().items, pages(&[2, 4]));
        groups.join(&mut order, &page(1), id);
        assert_eq!(order, pages(&[2, 4, 1, 3]));
    }

    #[test]
    fn a_page_takes_the_group_it_is_dropped_beside() {
        let (mut groups, mut order) = (Groups::default(), pages(&[1, 2, 3, 4]));
        let id = groups.start(&mut order, &[page(1), page(2)]);
        // Between the group's pages: in it.
        groups.place(
            &mut order,
            &Unit::Page(page(4)),
            &Unit::Page(page(2)),
            false,
        );
        assert_eq!(order, pages(&[1, 4, 2, 3]));
        assert_eq!(groups.get(id).unwrap().items.len(), 3);
        // Before its head: out of it.
        groups.place(&mut order, &Unit::Page(page(1)), &Unit::Group(id), false);
        assert_eq!(order, pages(&[1, 4, 2, 3]));
        assert!(groups.of(&page(1)).is_none());
        // Beside a page in none: out.
        groups.place(&mut order, &Unit::Page(page(4)), &Unit::Page(page(3)), true);
        assert_eq!(order, pages(&[1, 2, 3, 4]));
        assert!(groups.of(&page(4)).is_none());
    }

    #[test]
    fn a_group_moves_whole_and_into_no_other() {
        let (mut groups, mut order) = (Groups::default(), pages(&[1, 2, 3, 4, 5]));
        let a = groups.start(&mut order, &[page(1), page(2)]);
        let b = groups.start(&mut order, &[page(4), page(5)]);
        groups.place(&mut order, &Unit::Group(b), &Unit::Group(a), false);
        assert_eq!(order, pages(&[4, 5, 1, 2, 3]));
        // Dropped beside a page of another group: past the whole of it.
        groups.place(&mut order, &Unit::Group(b), &Unit::Page(page(1)), true);
        assert_eq!(order, pages(&[1, 2, 4, 5, 3]));
        assert_eq!(groups.of(&page(4)).unwrap().id, b);
    }

    #[test]
    fn the_last_page_out_ends_the_group_and_undo_brings_it_back() {
        let (mut groups, mut order) = (Groups::default(), pages(&[1, 2]));
        let id = groups.start(&mut order, &[page(2)]);
        groups.leave(&page(2));
        let ended = groups.ended();
        assert_eq!(ended.len(), 1);
        assert!(groups.list.is_empty());
        let mut back = ended[0].clone();
        back.items = pages(&[2]);
        groups.restore(back);
        assert_eq!(groups.of(&page(2)).unwrap().id, id);
    }

    #[test]
    fn groups_come_back_from_their_line_of_the_pages_still_open() {
        let (mut groups, mut order) = (Groups::default(), pages(&[1, 2, 3]));
        let id = groups.start(&mut order, &[page(1), page(2)]);
        let g = groups.get_mut(id).unwrap();
        (g.name, g.shut) = ("Release 2.5".into(), true);
        let line = groups.line().unwrap();
        assert!(!line.contains('\t'));
        let back = Groups::read(&line, &pages(&[2, 3])).unwrap();
        assert_eq!(back.list[0].name, "Release 2.5");
        assert!(back.list[0].shut);
        assert_eq!(back.list[0].items, pages(&[2]));
        assert!(Groups::read(&line, &pages(&[3])).unwrap().list.is_empty());
        assert!(Groups::read("code.file{}", &order).is_none());
        let key = Unit::Group(3).key();
        assert_eq!(Unit::read(&key), Some(Unit::Group(3)));
        assert_eq!(Unit::read(&page(1).text()), Some(Unit::Page(page(1))));
    }
}
