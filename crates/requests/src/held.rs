//! What a read of the open list does to the list on screen: a row's fields change in place, but a request that
//! came in, moved to another group or left the list is held — the list, the selection and the scroll stay as the
//! person has them until they ask (a click on the slot's count, `⌘R`, leaving the list). The kit's slot says
//! `1 new ↑` for what is held.

use artoo_core::mr::Mr;
use artoo_requests_api::Group;
use artoo_ui_kit::fresh::Held as Where;

/// The changes a read brought that the list does not show yet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Held {
    /// The list as the host last gave it: what the list becomes on apply.
    pub list: Vec<Mr>,
    /// Requests that came in, and those that moved to another group (they keep their old row until then).
    pub new: Vec<u64>,
    pub moved: Vec<u64>,
    /// Requests the host no longer lists (merged or closed there, or here): their rows stay until then.
    pub gone: Vec<u64>,
    /// Requests merged from here: their rows stay until then, and nothing is counted for them.
    pub kept: Vec<u64>,
    /// Where the first held change would land against the selected row.
    pub at: Option<Where>,
}

impl Held {
    /// How many changes wait.
    pub fn count(&self) -> usize {
        self.new.len() + self.moved.len() + self.gone.len()
    }

    /// What the slot counts: new ones first, else the moved, else the gone (counted as moved: they go on apply).
    pub fn kind(&self) -> Where {
        self.at.unwrap_or(Where::NewAbove)
    }
}

/// What `merge` gives: the rows to show now, and what is held.
pub struct Merged {
    pub shown: Vec<Mr>,
    pub held: Option<Held>,
}

/// `shown` is the list on screen, `read` the host's answer. `group_of` is the list's grouping and `picked` the
/// selected request, which decides whether a held change lands above or below it.
pub fn merge(
    shown: &[Mr],
    read: Vec<Mr>,
    group_of: impl Fn(&Mr) -> Group,
    merged_here: &[u64],
    picked: Option<u64>,
) -> Merged {
    // Nothing on screen yet: nothing to keep still, the list is the answer.
    if shown.is_empty() {
        return Merged {
            shown: read,
            held: None,
        };
    }
    let mut held = Held::default();
    let mut rows = Vec::with_capacity(shown.len());
    for old in shown {
        match read.iter().find(|m| m.iid == old.iid) {
            Some(new) if group_of(new) == group_of(old) => rows.push(new.clone()),
            Some(_) => {
                held.moved.push(old.iid);
                rows.push(old.clone());
            }
            None => {
                // A request that is not open any more but was kept (a merged or closed one) is not a change.
                if merged_here.contains(&old.iid) {
                    held.kept.push(old.iid);
                } else if old.state == artoo_core::mr::MrState::Opened {
                    held.gone.push(old.iid);
                }
                rows.push(old.clone());
            }
        }
    }
    for new in &read {
        if !shown.iter().any(|m| m.iid == new.iid) {
            held.new.push(new.iid);
        }
    }
    if held.count() == 0 && held.kept.is_empty() {
        return Merged {
            shown: rows,
            held: None,
        };
    }
    let key = |m: &Mr| (group_of(m), std::cmp::Reverse(m.updated));
    let anchor = picked
        .and_then(|iid| shown.iter().find(|m| m.iid == iid))
        .map(key);
    let first = held
        .new
        .iter()
        .chain(&held.moved)
        .filter_map(|iid| read.iter().find(|m| m.iid == *iid))
        .map(key)
        .min();
    held.at = Some(match (anchor, first) {
        (Some(a), Some(f)) if f > a => Where::NewBelow,
        _ if held.new.is_empty() && !held.moved.is_empty() => Where::MovedAbove,
        _ => Where::NewAbove,
    });
    held.list = read;
    Merged {
        shown: rows,
        held: Some(held),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use artoo_core::mr::MrState;

    pub(crate) fn mr(iid: u64, comments: usize, updated: i64) -> Mr {
        Mr {
            iid,
            title: format!("Request {iid}"),
            author: "Ana".into(),
            source: format!("feat/{iid}"),
            target: "main".into(),
            url: String::new(),
            updated,
            created: 0,
            head: String::new(),
            comments,
            draft: false,
            author_login: "ana".into(),
            reviewers: Vec::new(),
            reviewer_names: Vec::new(),
            description: String::new(),
            state: MrState::Opened,
            merged_with: None,
            merged_by: None,
            merged_at: 0,
            conflicts: false,
        }
    }

    /// Everyone's turn is someone else's; a request turned draft here moves to your turn.
    fn group(m: &Mr) -> Group {
        if m.draft { Group::You } else { Group::Others }
    }

    #[test]
    fn a_new_request_is_held_and_the_rows_stay() {
        let shown = vec![mr(1, 1, 10), mr(2, 5, 9)];
        let read = vec![mr(3, 0, 20), mr(1, 1, 10), mr(2, 5, 9)];
        let out = merge(&shown, read, group, &[], Some(2));
        let ids: Vec<u64> = out.shown.iter().map(|m| m.iid).collect();
        assert_eq!(ids, [1, 2]);
        let held = out.held.expect("held");
        assert_eq!(held.new, [3]);
        assert_eq!(held.count(), 1);
        assert_eq!(held.kind(), Where::NewAbove);
        assert_eq!(held.list.len(), 3);
    }

    #[test]
    fn a_field_changes_in_place() {
        let shown = vec![mr(1, 1, 10), mr(2, 5, 9)];
        let read = vec![mr(1, 2, 12), mr(2, 5, 9)];
        let out = merge(&shown, read, group, &[], None);
        assert_eq!(out.shown[0].comments, 2);
        assert_eq!(out.shown[0].updated, 12);
        assert!(out.held.is_none());
    }

    #[test]
    fn a_gone_request_stays_until_applied() {
        let shown = vec![mr(1, 1, 10), mr(2, 5, 9)];
        let out = merge(&shown, vec![mr(1, 1, 10)], group, &[], None);
        assert_eq!(out.shown.len(), 2);
        let held = out.held.expect("held");
        assert_eq!(held.gone, [2]);
        assert_eq!(held.list.len(), 1);
    }

    #[test]
    fn a_request_that_changes_group_keeps_its_old_row() {
        let shown = vec![mr(1, 1, 10), mr(2, 5, 9)];
        let mut moved = mr(2, 6, 30);
        moved.draft = true;
        let after = |m: &Mr| if m.draft { Group::Yours } else { Group::Others };
        let out = merge(&shown, vec![mr(1, 1, 10), moved], after, &[], Some(1));
        assert_eq!(out.shown[1].comments, 5, "the old row is kept");
        let held = out.held.expect("held");
        assert_eq!(held.moved, [2]);
        assert_eq!(held.kind(), Where::NewBelow, "Yours comes after Others");
    }

    #[test]
    fn a_moved_request_above_the_selection_says_moved() {
        let shown = vec![mr(1, 1, 10), mr(2, 5, 9)];
        let mut moved = mr(1, 1, 30);
        moved.draft = true;
        let out = merge(&shown, vec![moved, mr(2, 5, 9)], group, &[], Some(2));
        let held = out.held.expect("held");
        assert_eq!(held.moved, [1]);
        assert_eq!(held.kind(), Where::MovedAbove);
    }

    #[test]
    fn nothing_on_screen_takes_the_read_whole() {
        let out = merge(&[], vec![mr(1, 0, 1), mr(2, 0, 2)], group, &[], None);
        assert_eq!(out.shown.len(), 2);
        assert!(out.held.is_none());
    }

    #[test]
    fn a_request_merged_here_stays_without_a_count() {
        let shown = vec![mr(1, 1, 10), mr(2, 5, 9)];
        let out = merge(&shown, vec![mr(1, 1, 10)], group, &[2], None);
        assert_eq!(out.shown.len(), 2);
        let held = out.held.expect("kept for the next apply");
        assert_eq!((held.count(), held.kept.clone()), (0, vec![2]));
        assert_eq!(held.list.len(), 1);
    }

    #[test]
    fn a_kept_merged_row_is_not_a_change() {
        let mut merged = mr(2, 5, 9);
        merged.state = MrState::Merged;
        let shown = vec![mr(1, 1, 10), merged];
        let out = merge(&shown, vec![mr(1, 1, 10)], group, &[], None);
        assert!(out.held.is_none());
        assert_eq!(out.shown.len(), 2);
    }
}
