//! The token completed while writing a comment (#51 mentions, #9 `:name` emoji): `@` or `:` starting a word
//! before the caret, and a row of the list it opens. Who and which emoji the list holds is the tab's.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    Person,
    Emoji,
}

/// One row of the list: what it shows, beside it, and what it puts in the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub label: String,
    pub detail: String,
    pub insert: String,
}

/// The token being completed before `caret`: its trigger, where the trigger is, what follows it.
pub fn token(text: &str, caret: usize) -> Option<(Trigger, usize, String)> {
    let before = text.get(..caret)?;
    let word_start = before
        .char_indices()
        .rev()
        .find(|(_, c)| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '+')))
        .map_or(0, |(i, c)| i + c.len_utf8());
    let at = word_start.checked_sub(1)?;
    let trigger = match before.as_bytes().get(at)? {
        b'@' => Trigger::Person,
        b':' => Trigger::Emoji,
        _ => return None,
    };
    // The trigger starts a word: at the start, or after a space or an opening bracket.
    if at > 0
        && !matches!(
            before.as_bytes()[at - 1],
            b' ' | b'\n' | b'\t' | b'(' | b'['
        )
    {
        return None;
    }
    let query = before[word_start..].to_owned();
    if trigger == Trigger::Emoji && query.chars().count() < 2 {
        return None;
    }
    Some((trigger, at, query))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_read_back_from_the_caret() {
        let t = "thanks @jun";
        assert_eq!(token(t, t.len()), Some((Trigger::Person, 7, "jun".into())));
        let t = "nice :ta";
        assert_eq!(token(t, t.len()), Some((Trigger::Emoji, 5, "ta".into())));
        assert_eq!(token("nice :t", 7), None);
        assert_eq!(token("mail a@b", 8), None);
        assert_eq!(token("time 10:30", 10), None);
        assert_eq!(token("@", 1), Some((Trigger::Person, 0, String::new())));
    }
}
