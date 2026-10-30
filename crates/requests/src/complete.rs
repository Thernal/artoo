//! Who and which emoji a token completes to (#51 mentions, #9 `:name` emoji): `@` and a letter lists people — the
//! request's first (its author, reviewers, who wrote in it), then the project's members, one a mention does not
//! reach greyed (not notified); `:` and two letters lists emoji by name. No agents: they are messaged apart.

use crate::requests::Requests;
use artoo_core::{emoji, rank};
use artoo_ui_kit::complete::{Choice, Trigger, token};

impl Requests {
    /// Who can be mentioned, the request's people first: (username, name).
    pub fn people(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        let mut add = |login: &str, name: &str| {
            if login.is_empty() {
                return;
            }
            match out.iter_mut().find(|(l, _)| l == login) {
                // Known by login only (a reviewer): a name seen later fills it in.
                Some((_, known)) if known.is_empty() => *known = name.to_owned(),
                Some(_) => {}
                None => out.push((login.to_owned(), name.to_owned())),
            }
        };
        if let Some(m) = self.current.and_then(|iid| self.find(iid)) {
            add(&m.author_login, &m.author);
            for r in &m.reviewers {
                add(r, "");
            }
        }
        for t in &self.threads {
            for n in &t.notes {
                add(&n.login, &n.author);
            }
        }
        for (login, name) in &self.members {
            add(login, name);
        }
        // A name learnt later (a member) fills in one known only by login (a reviewer).
        let members = &self.members;
        out.into_iter()
            .map(|(l, n)| {
                let name = if n.is_empty() {
                    members
                        .iter()
                        .find(|(m, _)| *m == l)
                        .map(|(_, n)| n.clone())
                        .unwrap_or_default()
                } else {
                    n
                };
                (l, name)
            })
            .collect()
    }

    /// The list for the token before `caret` in `text`, unless it was closed with esc at `closed`.
    pub fn completion(
        &self,
        text: &str,
        caret: usize,
        closed: Option<usize>,
    ) -> Option<(usize, Vec<Choice>)> {
        let (trigger, at, query) = token(text, caret)?;
        if closed == Some(at) {
            return None;
        }
        let q = query.to_lowercase();
        let choices: Vec<Choice> = match trigger {
            Trigger::Person => {
                let mut people: Vec<(String, String)> = self
                    .people()
                    .into_iter()
                    // By username or name, as typed or by the starts of its words ("ko" is not
                    // enough, "kok" finds Kestrel Okafor).
                    .filter(|(l, n)| rank::word_matches(&q, l) || rank::word_matches(&q, n))
                    .collect();
                // Who starts with what is typed comes first; otherwise the request's order stays.
                people.sort_by_key(|(l, n)| {
                    !(l.to_lowercase().starts_with(&q) || n.to_lowercase().starts_with(&q))
                });
                people
            }
            .into_iter()
            .take(8)
            .map(|(login, name)| Choice {
                label: if name.is_empty() { login.clone() } else { name },
                // Greyed: GitLab sends them nothing, they cannot read the request.
                detail: if self.unreached.contains(&login) {
                    format!("@{login} · not notified")
                } else {
                    format!("@{login}")
                },
                insert: format!("@{login} "),
            })
            .collect(),
            Trigger::Emoji => emoji::find(&q, 8)
                .into_iter()
                .map(|(name, emoji)| Choice {
                    label: format!("{emoji}  {name}"),
                    detail: String::new(),
                    insert: emoji.to_owned(),
                })
                .collect(),
        };
        (!choices.is_empty()).then_some((at, choices))
    }

    /// The username of the person called `name`, as the request and the project know them.
    pub fn login_of(&self, name: &str) -> Option<String> {
        self.people()
            .into_iter()
            .find(|(l, n)| n == name || l == name)
            .map(|(l, _)| l)
    }
}
