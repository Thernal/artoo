//! The functions and types a file's diff touches: a declaration is found by its keyword, not by a
//! grammar, so it works for the common languages and says nothing for the rest. Pure text in, a
//! list out.

use crate::git::{FileDiff, LineKind};

/// One declaration of a file and what the diff did to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    /// `+` the declaration is new, `~` its body changed, `−` it is gone.
    pub mark: char,
    /// `fn`, `struct`, `class`, `def` …
    pub kind: &'static str,
    pub name: String,
    /// 1-based, on the old side when `old`.
    pub line: u32,
    pub old: bool,
}

const KINDS: &[&str] = &[
    "fn",
    "struct",
    "enum",
    "trait",
    "impl",
    "mod",
    "class",
    "interface",
    "object",
    "def",
    "func",
    "function",
    "fun",
    "type",
    "protocol",
    "extension",
    "record",
];

/// Words that may stand before the keyword.
const MODIFIERS: &[&str] = &[
    "pub",
    "pub(crate)",
    "pub(super)",
    "async",
    "unsafe",
    "const",
    "static",
    "public",
    "private",
    "protected",
    "internal",
    "export",
    "default",
    "final",
    "abstract",
    "open",
    "override",
    "suspend",
    "data",
    "sealed",
    "inline",
    "extern",
    "@objc",
    "mutating",
];

/// The declaration on a line: its kind and name.
pub fn declaration(line: &str) -> Option<(&'static str, String)> {
    let mut words = line.split_whitespace().peekable();
    while words.peek().is_some_and(|w| MODIFIERS.contains(w)) {
        words.next();
    }
    let kind = *KINDS.iter().find(|k| Some(**k) == words.peek().copied())?;
    words.next();
    let name: String = words
        .next()?
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '<' && kind == "impl")
        .collect();
    let name = name.trim_end_matches('<').to_owned();
    (!name.is_empty()).then_some((kind, name))
}

/// The function (or other declaration) around 1-based line `line` of a file whose lines are
/// `line_of(1..=count)`: from its declaration to the line that closes it — the first line after it
/// indented no deeper, taken in when it only closes (`}`, `)`, `end`) at the same depth. `None` outside any, or for
/// one longer than `MOST` lines (a class, not a function).
pub fn enclosing<'a>(
    count: u32,
    line: u32,
    line_of: impl Fn(u32) -> &'a str,
) -> Option<(u32, u32)> {
    const MOST: u32 = 400;
    // The nearest declaration above that reaches the line: a line between two functions belongs
    // to what holds them.
    (line.saturating_sub(MOST).max(1)..=line.min(count))
        .rev()
        .filter(|&n| declaration(line_of(n)).is_some())
        .map(|start| (start, reach(count, start, MOST, &line_of)))
        .find(|&(_, end)| end >= line)
}

/// The last line of the declaration on line `start`: the line before the first one after it indented no deeper,
/// or that line when it only closes (`}`, `)`, `end`) at the same depth; at most `most` lines on.
fn reach<'a>(count: u32, start: u32, most: u32, line_of: &impl Fn(u32) -> &'a str) -> u32 {
    let indent = |text: &str| text.len() - text.trim_start().len();
    let depth = indent(line_of(start));
    let mut end = start;
    for n in start + 1..=count.min(start + most) {
        let text = line_of(n);
        if text.trim().is_empty() {
            continue;
        }
        if indent(text) <= depth {
            let t = text.trim_start();
            let closes =
                t.starts_with('}') || t.starts_with(')') || t.starts_with(']') || t == "end";
            // Its own closing line, not the one of what holds it.
            if closes && indent(text) == depth {
                end = n;
                // A closing line that opens again goes on: Kotlin's `) {` after a class's constructor, `} else {`.
                if t.trim_end().ends_with('{') {
                    continue;
                }
            }
            break;
        }
        end = n;
    }
    end
}

/// A declaration of a file with the lines it spans (1-based, both ends in): what the scope line over Code's file
/// names (#207).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    pub start: u32,
    pub end: u32,
    pub kind: &'static str,
    pub name: String,
}

/// Every declaration of `lines` with the lines it spans, a class's whole body too.
pub fn scopes(lines: &[String]) -> Vec<Scope> {
    const MOST: u32 = 20_000;
    let count = lines.len() as u32;
    let line_of = |n: u32| lines[n as usize - 1].as_str();
    (lines.iter().enumerate())
        .filter_map(|(i, text)| {
            let (kind, name) = declaration(text)?;
            let start = i as u32 + 1;
            Some(Scope {
                start,
                end: reach(count, start, MOST, &line_of),
                kind,
                name,
            })
        })
        .collect()
}

/// The declarations that hold line `line` and whose own line is above it, the outermost first.
pub fn scopes_at(scopes: &[Scope], line: u32) -> Vec<&Scope> {
    (scopes.iter())
        .filter(|s| s.start < line && line <= s.end)
        .collect()
}

/// The name around byte `at` of `line`: letters, digits and `_`.
pub fn word_at(line: &str, at: usize) -> Option<&str> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let at = at.min(line.len());
    let start = line[..at]
        .char_indices()
        .rev()
        .take_while(|(_, c)| word(*c))
        .last()
        .map_or(at, |(i, _)| i);
    let end = line[at..]
        .char_indices()
        .find(|(_, c)| !word(*c))
        .map_or(line.len(), |(i, _)| at + i);
    let name = &line[start..end];
    (!name.is_empty() && !name.starts_with(|c: char| c.is_ascii_digit())).then_some(name)
}

/// Where a name is declared (#207): a file, its line, the keyword and the line's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declared {
    pub path: String,
    pub line: u32,
    pub kind: &'static str,
    pub text: String,
}

/// The repository's declarations by name, read by keyword from its files: going to a name with no language server.
/// Built once, at the first ask; Java's methods, which no keyword declares, are not in it.
#[derive(Default, Debug)]
pub struct Names {
    by: std::collections::HashMap<String, Vec<Declared>>,
    /// The files read and their lines, what building it cost.
    pub files: usize,
    pub lines: usize,
}

/// Files whose words declare nothing: documents and data.
const NOT_CODE: &[&str] = &[
    "md", "markdown", "txt", "json", "lock", "toml", "yaml", "yml", "svg", "html", "css", "csv",
    "xml",
];

impl Names {
    /// The declarations in `lines`, the file at `path`; none for a document or data.
    pub fn add<'a>(&mut self, path: &str, lines: impl IntoIterator<Item = &'a str>) {
        let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
        if ext.as_deref().is_some_and(|e| NOT_CODE.contains(&e)) {
            return;
        }
        self.files += 1;
        for (i, line) in lines.into_iter().enumerate() {
            self.lines += 1;
            let Some((kind, name)) = declaration(line) else {
                continue;
            };
            let text: String = line.trim().chars().take(140).collect();
            self.by.entry(name).or_default().push(Declared {
                path: path.to_owned(),
                line: i as u32 + 1,
                kind,
                text,
            });
        }
    }

    /// The declarations of `name`, those in `here` (the file read) first, then by path and line.
    pub fn of(&self, name: &str, here: &str) -> Vec<Declared> {
        let mut found = self.by.get(name).cloned().unwrap_or_default();
        found.sort_by(|a, b| {
            (b.path == here)
                .cmp(&(a.path == here))
                .then_with(|| a.path.cmp(&b.path))
                .then(a.line.cmp(&b.line))
        });
        found
    }

    /// How many names it holds.
    pub fn len(&self) -> usize {
        self.by.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by.is_empty()
    }
}

/// The symbols of `file` the diff touches, in file order, removed ones last.
pub fn changed(file: &FileDiff) -> Vec<Symbol> {
    let new_text = file.new_text.as_deref().unwrap_or_default();
    let old_text = file.old_text.as_deref().unwrap_or_default();
    let added: std::collections::HashSet<u32> = file
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|l| l.kind == LineKind::Added)
        .filter_map(|l| l.new_line)
        .collect();

    // The declarations of the new text, and for each the lines up to the next one.
    let decls: Vec<(u32, &'static str, String)> = new_text
        .lines()
        .enumerate()
        .filter_map(|(i, l)| declaration(l).map(|(k, n)| (i as u32 + 1, k, n)))
        .collect();
    let total = new_text.lines().count() as u32;
    let mut out = Vec::new();
    for (i, (line, kind, name)) in decls.iter().enumerate() {
        let end = decls.get(i + 1).map_or(total + 1, |d| d.0);
        let mark = if added.contains(line) {
            '+'
        } else if (*line..end).any(|n| added.contains(&n)) {
            '~'
        } else {
            continue;
        };
        out.push(Symbol {
            mark,
            kind,
            name: name.clone(),
            line: *line,
            old: false,
        });
    }

    // A declaration on a removed line.
    let new_names: std::collections::HashSet<&str> =
        decls.iter().map(|(_, _, n)| n.as_str()).collect();
    let removed: std::collections::HashSet<u32> = file
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|l| l.kind == LineKind::Removed)
        .filter_map(|l| l.old_line)
        .collect();
    for (i, text) in old_text.lines().enumerate() {
        let line = i as u32 + 1;
        if !removed.contains(&line) {
            continue;
        }
        if let Some((kind, name)) = declaration(text)
            && !new_names.contains(name.as_str())
        {
            out.push(Symbol {
                mark: '−',
                kind,
                name,
                line,
                old: true,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_function_around_a_line_ends_where_it_closes() {
        let text = "class A {\n    fun one() {\n        a()\n\n        b()\n    }\n\n    fun two() = c()\n}\n";
        let lines: Vec<&str> = text.lines().collect();
        let at = |n: u32| lines[n as usize - 1];
        let count = lines.len() as u32;
        assert_eq!(enclosing(count, 5, at), Some((2, 6)));
        assert_eq!(enclosing(count, 2, at), Some((2, 6)));
        assert_eq!(enclosing(count, 8, at), Some((8, 8)));
        // Between the two, the class is what holds it.
        assert_eq!(enclosing(count, 7, at), Some((1, 9)));
        let py = ["def f():", "    x = 1", "    return x", "", "y = 2"];
        assert_eq!(enclosing(5, 3, |n| py[n as usize - 1]), Some((1, 3)));
        assert_eq!(enclosing(5, 5, |n| py[n as usize - 1]), None);
    }

    #[test]
    fn a_files_scopes_hold_their_lines_the_outermost_first() {
        let text = "class Login {\n    fun submit() {\n        go()\n    }\n\n    fun retry() = submit()\n}\nfun top() {\n}\n";
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let all = scopes(&lines);
        let names = |line: u32| {
            (scopes_at(&all, line).into_iter())
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(3), ["Login", "submit"]);
        assert_eq!(names(5), ["Login"]);
        // Its own line is on screen: nothing to name over it.
        assert_eq!(names(1), Vec::<&str>::new());
        assert_eq!(names(9), ["top"]);
        // A constructor over several lines: the class goes on past its `) {`.
        let kt: Vec<String> = [
            "class Vm(",
            "    val a: A,",
            ") {",
            "    fun go() {",
            "        x()",
            "    }",
            "}",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let all = scopes(&kt);
        let held: Vec<&str> = scopes_at(&all, 5).iter().map(|s| s.name.as_str()).collect();
        assert_eq!(held, ["Vm", "go"]);
    }

    #[test]
    fn a_name_is_found_where_it_is_declared_the_file_read_first() {
        let mut names = Names::default();
        names.add("b/State.kt", ["sealed interface LoginState {", "}"]);
        names.add("a/Other.kt", ["class LoginState"]);
        names.add("README.md", ["class LoginState"]);
        names.add("a/Vm.kt", ["class Vm {", "    val s: LoginState", "}"]);
        let found = names.of("LoginState", "b/State.kt");
        let at: Vec<(&str, u32)> = found.iter().map(|d| (d.path.as_str(), d.line)).collect();
        assert_eq!(at, [("b/State.kt", 1), ("a/Other.kt", 1)]);
        assert_eq!(found[0].text, "sealed interface LoginState {");
        assert!(names.of("s", "a/Vm.kt").is_empty());
        assert_eq!((names.len(), names.lines), (2, 6));
    }

    #[test]
    fn the_word_at_a_place_is_a_whole_name() {
        let line = "    val s: LoginState = load(id_2)";
        assert_eq!(word_at(line, 14), Some("LoginState"));
        assert_eq!(word_at(line, 11), Some("LoginState"));
        assert_eq!(word_at(line, 21), Some("LoginState"));
        assert_eq!(word_at(line, 30), Some("id_2"));
        assert_eq!(word_at(line, 2), None);
        assert_eq!(word_at("x 42", 3), None);
    }

    #[test]
    fn declarations_are_read_by_keyword() {
        assert_eq!(
            declaration("pub struct NewRequest {"),
            Some(("struct", "NewRequest".into()))
        );
        assert_eq!(
            declaration("    pub(super) async fn open_create(&mut self) {"),
            Some(("fn", "open_create".into()))
        );
        assert_eq!(
            declaration("class Foo : Bar() {"),
            Some(("class", "Foo".into()))
        );
        assert_eq!(declaration("def retry(n):"), Some(("def", "retry".into())));
        assert_eq!(declaration("let fn_count = 1;"), None);
        assert_eq!(declaration("// fn commented()"), None);
    }

    #[test]
    fn an_impl_is_named_by_its_type() {
        assert_eq!(
            declaration("impl Workspace {"),
            Some(("impl", "Workspace".into()))
        );
    }

    fn file(old: &str, new: &str, added: &[u32], removed: &[u32]) -> FileDiff {
        use crate::git::{ChangeKind, DiffLine, Hunk};
        let lines = added
            .iter()
            .map(|&n| DiffLine {
                kind: LineKind::Added,
                old_line: None,
                new_line: Some(n),
            })
            .chain(removed.iter().map(|&n| DiffLine {
                kind: LineKind::Removed,
                old_line: Some(n),
                new_line: None,
            }))
            .collect();
        FileDiff {
            old_path: Some("a.rs".into()),
            new_path: Some("a.rs".into()),
            change: ChangeKind::Modified,
            added: added.len(),
            removed: removed.len(),
            hunks: vec![Hunk { lines }],
            old_text: Some(old.into()),
            new_text: Some(new.into()),
            note: None,
            old_image: None,
            new_image: None,
            pictures: Vec::new(),
        }
    }

    #[test]
    fn changed_symbols_are_new_edited_or_gone() {
        let old = "fn keep() {
    1
}
fn edit() {
    1
}
fn gone() {
}
";
        let new = "fn keep() {
    1
}
fn edit() {
    2
}
fn fresh() {
}
";
        let symbols = changed(&file(old, new, &[5, 7, 8], &[5, 7, 8]));
        let marks: Vec<(char, &str)> = symbols.iter().map(|s| (s.mark, s.name.as_str())).collect();
        assert_eq!(marks, [('~', "edit"), ('+', "fresh"), ('−', "gone")]);
    }
}
