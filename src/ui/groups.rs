//! Groups and order of the open pages (#246, Design rounds 46 and 48): a tree over them, per project. Each tab (a
//! space) has its list — its pages and its groups, in order —, and the Groups tab a list of groups whose pages may be
//! of any tab. A group holds pages and groups (subgroups), as deep as wanted; a page is in one place only. A group
//! under a tab holds that tab's pages; another tab's page put in it takes the whole top-level group to Groups. A new
//! or moved group goes to the bottom of its list, and so does a page opened later. Kept in `nav.txt` (`tree=`); the
//! last page out of a group ends it.

use artoo_core::route::Route;
use serde::{Deserialize, Serialize};

/// What a group is called until it is named.
pub(super) const NEW: &str = "New group";

/// A group's key in the menu, beside a page's route.
const KEY: &str = "group=";

/// Said over a row a drop would make a group on, and over a place that takes a group to Groups.
pub(super) const NEW_GROUP: &str = "New group";
pub(super) const TO_GROUPS: &str = "Move to Groups";

/// A group of open pages and groups.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Group {
    pub id: u64,
    pub name: String,
    /// Folded: its head alone, with the count.
    pub shut: bool,
    pub kids: Vec<Node>,
}

/// A row of a list: a page, or a group with what is in it.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Node {
    Page(Route),
    Group(Group),
}

/// A list rows stand in: a tab's (its space's id), the Groups tab's, a group's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum List {
    Tab(String),
    Groups,
    Group(u64),
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

/// Where a dragged row is let go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum At {
    Before,
    After,
    Onto,
}

/// What a drop does (`Groups::plan`), and what the place says while the drag is over it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Plan {
    /// The top-level group that goes to Groups first: another tab's page or group comes into it.
    pub lift: Option<u64>,
    pub act: Move,
    pub hint: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Move {
    /// Into `list`, before or after `anchor` there, else at its bottom.
    Put {
        list: List,
        anchor: Option<(Unit, bool)>,
    },
    /// A new group of `with` and the one moved, at the bottom of `list` (it does not take `with`'s place).
    Pair { list: List, with: Route },
}

/// A row as the menu draws it: how deep, and what.
pub(super) enum Row<'a> {
    Page(&'a Route),
    Head(&'a Group),
}

/// The open pages' order and their groups.
#[derive(Clone, Default, Debug, PartialEq)]
pub(super) struct Groups {
    /// Each tab's list, by its space's id.
    tabs: Vec<(String, Vec<Node>)>,
    /// The Groups tab's list: groups alone.
    pub shared: Vec<Node>,
    next: u64,
}

fn pages_in(nodes: &[Node], out: &mut Vec<Route>) {
    for node in nodes {
        match node {
            Node::Page(item) => out.push(item.clone()),
            Node::Group(g) => pages_in(&g.kids, out),
        }
    }
}

fn group_in(nodes: &[Node], id: u64) -> Option<&Group> {
    nodes.iter().find_map(|n| match n {
        Node::Group(g) if g.id == id => Some(g),
        Node::Group(g) => group_in(&g.kids, id),
        Node::Page(_) => None,
    })
}

fn group_in_mut(nodes: &mut [Node], id: u64) -> Option<&mut Group> {
    for node in nodes {
        if let Node::Group(g) = node {
            if g.id == id {
                return Some(g);
            }
            if let Some(found) = group_in_mut(&mut g.kids, id) {
                return Some(found);
            }
        }
    }
    None
}

fn is(node: &Node, unit: &Unit) -> bool {
    match (node, unit) {
        (Node::Page(a), Unit::Page(b)) => a == b,
        (Node::Group(g), Unit::Group(id)) => g.id == *id,
        _ => false,
    }
}

/// The list `unit` stands in, of those under `nodes` (`list` their own), and its place there.
fn find_in(nodes: &[Node], list: List, unit: &Unit) -> Option<(List, usize)> {
    if let Some(at) = nodes.iter().position(|n| is(n, unit)) {
        return Some((list, at));
    }
    nodes.iter().find_map(|n| match n {
        Node::Group(g) => find_in(&g.kids, List::Group(g.id), unit),
        Node::Page(_) => None,
    })
}

fn take_in(nodes: &mut Vec<Node>, unit: &Unit) -> Option<Node> {
    if let Some(at) = nodes.iter().position(|n| is(n, unit)) {
        return Some(nodes.remove(at));
    }
    nodes.iter_mut().find_map(|n| match n {
        Node::Group(g) => take_in(&mut g.kids, unit),
        Node::Page(_) => None,
    })
}

/// Pages for which `keep` is false taken out, at every depth.
fn retain_pages(nodes: &mut Vec<Node>, keep: &dyn Fn(&Route) -> bool) {
    nodes.retain(|n| !matches!(n, Node::Page(item) if !keep(item)));
    for node in nodes {
        if let Node::Group(g) = node {
            retain_pages(&mut g.kids, keep);
        }
    }
}

/// Groups with no page under them that `ended` says end, taken out from the deepest up; their names.
fn end_in(nodes: &mut Vec<Node>, ended: &dyn Fn(&Group) -> bool, out: &mut Vec<String>) {
    for node in nodes.iter_mut() {
        if let Node::Group(g) = node {
            end_in(&mut g.kids, ended, out);
        }
    }
    nodes.retain(|n| match n {
        Node::Group(g) if g.kids.is_empty() && ended(g) => {
            out.push(g.name.clone());
            false
        }
        _ => true,
    });
}

fn rows_in<'a>(nodes: &'a [Node], depth: usize, out: &mut Vec<(usize, Row<'a>)>) {
    for node in nodes {
        match node {
            Node::Page(item) => out.push((depth, Row::Page(item))),
            Node::Group(g) => {
                out.push((depth, Row::Head(g)));
                if !g.shut {
                    rows_in(&g.kids, depth + 1, out);
                }
            }
        }
    }
}

fn groups_in<'a>(nodes: &'a [Node], depth: usize, out: &mut Vec<(usize, &'a Group)>) {
    for node in nodes {
        if let Node::Group(g) = node {
            out.push((depth, g));
            groups_in(&g.kids, depth + 1, out);
        }
    }
}

impl Groups {
    /// Whether any group is kept (a project with none writes no `tree=`).
    pub fn any(&self) -> bool {
        let mut all = Vec::new();
        for (_, nodes) in &self.tabs {
            groups_in(nodes, 0, &mut all);
        }
        !all.is_empty() || !self.shared.is_empty()
    }

    pub fn list(&self, list: &List) -> Option<&Vec<Node>> {
        match list {
            List::Tab(tab) => self.tabs.iter().find(|(t, _)| t == tab).map(|(_, n)| n),
            List::Groups => Some(&self.shared),
            List::Group(id) => self.get(*id).map(|g| &g.kids),
        }
    }

    fn list_mut(&mut self, list: &List) -> Option<&mut Vec<Node>> {
        match list {
            List::Tab(tab) => {
                if !self.tabs.iter().any(|(t, _)| t == tab) {
                    self.tabs.push((tab.clone(), Vec::new()));
                }
                self.tabs.iter_mut().find(|(t, _)| t == tab).map(|(_, n)| n)
            }
            List::Groups => Some(&mut self.shared),
            List::Group(id) => self.get_mut(*id).map(|g| &mut g.kids),
        }
    }

    fn tops(&self) -> impl Iterator<Item = (List, &Vec<Node>)> {
        (self.tabs.iter().map(|(t, n)| (List::Tab(t.clone()), n)))
            .chain(std::iter::once((List::Groups, &self.shared)))
    }

    pub fn get(&self, id: u64) -> Option<&Group> {
        self.tops().find_map(|(_, nodes)| group_in(nodes, id))
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Group> {
        if let Some(g) = group_in_mut(&mut self.shared, id) {
            return Some(g);
        }
        self.tabs
            .iter_mut()
            .find_map(|(_, nodes)| group_in_mut(nodes, id))
    }

    /// The list `unit` stands in, and its place there.
    pub fn find(&self, unit: &Unit) -> Option<(List, usize)> {
        self.tops()
            .find_map(|(list, nodes)| find_in(nodes, list, unit))
    }

    /// The group page `item` is in, the innermost; none at a tab's top.
    pub fn of(&self, item: &Route) -> Option<&Group> {
        match self.find(&Unit::Page(item.clone()))? {
            (List::Group(id), _) => self.get(id),
            _ => None,
        }
    }

    /// The top of the tree `list` is under: a tab's list, or the Groups tab's.
    pub fn home(&self, list: &List) -> Option<List> {
        match list {
            List::Group(id) => self.home(&self.find(&Unit::Group(*id))?.0),
            top => Some(top.clone()),
        }
    }

    /// The top-level group `list` is in (itself at the top); none for a top list.
    fn root(&self, list: &List) -> Option<u64> {
        let List::Group(id) = list else {
            return None;
        };
        match self.find(&Unit::Group(*id))?.0 {
            List::Group(parent) => self.root(&List::Group(parent)),
            _ => Some(*id),
        }
    }

    /// Whether `list` is group `id` or inside it.
    fn within(&self, list: &List, id: u64) -> bool {
        match list {
            List::Group(g) if *g == id => true,
            List::Group(g) => self
                .find(&Unit::Group(*g))
                .is_some_and(|(up, _)| self.within(&up, id)),
            _ => false,
        }
    }

    /// Every page under group `id`, at every depth.
    pub fn pages(&self, id: u64) -> Vec<Route> {
        let mut out = Vec::new();
        if let Some(g) = self.get(id) {
            pages_in(&g.kids, &mut out);
        }
        out
    }

    /// The pages of `unit`: itself, or every one under it.
    fn pages_of(&self, unit: &Unit) -> Vec<Route> {
        match unit {
            Unit::Page(item) => vec![item.clone()],
            Unit::Group(id) => self.pages(*id),
        }
    }

    /// Every group, in the order of the tree, with how deep it is: a tab's under it, the Groups tab's.
    pub fn all(&self, top: &List) -> Vec<(usize, &Group)> {
        let mut out = Vec::new();
        if let Some(nodes) = self.list(top) {
            groups_in(nodes, 0, &mut out);
        }
        out
    }

    /// The rows of a top list as the menu draws them, a folded group's own left out.
    pub fn rows(&self, top: &List) -> Vec<(usize, Row<'_>)> {
        let mut out = Vec::new();
        if let Some(nodes) = self.list(top) {
            rows_in(nodes, 0, &mut out);
        }
        out
    }

    /// Every page in the tree's order: the tabs', then the Groups tab's.
    pub fn order(&self) -> Vec<Route> {
        let mut out = Vec::new();
        for (_, nodes) in self.tops() {
            pages_in(nodes, &mut out);
        }
        out
    }

    /// `unit` taken out of wherever it is.
    pub fn take(&mut self, unit: &Unit) -> Option<Node> {
        if let Some(node) = take_in(&mut self.shared, unit) {
            return Some(node);
        }
        self.tabs
            .iter_mut()
            .find_map(|(_, nodes)| take_in(nodes, unit))
    }

    /// Page `item` out of the tree; a group it leaves stays (`ended` ends it).
    pub fn leave(&mut self, item: &Route) {
        self.take(&Unit::Page(item.clone()));
    }

    /// `node` into `list`, before or after `anchor` there, else at its bottom. False when the list is not there.
    fn put(&mut self, list: &List, node: Node, anchor: Option<(&Unit, bool)>) -> bool {
        let Some(nodes) = self.list_mut(list) else {
            return false;
        };
        let at = anchor
            .and_then(|(unit, after)| {
                (nodes.iter().position(|n| is(n, unit))).map(|at| at + usize::from(after))
            })
            .unwrap_or(nodes.len());
        nodes.insert(at, node);
        true
    }

    /// The tree as the open pages are: one closed taken out, one not in it put at the bottom of its tab's list.
    /// `open` each page with its space's id, in the order they were opened.
    pub fn sync(&mut self, open: &[(Route, String)]) {
        let here = |item: &Route| open.iter().any(|(o, _)| o == item);
        retain_pages(&mut self.shared, &here);
        for (_, nodes) in &mut self.tabs {
            retain_pages(nodes, &here);
        }
        let known = self.order();
        for (item, tab) in open {
            if !known.contains(item) {
                self.put(&List::Tab(tab.clone()), Node::Page(item.clone()), None);
            }
        }
    }

    /// The groups that had a page in `before` and have none now, ended; their names.
    pub fn ended(&mut self, before: &Groups) -> Vec<String> {
        let had = |g: &Group| !before.pages(g.id).is_empty();
        let mut out = Vec::new();
        end_in(&mut self.shared, &had, &mut out);
        for (_, nodes) in &mut self.tabs {
            end_in(nodes, &had, &mut out);
        }
        out
    }

    fn fresh(&mut self, name: &str) -> Group {
        self.next += 1;
        Group {
            id: self.next,
            name: name.to_owned(),
            shut: false,
            kids: Vec::new(),
        }
    }

    /// A new group of `items`, at the bottom of `list`; its id.
    pub fn start(&mut self, list: &List, items: &[Route]) -> u64 {
        let mut group = self.fresh(NEW);
        for item in items {
            self.leave(item);
            group.kids.push(Node::Page(item.clone()));
        }
        let id = group.id;
        self.put(list, Node::Group(group), None);
        id
    }

    /// A group with nothing in it yet at the bottom of group `id` (New subgroup…); its id.
    pub fn subgroup(&mut self, id: u64) -> Option<u64> {
        let group = self.fresh(NEW);
        let new = group.id;
        self.put(&List::Group(id), Node::Group(group), None)
            .then_some(new)
    }

    /// Ungroup: what group `id` holds stands where it stood. Pages cannot stand at the Groups tab's top: those go
    /// to the bottom of their own tab's list.
    pub fn ungroup(&mut self, id: u64, tab_of: &dyn Fn(&Route) -> String) {
        let Some((list, at)) = self.find(&Unit::Group(id)) else {
            return;
        };
        let Some(Node::Group(group)) = self.take(&Unit::Group(id)) else {
            return;
        };
        let (pages, kept): (Vec<Node>, Vec<Node>) = match list {
            List::Groups => group
                .kids
                .into_iter()
                .partition(|n| matches!(n, Node::Page(_))),
            _ => (Vec::new(), group.kids),
        };
        if let Some(nodes) = self.list_mut(&list) {
            let at = at.min(nodes.len());
            nodes.splice(at..at, kept);
        }
        for page in pages {
            if let Node::Page(item) = &page {
                let tab = tab_of(item);
                self.put(&List::Tab(tab), page, None);
            }
        }
    }

    /// `unit` one place up or down its list; whether it moved.
    pub fn step(&mut self, unit: &Unit, up: bool) -> bool {
        let Some((list, at)) = self.find(unit) else {
            return false;
        };
        let Some(nodes) = self.list_mut(&list) else {
            return false;
        };
        let to = match up {
            true => at.checked_sub(1),
            false => (at + 1 < nodes.len()).then_some(at + 1),
        };
        match to {
            Some(to) => {
                nodes.swap(at, to);
                true
            }
            None => false,
        }
    }

    /// Whether `unit` may stand in a list under tab `tab`: a page of it, a group all of whose pages are.
    fn fits(&self, unit: &Unit, tab: &str, tab_of: &dyn Fn(&Route) -> String) -> bool {
        self.pages_of(unit).iter().all(|p| tab_of(p) == tab)
    }

    /// Whether `unit` may go into `list`, and the top-level group that goes to Groups first so that it can.
    fn admits(
        &self,
        unit: &Unit,
        list: &List,
        tab_of: &dyn Fn(&Route) -> String,
    ) -> Option<Option<u64>> {
        if let Unit::Group(id) = unit
            && self.within(list, *id)
        {
            return None;
        }
        match (self.home(list)?, list) {
            (List::Groups, List::Groups) => matches!(unit, Unit::Group(_)).then_some(None),
            (List::Groups, _) => Some(None),
            (List::Tab(tab), _) if self.fits(unit, &tab, tab_of) => Some(None),
            // Another tab's: into a group, its whole top-level group goes to Groups.
            (List::Tab(_), List::Group(_)) => Some(Some(self.root(list)?)),
            _ => None,
        }
    }

    /// What `from` let go on `to` does: on a row (at its edge or onto it), or at the bottom of a list (a tab's row,
    /// the Groups tab's row or the menu's end); none where it may not go.
    pub fn plan(&self, from: &Unit, to: Target, tab_of: &dyn Fn(&Route) -> String) -> Option<Plan> {
        let (list, act) = match to {
            Target::List(list) => {
                // Already at the bottom of that very list: nothing to do.
                if let Some((now, at)) = self.find(from)
                    && now == list
                    && self.list(&list).is_some_and(|n| at + 1 == n.len())
                {
                    return None;
                }
                (list.clone(), Move::Put { list, anchor: None })
            }
            Target::Row(target, at) => {
                if *from == target {
                    return None;
                }
                let (list, _) = self.find(&target)?;
                match (at, &target) {
                    (At::Onto, Unit::Group(id)) => (
                        List::Group(*id),
                        Move::Put {
                            list: List::Group(*id),
                            anchor: None,
                        },
                    ),
                    (At::Onto, Unit::Page(with)) => {
                        // A group dropped on a page makes nothing.
                        if matches!(from, Unit::Group(_)) || list == List::Groups {
                            return None;
                        }
                        let with = with.clone();
                        (list.clone(), Move::Pair { list, with })
                    }
                    (at, _) => (
                        list.clone(),
                        Move::Put {
                            list,
                            anchor: Some((target.clone(), at == At::After)),
                        },
                    ),
                }
            }
        };
        let lift = self.admits(from, &list, tab_of)?;
        // A pair of pages under a tab's top is a group of that tab's.
        if let Move::Pair { with, .. } = &act
            && let Some(List::Tab(tab)) = self.home(&list)
            && lift.is_none()
            && tab_of(with) != tab
        {
            return None;
        }
        let from_home = self.home(&self.find(from)?.0);
        let into_groups = lift.is_some()
            || (self.home(&list) == Some(List::Groups) && from_home != Some(List::Groups));
        let hint = match (&act, into_groups) {
            (_, true) => Some(TO_GROUPS),
            (Move::Pair { .. }, false) => Some(NEW_GROUP),
            _ => None,
        };
        Some(Plan { lift, act, hint })
    }

    /// `plan` carried out for `from`: the group lifted to Groups first, if any. The new group's id for a pair.
    pub fn apply(&mut self, from: &Unit, plan: &Plan) -> Option<u64> {
        if let Some(root) = plan.lift
            && let Some(node) = self.take(&Unit::Group(root))
        {
            self.put(&List::Groups, node, None);
        }
        match &plan.act {
            Move::Put { list, anchor } => {
                let node = self.take(from)?;
                self.put(list, node, anchor.as_ref().map(|(u, after)| (u, *after)));
                None
            }
            Move::Pair { list, with } => {
                let Unit::Page(item) = from else {
                    return None;
                };
                Some(self.start(list, &[with.clone(), item.clone()]))
            }
        }
    }

    /// As a column of `nav.txt`; none without a group.
    pub fn line(&self) -> Option<String> {
        if !self.any() {
            return None;
        }
        fn kept(nodes: &[Node]) -> Vec<KeptNode> {
            (nodes.iter())
                .map(|n| match n {
                    Node::Page(item) => KeptNode::Page(item.text()),
                    Node::Group(g) => KeptNode::Group {
                        name: g.name.clone(),
                        shut: g.shut,
                        kids: kept(&g.kids),
                    },
                })
                .collect()
        }
        let tree = KeptTree {
            tabs: (self.tabs.iter())
                .map(|(t, n)| (t.clone(), kept(n)))
                .collect(),
            shared: kept(&self.shared),
        };
        serde_json::to_string(&tree)
            .ok()
            .map(|json| format!("{TREE}{json}"))
    }

    /// `column` read back, of the pages in `open` alone (each with its tab's id); none when it is another column.
    /// A column an earlier build wrote (`groups=`, flat groups under their tab) is read into the tree.
    pub fn read(column: &str, open: &[(Route, String)]) -> Option<Groups> {
        let mut groups = Groups::default();
        if let Some(json) = column.strip_prefix(TREE) {
            let tree: KeptTree = serde_json::from_str(json).ok()?;
            for (tab, nodes) in tree.tabs {
                let nodes = groups.unkept(nodes, open);
                groups.tabs.push((tab, nodes));
            }
            groups.shared = (groups.unkept(tree.shared, open).into_iter())
                .filter(|n| matches!(n, Node::Group(_)))
                .collect();
        } else {
            let kept: Vec<OldKept> = serde_json::from_str(column.strip_prefix(OLD)?).ok()?;
            for (item, tab) in open {
                let old = kept.iter().find(|k| k.items.contains(&item.text()));
                match old {
                    // Its group stands already, with it.
                    Some(_) if groups.order().contains(item) => {}
                    Some(k) => {
                        let mut group = groups.fresh(&k.name);
                        group.shut = k.shut;
                        group.kids = (open.iter())
                            .filter(|(o, _)| k.items.contains(&o.text()))
                            .map(|(o, _)| Node::Page(o.clone()))
                            .collect();
                        groups.put(&List::Tab(tab.clone()), Node::Group(group), None);
                    }
                    None => {
                        groups.put(&List::Tab(tab.clone()), Node::Page(item.clone()), None);
                    }
                }
            }
        }
        groups.sync(open);
        Some(groups)
    }

    fn unkept(&mut self, nodes: Vec<KeptNode>, open: &[(Route, String)]) -> Vec<Node> {
        let mut out = Vec::new();
        for node in nodes {
            match node {
                KeptNode::Page(text) => {
                    // A page twice, or one not open again, is left out.
                    if let Some(item) = Route::parse(&text)
                        && open.iter().any(|(o, _)| *o == item)
                        && !self.order().contains(&item)
                        && !out.contains(&Node::Page(item.clone()))
                    {
                        out.push(Node::Page(item));
                    }
                }
                KeptNode::Group { name, shut, kids } => {
                    let mut group = self.fresh(&name);
                    group.shut = shut;
                    group.kids = self.unkept(kids, open);
                    out.push(Node::Group(group));
                }
            }
        }
        out
    }
}

/// Where a drop lands (`Groups::plan`): a row, or the bottom of a list (a tab's row, the Groups tab's).
pub(super) enum Target {
    Row(Unit, At),
    List(List),
}

const TREE: &str = "tree=";
const OLD: &str = "groups=";

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum KeptNode {
    Page(String),
    Group {
        name: String,
        #[serde(default)]
        shut: bool,
        #[serde(default)]
        kids: Vec<KeptNode>,
    },
}

#[derive(Serialize, Deserialize)]
struct KeptTree {
    tabs: Vec<(String, Vec<KeptNode>)>,
    #[serde(default)]
    shared: Vec<KeptNode>,
}

/// A group as an earlier build kept it: a name and its pages, under their tab.
#[derive(Deserialize)]
struct OldKept {
    name: String,
    #[serde(default)]
    shut: bool,
    items: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mr(n: u32) -> Route {
        Route {
            kind: "requests.request".into(),
            args: format!("{{\"number\":{n}}}"),
        }
    }

    fn issue(n: u32) -> Route {
        Route {
            kind: "tasks.issue".into(),
            args: format!("{{\"key\":\"PAY-{n}\"}}"),
        }
    }

    fn tab_of(item: &Route) -> String {
        match item.kind.as_str() {
            "tasks.issue" => "tasks".into(),
            _ => "requests".into(),
        }
    }

    fn open(items: &[Route]) -> Vec<(Route, String)> {
        items.iter().map(|i| (i.clone(), tab_of(i))).collect()
    }

    fn tree(items: &[Route]) -> Groups {
        let mut g = Groups::default();
        g.sync(&open(items));
        g
    }

    fn requests() -> List {
        List::Tab("requests".into())
    }

    fn drop(g: &mut Groups, from: Unit, to: Target) -> Option<Plan> {
        let plan = g.plan(&from, to, &tab_of)?;
        g.apply(&from, &plan);
        Some(plan)
    }

    #[test]
    fn a_page_dropped_on_another_makes_a_group_at_the_bottom_of_its_list() {
        let mut g = tree(&[mr(1), mr(2), mr(3)]);
        let plan = drop(
            &mut g,
            Unit::Page(mr(3)),
            Target::Row(Unit::Page(mr(1)), At::Onto),
        )
        .unwrap();
        assert_eq!(plan.hint, Some(NEW_GROUP));
        // The new group does not take the target's place: it is the last of the tab's list.
        let rows = g.list(&requests()).unwrap();
        assert_eq!(rows[0], Node::Page(mr(2)));
        let Node::Group(group) = &rows[1] else {
            panic!("a group last")
        };
        assert_eq!(group.kids, vec![Node::Page(mr(1)), Node::Page(mr(3))]);
        // A page opened later goes under it.
        let mut items = vec![mr(1), mr(2), mr(3), mr(4)];
        g.sync(&open(&items));
        assert_eq!(g.list(&requests()).unwrap()[2], Node::Page(mr(4)));
        // Closed, a page leaves the tree; the last one out ends the group.
        let before = g.clone();
        items.retain(|i| *i != mr(1) && *i != mr(3));
        g.sync(&open(&items));
        assert_eq!(g.ended(&before), vec![NEW.to_owned()]);
        assert!(!g.any());
    }

    #[test]
    fn groups_go_inside_groups_and_never_into_themselves() {
        let mut g = tree(&[mr(1), mr(2), mr(3), mr(4)]);
        let a = g.start(&requests(), &[mr(1), mr(2)]);
        let b = g.start(&requests(), &[mr(3)]);
        // A group dropped on a group: its subgroup, at its bottom.
        drop(
            &mut g,
            Unit::Group(b),
            Target::Row(Unit::Group(a), At::Onto),
        )
        .unwrap();
        assert_eq!(g.find(&Unit::Group(b)), Some((List::Group(a), 2)));
        assert_eq!(g.pages(a), vec![mr(1), mr(2), mr(3)]);
        // Never into itself or one inside it.
        assert!(
            g.plan(
                &Unit::Group(a),
                Target::Row(Unit::Group(b), At::Onto),
                &tab_of
            )
            .is_none()
        );
        assert!(
            g.plan(
                &Unit::Group(a),
                Target::Row(Unit::Page(mr(3)), At::Before),
                &tab_of
            )
            .is_none()
        );
        // A page dropped on a page inside a group: a subgroup of both, at the bottom of that group.
        let plan = drop(
            &mut g,
            Unit::Page(mr(4)),
            Target::Row(Unit::Page(mr(1)), At::Onto),
        )
        .unwrap();
        assert_eq!(plan.hint, Some(NEW_GROUP));
        let Some(Node::Group(pair)) = g.get(a).unwrap().kids.last() else {
            panic!("a subgroup last")
        };
        assert_eq!(pair.kids, vec![Node::Page(mr(1)), Node::Page(mr(4))]);
        // The rows, a folded group's own left out.
        g.get_mut(b).unwrap().shut = true;
        let depths: Vec<usize> = g.rows(&requests()).iter().map(|(d, _)| *d).collect();
        assert_eq!(depths, [0, 1, 1, 1, 2, 2]);
        // Ungroup lifts what it held a level.
        g.ungroup(a, &tab_of);
        assert_eq!(g.find(&Unit::Page(mr(2))), Some((requests(), 0)));
        assert_eq!(g.find(&Unit::Group(b)), Some((requests(), 1)));
    }

    #[test]
    fn another_tabs_page_takes_the_whole_group_to_groups() {
        let mut g = tree(&[mr(1), mr(2), issue(7)]);
        let rel = g.start(&requests(), &[mr(1)]);
        let refunds = g.subgroup(rel).unwrap();
        drop(
            &mut g,
            Unit::Page(mr(2)),
            Target::Row(Unit::Group(refunds), At::Onto),
        )
        .unwrap();
        // A Tasks issue dropped on a group under Merge requests: the top-level group goes to Groups with it.
        let plan = g
            .plan(
                &Unit::Page(issue(7)),
                Target::Row(Unit::Group(refunds), At::Onto),
                &tab_of,
            )
            .unwrap();
        assert_eq!((plan.lift, plan.hint), (Some(rel), Some(TO_GROUPS)));
        g.apply(&Unit::Page(issue(7)), &plan);
        assert_eq!(g.find(&Unit::Group(rel)), Some((List::Groups, 0)));
        assert_eq!(g.pages(refunds), vec![mr(2), issue(7)]);
        // Under a tab's top, another tab's page has no place.
        assert!(
            g.plan(&Unit::Page(issue(7)), Target::List(requests()), &tab_of)
                .is_none()
        );
        // Back onto Merge requests only while every page of it is that tab's.
        assert!(
            g.plan(&Unit::Group(rel), Target::List(requests()), &tab_of)
                .is_none()
        );
        drop(
            &mut g,
            Unit::Page(issue(7)),
            Target::List(List::Tab("tasks".into())),
        )
        .unwrap();
        drop(&mut g, Unit::Group(rel), Target::List(requests())).unwrap();
        assert!(g.shared.is_empty());
    }

    #[test]
    fn a_group_moved_to_groups_goes_to_its_bottom_and_pages_never_stand_there() {
        let mut g = tree(&[mr(1), mr(2), mr(3)]);
        let a = g.start(&requests(), &[mr(1)]);
        let b = g.start(&requests(), &[mr(2)]);
        let plan = drop(&mut g, Unit::Group(a), Target::List(List::Groups)).unwrap();
        assert_eq!(plan.hint, Some(TO_GROUPS));
        drop(&mut g, Unit::Group(b), Target::List(List::Groups)).unwrap();
        assert_eq!(g.find(&Unit::Group(b)), Some((List::Groups, 1)));
        // Already at the bottom there: nothing to do; a page has no place at Groups' top.
        assert!(
            g.plan(&Unit::Group(b), Target::List(List::Groups), &tab_of)
                .is_none()
        );
        assert!(
            g.plan(&Unit::Page(mr(3)), Target::List(List::Groups), &tab_of)
                .is_none()
        );
        assert!(
            g.plan(
                &Unit::Page(mr(3)),
                Target::Row(Unit::Group(a), At::Before),
                &tab_of
            )
            .is_none()
        );
        // Ordered by a drag at an edge, or a step.
        drop(
            &mut g,
            Unit::Group(b),
            Target::Row(Unit::Group(a), At::Before),
        )
        .unwrap();
        assert_eq!(g.find(&Unit::Group(b)), Some((List::Groups, 0)));
        assert!(g.step(&Unit::Group(b), false));
        assert_eq!(g.find(&Unit::Group(b)), Some((List::Groups, 1)));
        assert!(!g.step(&Unit::Group(b), false));
        // Ungrouped at Groups' top, its pages go home to their tab.
        g.ungroup(a, &tab_of);
        assert_eq!(g.find(&Unit::Page(mr(1))), Some((requests(), 1)));
    }

    #[test]
    fn the_tree_comes_back_from_its_line_and_an_earlier_builds_groups_too() {
        let items = [mr(1), mr(2), issue(3)];
        let mut g = tree(&items);
        let a = g.start(&requests(), &[mr(1)]);
        let sub = g.subgroup(a).unwrap();
        g.get_mut(a).unwrap().name = "Release 2.5".into();
        g.get_mut(sub).unwrap().shut = true;
        drop(&mut g, Unit::Group(a), Target::List(List::Groups)).unwrap();
        let line = g.line().unwrap();
        assert!(!line.contains('\t'));
        let back = Groups::read(&line, &open(&items)).unwrap();
        assert_eq!(back.order(), g.order());
        let (_, top) = back.all(&List::Groups)[0];
        assert_eq!(top.name, "Release 2.5");
        assert!(back.all(&List::Groups)[1].1.shut);
        // A page not open again is left out; the group stays.
        let fewer = Groups::read(&line, &open(&[mr(2)])).unwrap();
        assert_eq!(fewer.order(), vec![mr(2)]);
        assert!(fewer.any());
        // What an earlier build wrote: flat groups by their pages.
        let old = format!(
            r#"groups=[{{"name":"Old","shut":true,"items":["{}"]}}]"#,
            mr(2).text().replace('"', "\\\"")
        );
        let read = Groups::read(&old, &open(&items)).unwrap();
        let (list, _) = read.find(&Unit::Page(mr(2))).unwrap();
        assert!(matches!(list, List::Group(_)));
        assert!(Groups::read("code.file{}", &open(&items)).is_none());
        let key = Unit::Group(3).key();
        assert_eq!(Unit::read(&key), Some(Unit::Group(3)));
        assert_eq!(Unit::read(&mr(1).text()), Some(Unit::Page(mr(1))));
    }
}
