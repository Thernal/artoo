//! The drafts of the Agents tab (#220, #243): what Send all at a session's foot says it sends; the drafts themselves
//! are a group of the session's discussions (`crate::threads`).

/// Send all's tooltip, from the number it sends.
pub fn send_all_tip(n: usize) -> String {
    format!("Send all {n} to Claude: your message first, then each comment as a message of its own")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::{self, Comment};
    use artoo_agents_api::comment::{self};
    use artoo_agents_api::conversation::{Entry, conversation};

    fn draft(id: u64, body: &str) -> Comment {
        Comment {
            id,
            path: "a/Login.kt".into(),
            line: 10 + id as u32,
            body: body.into(),
            ..Default::default()
        }
    }

    fn drafts_of(all: &[Comment]) -> Vec<&Comment> {
        crate::sessions::drafts_of(all, &crate::agents::Chat::default())
    }

    #[test]
    fn send_all_reads_back_your_words_first_then_each_comment() {
        let mut all = vec![draft(1, "a"), draft(2, "b"), draft(3, "c")];
        let ids: Vec<u64> = drafts_of(&all).iter().map(|c| c.id).collect();
        let mut kept = Vec::new();
        let mut m = review::next_message(&kept, "s1", "Take the login first.", 100).unwrap();
        m.carried = ids.clone();
        kept.push(m);
        comment::send(&mut all, &ids, "s1", "chat");
        for c in &mut all {
            c.sent.as_mut().unwrap().at = 100;
        }
        let talk = conversation(&all, &kept, &[], "s1");
        assert!(matches!(&talk[0], Entry::You { text, carried, .. }
            if text == "Take the login first." && carried == &ids));
        let order: Vec<u64> = talk
            .iter()
            .filter_map(|e| match e {
                Entry::Comment { id, .. } => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(order, [1, 2, 3], "each comment its own message, as written");
        assert_eq!(talk.len(), 4);
        assert!(drafts_of(&all).is_empty(), "nothing is left in Drafts");
        assert_eq!(
            artoo_agents_api::conversation::working_on(&all, "s1"),
            [1, 2, 3]
        );
    }

    #[test]
    fn the_tooltip_names_the_count() {
        assert_eq!(
            send_all_tip(10),
            "Send all 10 to Claude: your message first, then each comment as a message of its own"
        );
    }
}
