//! The kinds of place a note for an agent can be on (#212): the app's own, `line` (a file, a line, the code on it),
//! and those plugins declare — `jira.issue`, `jira.board` —, each saying how its place reads, what an agent gets as
//! a note's context, the tool that reads more of it (#213) and the page a click on the note opens.
//!
//! A plugin declares its kinds as it installs (`add`), and writes a note on its page with `on` and `note`, handed to
//! the tab's agents (`crate::tab::Agents::add_comment`) like one on a line.

use crate::comment::{Comment, LINE, On};
use artoo_core::route::Route;
use gpui::{App, Global};
use std::rc::Rc;

/// Words a plugin gives for a place, from its data.
pub type Words = Rc<dyn Fn(&str, &App) -> String>;

/// The page a place opens, from its data.
pub type Opens = Rc<dyn Fn(&str) -> Option<Route>>;

/// A kind of place a plugin declares.
#[derive(Clone)]
pub struct Kind {
    /// `jira.issue`: a namespace of the plugin's, a dot, a name.
    pub kind: &'static str,
    /// How a place of this kind reads, from its data: `PAY-412 · description · ¶4`.
    pub reads: Words,
    /// What an agent gets as a note's context, from its data: the paragraph, what the board lists.
    pub digest: Words,
    /// The tool that reads more of it (#213), by its name as the plugin declares it: `jira.issue`.
    pub tool: Option<&'static str>,
    /// The page a click on the note opens, from its data.
    pub route: Opens,
}

#[derive(Default)]
struct Kinds(Vec<Kind>);

impl Global for Kinds {}

/// Declares `kind`; why not when its name is no namespace and a name, is the app's own or is taken.
pub fn add(cx: &mut App, kind: Kind) -> Result<(), String> {
    let name = kind.kind;
    if !named(name) || name == LINE {
        return Err(format!(
            "`{name}` is no kind of place: a namespace, a dot, a name (`jira.issue`)"
        ));
    }
    let kinds = cx.default_global::<Kinds>();
    if kinds.0.iter().any(|k| k.kind == name) {
        return Err(format!("the kind of place `{name}` is declared already"));
    }
    kinds.0.push(kind);
    Ok(())
}

/// Whether `name` is a namespace, a dot and a name, of letters, digits, `_` and `-`.
pub fn named(name: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    name.split_once('.')
        .is_some_and(|(ns, rest)| part(ns) && rest.split('.').all(part))
}

/// The kind declared as `kind`.
pub fn get(cx: &App, kind: &str) -> Option<Kind> {
    cx.try_global::<Kinds>()?
        .0
        .iter()
        .find(|k| k.kind == kind)
        .cloned()
}

/// A note's place of kind `kind` at `data`, with the plugin's words for it now; None for a kind not declared.
pub fn on(cx: &App, kind: &str, data: &str) -> Option<On> {
    let k = get(cx, kind)?;
    Some(On {
        kind: kind.to_owned(),
        data: data.to_owned(),
        reads: (k.reads)(data, cx),
        digest: (k.digest)(data, cx),
        tool: k.tool.map(tool_name),
    })
}

/// A note for an agent at `on`, `body` what to do, to hand to the tab's agents as a draft.
pub fn note(on: On, body: String) -> Comment {
    Comment {
        at: on.reads.clone(),
        on: Some(on),
        body,
        ..Default::default()
    }
}

/// The page a click on a note at `on` opens; None while its plugin is off.
pub fn route(cx: &App, on: &On) -> Option<Route> {
    (get(cx, &on.kind)?.route)(&on.data)
}

/// A plugin's tool as an agent calls it: tool names take no dot, so `jira.issue` is `jira_issue`.
pub fn tool_name(name: &str) -> String {
    name.replace('.', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_is_a_namespace_and_a_name() {
        assert!(named("jira.issue"));
        assert!(named("my-plugin.board_view"));
        assert!(!named("line"));
        assert!(!named(".issue"));
        assert!(!named("jira."));
        assert!(!named("jira issue.x"));
    }

    #[test]
    fn a_note_on_a_place_reads_by_its_words() {
        let on = On {
            kind: "jira.issue".into(),
            data: r#"{"key":"PAY-412","para":4}"#.into(),
            reads: "PAY-412 · description · ¶4".into(),
            digest: "Done when the refund equals what was captured.".into(),
            tool: Some(tool_name("jira.issue")),
        };
        let c = note(on, "Work on this.".into());
        assert_eq!(c.kind(), "jira.issue");
        assert_eq!(c.reads(), "PAY-412 · description · ¶4");
        assert_eq!(c.at, "PAY-412 · description · ¶4");
        assert!(!c.is_folder());
        assert_eq!(c.on.as_ref().unwrap().tool.as_deref(), Some("jira_issue"));
        // Kept as JSON beside the notes on lines, and read back.
        let back: Comment = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn a_note_written_before_kinds_is_on_a_line() {
        let old = r#"{"id":3,"path":"src/Money.kt","old":false,"line":23,"code":"x","body":"b","at":"c41d7be"}"#;
        let c: Comment = serde_json::from_str(old).unwrap();
        assert_eq!(c.kind(), LINE);
        assert_eq!(c.reads(), "Money.kt:23");
        assert!(!serde_json::to_string(&c).unwrap().contains("\"on\""));
    }
}
