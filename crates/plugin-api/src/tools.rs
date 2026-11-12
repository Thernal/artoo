//! A plugin's own tools (#213): what an agent asks a plugin through Artoo's MCP server — the issue a note is about,
//! a search of the tracker. A plugin adds its tools as it installs; the window lists them where the MCP server reads
//! the app's windows (the agents' folder), and runs each call an agent makes, answering it there.
//!
//! A tool's name is the plugin's namespace, a dot and a name (`jira.issue`); the namespace is the first plugin's to
//! use it. An agent calls it with the dot as `_` (`jira_issue`): tool names take no dot. A plugin that stops as it
//! installs takes its tools with it (`drop_of`); one turned off is not loaded, so its tools are not there.

use gpui::{App, Global, Task};
use std::rc::Rc;

/// A tool's answer to its arguments (a JSON object, as text): text for the agent, or why not.
pub type Call = Rc<dyn Fn(String, &mut App) -> Task<Result<String, String>>>;

/// A tool a plugin gives agents.
#[derive(Clone)]
pub struct Tool {
    /// `jira.issue`: the plugin's namespace, a dot, a name.
    pub name: &'static str,
    /// What it answers, for the agent choosing a tool.
    pub description: &'static str,
    /// What it takes: a JSON schema of an object, as text.
    pub input: &'static str,
    pub call: Call,
}

/// A tool as the MCP server lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// As an agent calls it: `jira_issue`.
    pub name: String,
    pub description: String,
    pub input: String,
    /// The plugin it is of.
    pub plugin: String,
}

struct Entry {
    tool: Tool,
    owner: String,
}

#[derive(Default)]
struct Tools {
    list: Vec<Entry>,
    /// The plugin installing now, whose the tools added are.
    installing: Option<String>,
}

impl Global for Tools {}

/// Who adds tools from now on: plugin `id` as it installs (the loader's), the app's own after.
pub fn installing(cx: &mut App, id: Option<&str>) {
    cx.default_global::<Tools>().installing = id.map(str::to_owned);
}

/// Adds `tool`; why not when its name is no namespace and a name, or another plugin's namespace, or taken.
pub fn add(cx: &mut App, tool: Tool) -> Result<(), String> {
    let name = tool.name;
    let Some((space, _)) = name.split_once('.').filter(|_| named(name)) else {
        return Err(format!(
            "`{name}` is no tool's name: a namespace, a dot, a name (`jira.issue`)"
        ));
    };
    let tools = cx.default_global::<Tools>();
    let owner = tools.installing.clone().unwrap_or_else(|| "artoo".into());
    if let Some(other) = (tools.list.iter())
        .find(|e| e.tool.name.split_once('.').map(|(s, _)| s) == Some(space) && e.owner != owner)
    {
        return Err(format!(
            "the tools' namespace `{space}` is {}'s",
            other.owner
        ));
    }
    if tools
        .list
        .iter()
        .any(|e| mcp_name(e.tool.name) == mcp_name(name))
    {
        return Err(format!("the tool `{name}` is there already"));
    }
    tools.list.push(Entry { tool, owner });
    Ok(())
}

/// Whether `name` is a namespace, a dot and a name, of letters, digits, `_` and `-`.
fn named(name: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    name.split_once('.')
        .is_some_and(|(ns, rest)| part(ns) && part(rest))
}

/// Plugin `id`'s tools gone: it stopped as it installed.
pub fn drop_of(cx: &mut App, id: &str) {
    if let Some(tools) = cx.try_global::<Tools>()
        && tools.list.iter().any(|e| e.owner == id)
    {
        cx.global_mut::<Tools>().list.retain(|e| e.owner != id);
    }
}

/// The tools there now, as the MCP server lists them.
pub fn listed(cx: &App) -> Vec<Listed> {
    (cx.try_global::<Tools>().into_iter())
        .flat_map(|t| &t.list)
        .map(|e| Listed {
            name: mcp_name(e.tool.name),
            description: e.tool.description.to_owned(),
            input: e.tool.input.to_owned(),
            plugin: e.owner.clone(),
        })
        .collect()
}

/// Tool `name` (as an agent calls it) run on `args`; None when no tool has the name.
pub fn call(cx: &mut App, name: &str, args: String) -> Option<Task<Result<String, String>>> {
    let call = (cx.try_global::<Tools>())?
        .list
        .iter()
        .find(|e| mcp_name(e.tool.name) == name)
        .map(|e| e.tool.call.clone())?;
    Some(call(args, cx))
}

/// A tool's name as an agent calls it: `jira.issue` is `jira_issue`.
pub fn mcp_name(name: &str) -> String {
    name.replace('.', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &'static str, answer: &'static str) -> Tool {
        Tool {
            name,
            description: "Reads an issue",
            input: r#"{"type":"object","properties":{"key":{"type":"string"}}}"#,
            call: Rc::new(move |args, _| Task::ready(Ok(format!("{answer} {args}")))),
        }
    }

    #[gpui::test]
    fn a_plugins_tools_are_its_namespace(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            installing(cx, Some("artoo-tasks"));
            assert!(add(cx, tool("jira.issue", "issue")).is_ok());
            assert!(add(cx, tool("jira.search", "search")).is_ok());
            assert!(add(cx, tool("jira.issue", "again")).is_err());
            assert!(add(cx, tool("issue", "no namespace")).is_err());
            installing(cx, Some("other"));
            assert!(
                add(cx, tool("jira.board", "not theirs"))
                    .unwrap_err()
                    .contains("artoo-tasks")
            );
            assert!(add(cx, tool("trello.card", "theirs")).is_ok());
            installing(cx, None);
            let names: Vec<_> = listed(cx).into_iter().map(|l| (l.name, l.plugin)).collect();
            assert_eq!(
                names,
                [
                    ("jira_issue".to_owned(), "artoo-tasks".to_owned()),
                    ("jira_search".to_owned(), "artoo-tasks".to_owned()),
                    ("trello_card".to_owned(), "other".to_owned()),
                ]
            );
        });
    }

    #[gpui::test]
    async fn a_call_reaches_the_tool_and_a_plugin_that_stopped_has_none(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            installing(cx, Some("artoo-tasks"));
            add(cx, tool("jira.issue", "issue")).unwrap();
            installing(cx, None);
        });
        let task = cx.update(|cx| call(cx, "jira_issue", r#"{"key":"PAY-412"}"#.into()));
        assert_eq!(
            task.unwrap().await,
            Ok(r#"issue {"key":"PAY-412"}"#.to_owned())
        );
        cx.update(|cx| {
            assert!(call(cx, "jira_search", "{}".into()).is_none());
            drop_of(cx, "artoo-tasks");
            assert!(listed(cx).is_empty());
            assert!(call(cx, "jira_issue", "{}".into()).is_none());
        });
    }
}
