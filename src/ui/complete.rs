//! Completion while writing a comment (#51 mentions, #9 `:name` emoji): `@` and a letter lists
//! people — the request's first (its author, reviewers, who wrote in it), then the project's
//! members, one a mention does not reach greyed (not notified); `:` and two letters lists emoji by
//! name. ↑ ↓ move, ↵ or ⇥ puts it in, esc closes it.
//! No agents: they are messaged apart.

use super::Workspace;
pub(super) use artoo_ui_kit::complete::{Choice, Trigger, render_list, token};
use gpui::App;

impl Workspace {
    /// Who can be mentioned, the request's people first: (username, name).
    fn people(&self, cx: &App) -> Vec<(String, String)> {
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
        if let Some(m) = self
            .requests
            .read(cx)
            .current
            .and_then(|iid| self.requests.read(cx).find(iid))
        {
            add(&m.author_login, &m.author);
            for r in &m.reviewers {
                add(r, "");
            }
        }
        for t in &self.requests.read(cx).threads {
            for n in &t.notes {
                add(&n.login, &n.author);
            }
        }
        for (login, name) in &self.requests.read(cx).members {
            add(login, name);
        }
        // A name learnt later (a member) fills in one known only by login (a reviewer).
        let members = &self.requests.read(cx).members;
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
    pub(super) fn completion(
        &self,
        text: &str,
        caret: usize,
        closed: Option<usize>,
        cx: &App,
    ) -> Option<(usize, Vec<Choice>)> {
        let (trigger, at, query) = token(text, caret)?;
        if closed == Some(at) {
            return None;
        }
        let q = query.to_lowercase();
        let choices: Vec<Choice> = match trigger {
            Trigger::Person => {
                let mut people: Vec<(String, String)> = self
                    .people(cx)
                    .into_iter()
                    // By username or name, as typed or by the starts of its words ("ko" is not
                    // enough, "kok" finds Kestrel Okafor).
                    .filter(|(l, n)| {
                        crate::rank::word_matches(&q, l) || crate::rank::word_matches(&q, n)
                    })
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
                detail: if self.requests.read(cx).unreached.contains(&login) {
                    format!("@{login} · not notified")
                } else {
                    format!("@{login}")
                },
                insert: format!("@{login} "),
            })
            .collect(),
            Trigger::Emoji => crate::emoji::find(&q, 8)
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

    /// The list for the token at the caret of a plain field showing `text` (the new request's
    /// description, a review's summary), if one is open.
    pub(super) fn field_completion(&self, text: &str, cx: &App) -> Option<(usize, Vec<Choice>)> {
        // Until the field is edited the shared edit holds another text: the caret is at the end.
        let caret = if self.field_edit.text() == text {
            self.field_edit.caret()
        } else {
            text.len()
        };
        self.completion(text, caret, self.field_closed, cx)
    }

    /// ↑ ↓ ↵ ⇥ esc in a plain field while its list is open; whether it took the key. `text` is the
    /// field's text, changed when a choice goes in.
    pub(super) fn field_completion_key(
        &mut self,
        text: &mut String,
        key: &gpui::Keystroke,
        cx: &App,
    ) -> bool {
        let Some((at, choices)) = self.field_completion(text, cx) else {
            return false;
        };
        if key.modifiers.number_of_modifiers() > 0 {
            return false;
        }
        match key.key.as_str() {
            "down" => self.field_pick = (self.field_pick + 1).min(choices.len() - 1),
            "up" => self.field_pick = self.field_pick.saturating_sub(1),
            "enter" | "tab" => {
                super::input::sync(text, &mut self.field_edit);
                let choice = &choices[self.field_pick.min(choices.len() - 1)];
                let caret = self.field_edit.caret();
                let end = at + choice.insert.len();
                self.field_edit.replace(at..caret, &choice.insert, end..end);
                *text = self.field_edit.text().to_owned();
                self.field_pick = 0;
            }
            "escape" => self.field_closed = Some(at),
            _ => return false,
        }
        true
    }

    /// The username of the person called `name`, as the request and the project know them.
    pub(super) fn login_of(&self, name: &str, cx: &App) -> Option<String> {
        self.people(cx)
            .into_iter()
            .find(|(l, n)| n == name || l == name)
            .map(|(l, _)| l)
    }
}
