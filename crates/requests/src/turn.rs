//! Whose turn a request is (#189, Design L2), from what happened on it: a review asked for, a comment, a push, a
//! reply, an approval. A push or an author's reply gives the turn back to the reviewer; a comment hands it to the
//! author; approving ends it. Handing the turn over by hand waits.

use crate::group::Group;
use artoo_hosts_api::{Approvals, Mr, Thread};

/// Whose turn a request is, seen by `me`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Turn {
    /// Yours: asked to review and not answered, or answered since you last wrote — `waiting` the threads that wait on
    /// you (an author's replies to your comments; on one of yours, what reviewers wrote).
    You { waiting: usize },
    /// Someone else's: who (a first name), and what they owe.
    Them { who: String, owes: Owes },
    /// You approved it.
    Approved,
}

/// What the one whose turn it is owes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owes {
    /// Their review.
    Review,
    /// Answers to threads.
    Threads(usize),
    /// Nothing named: the next step is theirs.
    Step,
}

impl Turn {
    /// How a row ends: `Waiting on you · 1 reply`, `Waiting on Juniper · 1 thread`, `You approved`. `mine`: the
    /// request is yours, so what waits on you are threads, not replies.
    pub fn words(&self, mine: bool) -> String {
        match self {
            Turn::You { waiting: 0 } => "Waiting on you".to_owned(),
            Turn::You { waiting } if mine => {
                format!("Waiting on you · {}", plural(*waiting, "thread"))
            }
            Turn::You { waiting } => format!("Waiting on you · {}", plural(*waiting, "reply")),
            Turn::Them {
                who,
                owes: Owes::Review,
            } => format!("Waiting on {who} · review"),
            Turn::Them {
                who,
                owes: Owes::Threads(n),
            } => format!("Waiting on {who} · {}", plural(*n, "thread")),
            Turn::Them {
                who,
                owes: Owes::Step,
            } => format!("Waiting on {who}"),
            Turn::Approved => "You approved".to_owned(),
        }
    }

    /// The group a request of whose turn this is goes under: yours first of all.
    pub fn group(&self, mine: bool) -> Group {
        match self {
            _ if mine => Group::Yours,
            Turn::You { .. } => Group::You,
            Turn::Them { .. } => Group::Others,
            Turn::Approved => Group::Done,
        }
    }
}

fn plural(n: usize, word: &str) -> String {
    match (n, word) {
        (1, _) => format!("1 {word}"),
        (_, "reply") => format!("{n} replies"),
        _ => format!("{n} {word}s"),
    }
}

/// A first name: `Juniper` of `Juniper Hale`, a login as it is when no name is known.
fn first(name: &str) -> String {
    name.split_whitespace().next().unwrap_or(name).to_owned()
}

/// A login's name as the request's notes give it; the login capitalised when nobody of that login wrote.
fn name_of(login: &str, threads: &[Thread]) -> String {
    threads
        .iter()
        .flat_map(|t| &t.notes)
        .find(|n| n.login == login && !n.author.is_empty())
        .map(|n| first(&n.author))
        .unwrap_or_else(|| {
            let mut c = login.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        })
}

/// Of `threads`, those waiting on `who`: the last word someone else's, and — a discussion — still open, or — a plain
/// comment, which cannot be resolved — written after anything `who` wrote on the request.
pub fn waiting_on<'a>(who: &str, threads: &'a [Thread]) -> Vec<&'a Thread> {
    if who.is_empty() {
        return Vec::new();
    }
    let last_of_who = threads
        .iter()
        .flat_map(|t| &t.notes)
        .filter(|n| n.login == who)
        .map(|n| n.created)
        .max()
        .unwrap_or(0);
    threads
        .iter()
        .filter(|t| match t.notes.last() {
            Some(last) if last.login != who => {
                if t.resolvable {
                    !t.resolved
                } else {
                    last.created > last_of_who
                }
            }
            _ => false,
        })
        .collect()
}

/// Whose turn request `m` is, seen by `me`: from its approvals, its `threads` and its `pushes` (when each was).
pub fn turn(
    me: &str,
    m: &Mr,
    approvals: Option<&Approvals>,
    threads: &[Thread],
    pushes: &[i64],
) -> Turn {
    let author = first(&m.author);
    // A reviewer still to approve: the first asked who has not.
    let reviewer = || {
        m.reviewers
            .iter()
            .filter(|login| login.as_str() != me && **login != m.author_login)
            .map(|login| name_of(login, threads))
            .find(|name| approvals.is_none_or(|a| !a.by.iter().any(|by| first(by) == *name)))
    };
    if !me.is_empty() && m.author_login == me {
        // Yours: what reviewers wrote waits on you; else whoever is still to review; approved enough, the merge.
        let waiting = waiting_on(me, threads).len();
        if waiting > 0 {
            return Turn::You { waiting };
        }
        return match reviewer() {
            Some(who) if approvals.is_none_or(|a| a.left > 0 || a.by.is_empty()) => Turn::Them {
                who,
                owes: Owes::Review,
            },
            _ => Turn::You { waiting: 0 },
        };
    }
    // Approved: done, unless a push came after it (when the host says when) — the turn is yours again (#189).
    let last_push = pushes.iter().copied().max().unwrap_or(0);
    let approved_at = match approvals.filter(|a| a.mine) {
        Some(a) if a.mine_at.is_none_or(|at| last_push <= at) => return Turn::Approved,
        Some(a) => a.mine_at,
        None => None,
    };
    let my_last = threads
        .iter()
        .flat_map(|t| &t.notes)
        .filter(|n| !me.is_empty() && n.login == me)
        .map(|n| n.created)
        .chain(approved_at)
        .max();
    let asked = !me.is_empty() && m.reviewers.iter().any(|r| r == me);
    let Some(my_last) = my_last else {
        // Nothing written yet: yours when you are asked; else the author's threads, or a reviewer's review.
        if asked {
            return Turn::You { waiting: 0 };
        }
        let on_author = waiting_on(&m.author_login, threads).len();
        return match (on_author, reviewer()) {
            (n, _) if n > 0 => Turn::Them {
                who: author,
                owes: Owes::Threads(n),
            },
            (_, Some(who)) => Turn::Them {
                who,
                owes: Owes::Review,
            },
            _ => Turn::Them {
                who: author,
                owes: Owes::Step,
            },
        };
    };
    // The threads you wrote in whose last word came after yours: replies waiting on you.
    let replies = threads
        .iter()
        .filter(|t| t.notes.iter().any(|n| n.login == me))
        .filter(|t| {
            t.notes.last().is_some_and(|last| {
                last.login != me
                    && last.created
                        > t.notes
                            .iter()
                            .filter(|n| n.login == me)
                            .map(|n| n.created)
                            .max()
                            .unwrap_or(0)
            })
        })
        .count();
    let pushed = last_push > my_last;
    if pushed || replies > 0 {
        return Turn::You { waiting: replies };
    }
    // You wrote last: the author's turn, the threads you left open waiting on them.
    let open = threads
        .iter()
        .filter(|t| t.notes.last().is_some_and(|n| n.login == me))
        .filter(|t| !t.resolvable || !t.resolved)
        .count();
    Turn::Them {
        who: author,
        owes: if open > 0 {
            Owes::Threads(open)
        } else {
            Owes::Step
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_hosts_api::Note;

    fn request(author: &str) -> Mr {
        Mr {
            iid: 398,
            author: if author == "you" {
                "You".into()
            } else {
                "Juniper Hale".into()
            },
            author_login: author.into(),
            reviewers: vec!["you".into(), "ember".into()],
            ..Default::default()
        }
    }

    fn note(login: &str, at: i64) -> Note {
        Note {
            id: at as u64,
            author: match login {
                "you" => "You",
                "juniper" => "Juniper Hale",
                _ => "Ember Lind",
            }
            .into(),
            login: login.into(),
            body: String::new(),
            created: at,
            edited: false,
        }
    }

    fn thread(notes: Vec<Note>) -> Thread {
        Thread {
            id: "d".into(),
            notes,
            path: Some("a.kt".into()),
            new_line: Some(1),
            old_line: None,
            resolved: false,
            resolvable: true,
            resolved_by: None,
            resolved_at: 0,
            commit: None,
            outdated: false,
        }
    }

    #[test]
    fn the_turn_follows_what_happened_on_the_request() {
        let m = request("juniper");
        // A review asked of you, nothing written yet: yours.
        assert_eq!(turn("you", &m, None, &[], &[100]), Turn::You { waiting: 0 });
        // You commented: the author's turn, the thread waiting on them.
        let commented = [thread(vec![note("you", 200)])];
        assert_eq!(
            turn("you", &m, None, &commented, &[100]),
            Turn::Them {
                who: "Juniper".into(),
                owes: Owes::Threads(1)
            }
        );
        // The author replied: yours again, one reply waiting.
        let replied = [thread(vec![note("you", 200), note("juniper", 300)])];
        assert_eq!(
            turn("you", &m, None, &replied, &[100]),
            Turn::You { waiting: 1 }
        );
        // A push after your comment gives the turn back to you.
        assert_eq!(
            turn("you", &m, None, &commented, &[100, 400]),
            Turn::You { waiting: 0 }
        );
        // Approved: done.
        let approved = Approvals {
            by: vec!["You".into()],
            required: 2,
            left: 1,
            mine: true,
            mine_at: Some(300),
            can: true,
        };
        assert_eq!(
            turn("you", &m, Some(&approved), &replied, &[100]),
            Turn::Approved
        );
        // A push after your approval: yours to look at again.
        assert_eq!(
            turn("you", &m, Some(&approved), &[], &[100, 400]),
            Turn::You { waiting: 0 }
        );
        // The host does not say when: approved stays done.
        let unknown = Approvals {
            mine_at: None,
            ..approved.clone()
        };
        assert_eq!(
            turn("you", &m, Some(&unknown), &[], &[100, 400]),
            Turn::Approved
        );
        assert_eq!(Turn::Approved.group(false), Group::Done);
        assert_eq!(Turn::Approved.words(false), "You approved");
    }

    #[test]
    fn a_request_of_yours_waits_on_its_reviewers_or_on_you() {
        let m = request("you");
        // Nobody wrote: the first reviewer still to approve owes a review.
        let turn_now = turn("you", &m, None, &[], &[100]);
        assert_eq!(
            turn_now,
            Turn::Them {
                who: "Ember".into(),
                owes: Owes::Review
            }
        );
        assert_eq!(turn_now.words(true), "Waiting on Ember · review");
        assert_eq!(turn_now.group(true), Group::Yours);
        // A reviewer's thread waits on you.
        let asked = [thread(vec![note("ember", 200)])];
        let yours = turn("you", &m, None, &asked, &[100]);
        assert_eq!(yours, Turn::You { waiting: 1 });
        assert_eq!(yours.words(true), "Waiting on you · 1 thread");
    }

    #[test]
    fn a_request_not_asked_of_you_waits_on_whoever_owes_the_next_step() {
        let mut m = request("juniper");
        m.reviewers = vec!["ember".into()];
        assert_eq!(
            turn("you", &m, None, &[], &[100]),
            Turn::Them {
                who: "Ember".into(),
                owes: Owes::Review
            }
        );
        let asked = [thread(vec![note("ember", 200)])];
        let on_author = turn("you", &m, None, &asked, &[100]);
        assert_eq!(on_author.words(false), "Waiting on Juniper · 1 thread");
        assert_eq!(on_author.group(false), Group::Others);
    }
}
