//! What Artoo remembers between runs, as small text files in its application support directory:
//! recently opened repositories, pane sizes and how diffs are shown.

use std::path::{Path, PathBuf};

const RECENT_LIMIT: usize = 10;

fn file(name: &str) -> Option<PathBuf> {
    Some(crate::platform::data_dir()?.join(name))
}

/// A snapshot run: it draws a state, which must not become the user's remembered state or files.
pub fn snapshot_run() -> bool {
    cfg!(feature = "snapshot") && std::env::var_os("ARTOO_SNAPSHOT").is_some()
}

/// Writes `body` to `name`; failures only cost what would have been remembered.
pub fn save(name: &str, body: String) {
    if snapshot_run() {
        return;
    }
    // Nor may a test.
    if crate::platform::sandboxed() {
        return;
    }
    if let Some(file) = file(name)
        && let Some(dir) = file.parent()
        && std::fs::create_dir_all(dir).is_ok()
    {
        let _ = std::fs::write(file, body);
    }
}

pub fn read(name: &str) -> String {
    file(name)
        .and_then(|f| std::fs::read_to_string(f).ok())
        .unwrap_or_default()
}

/// How the side panel's sections were left folded, each kept by the feature whose section it is.
pub const PANEL: &str = "panel.txt";

/// The tab's panes (`Layout`); it kept the side panel's folds before `PANEL` did.
pub const LAYOUT: &str = "layout.txt";

/// Value `key` of `name`'s `key=value` lines. One `name` does not have yet is taken from `was`, the file that
/// kept it before, and kept in `name` from now on.
pub fn value(name: &str, key: &str, was: &str) -> Option<String> {
    let found = |text: &str| {
        pairs(text)
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.to_owned())
    };
    if let Some(value) = found(&read(name)) {
        return Some(value);
    }
    let value = found(&read(was))?;
    set_value(name, key, &value);
    Some(value)
}

/// `key` set to `value` among `name`'s `key=value` lines, the others kept.
pub fn set_value(name: &str, key: &str, value: &str) {
    save(name, with_value(&read(name), key, value));
}

fn with_value(text: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| l.split_once('=').is_none_or(|(k, _)| k.trim() != key))
        .map(str::to_owned)
        .collect();
    lines.push(format!("{key}={value}"));
    lines.join("\n") + "\n"
}

/// Recently opened repositories that still exist, newest first.
pub fn recent() -> Vec<PathBuf> {
    read("recent.txt")
        .lines()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .collect()
}

/// The tabs open when the window last changed, and which one was in front.
pub fn session() -> (Vec<PathBuf>, usize) {
    let text = read("session.txt");
    let mut lines = text.lines();
    let active = lines.next().and_then(|l| l.parse().ok()).unwrap_or(0);
    let tabs: Vec<PathBuf> = lines.map(PathBuf::from).filter(|p| p.is_dir()).collect();
    (tabs, active)
}

/// Keeps the open tabs (repository roots, in order) and the one in front.
pub fn save_session(tabs: &[PathBuf], active: usize) {
    let mut body = format!("{active}\n");
    for t in tabs {
        body.push_str(&format!("{}\n", t.display()));
    }
    save("session.txt", body);
}

/// Moves `path` to the front of the recent list and saves it.
pub fn remember(path: &Path) -> Vec<PathBuf> {
    let mut list = recent();
    list.retain(|p| p != path);
    list.insert(0, path.to_path_buf());
    list.truncate(RECENT_LIMIT);
    let body: Vec<String> = list.iter().map(|p| p.display().to_string()).collect();
    save("recent.txt", body.join("\n") + "\n");
    list
}

/// Whether background fetch is on for this repository: on unless turned off for it
/// (`fetch-off.txt`). Until 1.0.1 it was off until turned on (`fetch.txt`, no longer read).
pub fn fetch_enabled(root: &Path) -> bool {
    let want = root.display().to_string();
    !read("fetch-off.txt").lines().any(|l| l == want)
}

pub fn set_fetch_enabled(root: &Path, on: bool) {
    let body = turned_off(&read("fetch-off.txt"), &root.display().to_string(), !on);
    save("fetch-off.txt", body);
}

/// The repositories fetch is off for, `want` listed or not.
fn turned_off(list: &str, want: &str, off: bool) -> String {
    let mut lines: Vec<&str> = list
        .lines()
        .filter(|l| !l.is_empty() && *l != want)
        .collect();
    if off {
        lines.push(want);
    }
    lines.join("\n") + "\n"
}

/// Pane sizes in logical pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// Width of the branches/versions/commits column.
    pub sidebar: f32,
    /// Height of the branch list.
    pub branches: f32,
    /// The side panel is shown (⌥⌘1). Its sections' folds are their features' (`panel.txt`).
    pub show_sidebar: bool,
    /// The menu of spaces is folded to icons.
    pub fold_places: bool,
    /// Its width open, its edge dragged (#186).
    pub menu: f32,
    /// The tool window open on the right (`discussions`, `agents`, `notifications`), empty for none.
    pub tool: String,
    /// Its width.
    pub tool_width: f32,
    /// The width of the Inbox's, Branches' and Merge requests' list beside its preview.
    pub list: f32,
    /// The menu's spaces in the order dragged to, by name (`inbox,code,branches,requests`);
    /// empty, the first order.
    pub spaces: String,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            sidebar: 300.,
            branches: 180.,
            show_sidebar: true,
            fold_places: false,
            menu: 216.,
            tool: String::new(),
            tool_width: 320.,
            list: 460.,
            spaces: String::new(),
        }
    }
}

impl Layout {
    pub fn load() -> Self {
        Self::parse(&read(LAYOUT))
    }

    pub fn save(&self) {
        save(LAYOUT, self.to_string());
    }

    fn parse(text: &str) -> Self {
        let mut layout = Self::default();
        for (key, value) in pairs(text) {
            match key {
                "show_sidebar" => layout.show_sidebar = value != "false",
                "fold_places" => layout.fold_places = value == "true",
                "tool" => layout.tool = value.to_owned(),
                "spaces" => layout.spaces = value.to_owned(),
                // Before the tool windows: the comments drawer, now Discussions.
                "comments_open" if value == "true" => layout.tool = "discussions".to_owned(),
                _ => {}
            }
            let Ok(value) = value.parse::<f32>() else {
                continue;
            };
            match key {
                "sidebar" => layout.sidebar = value,
                "branches" => layout.branches = value,
                "menu" => layout.menu = value,
                "tool_width" => layout.tool_width = value,
                "list" => layout.list = value,
                _ => {}
            }
        }
        layout
    }
}

impl std::fmt::Display for Layout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "sidebar={}", self.sidebar)?;
        writeln!(f, "branches={}", self.branches)?;
        writeln!(f, "show_sidebar={}", self.show_sidebar)?;
        writeln!(f, "fold_places={}", self.fold_places)?;
        writeln!(f, "menu={}", self.menu)?;
        writeln!(f, "tool={}", self.tool)?;
        writeln!(f, "tool_width={}", self.tool_width)?;
        writeln!(f, "list={}", self.list)?;
        writeln!(f, "spaces={}", self.spaces)
    }
}

/// What a diff marks inside its changed lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffMode {
    /// git's line diff: whole lines.
    Lines,
    /// The line diff, plus the words that changed inside a changed line.
    #[default]
    Words,
    /// difftastic's syntax-aware alignment and token marks.
    Structural,
}

impl DiffMode {
    pub const ALL: [DiffMode; 3] = [DiffMode::Lines, DiffMode::Words, DiffMode::Structural];

    pub fn label(self) -> &'static str {
        match self {
            DiffMode::Lines => "Lines",
            DiffMode::Words => "Words",
            DiffMode::Structural => "Structural",
        }
    }

    /// The next mode, wrapping around.
    pub fn next(self) -> Self {
        match self {
            DiffMode::Lines => DiffMode::Words,
            DiffMode::Words => DiffMode::Structural,
            DiffMode::Structural => DiffMode::Lines,
        }
    }

    fn key(self) -> &'static str {
        match self {
            DiffMode::Lines => "lines",
            DiffMode::Words => "words",
            DiffMode::Structural => "structural",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == key)
    }
}

/// How a changed word is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MarkStyle {
    #[default]
    Tinted,
    Underlined,
}

impl MarkStyle {
    pub const ALL: [MarkStyle; 2] = [MarkStyle::Tinted, MarkStyle::Underlined];

    pub fn label(self) -> &'static str {
        match self {
            MarkStyle::Tinted => "Tinted",
            MarkStyle::Underlined => "Underlined",
        }
    }

    fn key(self) -> &'static str {
        match self {
            MarkStyle::Tinted => "tinted",
            MarkStyle::Underlined => "underlined",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == key)
    }
}

/// How several open repositories are shown (redesign round 2): a rail of projects on the left,
/// tabs in the title bar, or the projects inside the menu of spaces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Projects {
    #[default]
    Rail,
    Tabs,
    Menu,
}

impl Projects {
    pub const ALL: [Projects; 3] = [Projects::Rail, Projects::Tabs, Projects::Menu];

    pub fn label(self) -> &'static str {
        match self {
            Projects::Rail => "Rail",
            Projects::Tabs => "Tabs",
            Projects::Menu => "Menu",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Projects::Rail => "rail",
            Projects::Tabs => "tabs",
            Projects::Menu => "menu",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }
}

/// Where a repository opened while another is open goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpenIn {
    #[default]
    Ask,
    ThisTab,
    NewTab,
}

impl OpenIn {
    pub const ALL: [OpenIn; 3] = [OpenIn::Ask, OpenIn::ThisTab, OpenIn::NewTab];

    pub fn label(self) -> &'static str {
        match self {
            OpenIn::Ask => "Ask",
            OpenIn::ThisTab => "This tab",
            OpenIn::NewTab => "New tab",
        }
    }

    fn key(self) -> &'static str {
        match self {
            OpenIn::Ask => "ask",
            OpenIn::ThisTab => "this-tab",
            OpenIn::NewTab => "new-tab",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|o| o.key() == key)
    }
}

/// Preferences from the Settings page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The editor files open in, by its key (`crate::editor::Editor::key`); `None`, or a key no editor has, takes
    /// the first one installed.
    pub editor: Option<String>,
    /// How changed words are drawn, in Words and Structural modes.
    pub mark_style: MarkStyle,
    /// The mode a diff opens in.
    pub default_mode: DiffMode,
    /// Re-read the repository when something outside the app changes it.
    pub auto_refresh: bool,
    /// Lock files and other generated files start folded in a diff.
    pub fold_generated: bool,
    /// With all files in one scroll, a file scrolled past is marked reviewed.
    pub auto_reviewed: bool,
    /// The size of the code in a diff, in percent of the normal size (⌘+ ⌘− ⌘0).
    pub zoom: u16,
    /// The size of the whole interface, in percent (⌘+ ⌘− ⌘0).
    pub ui_zoom: u16,
    /// Comments are drawn as formatted markdown, not as the text written (⌥M).
    pub rich_comments: bool,
    /// The keyboard card was shown at a first launch; it is not shown at launch again.
    pub keys_seen: bool,
    /// Where a repository opened while another is open goes.
    pub open_in: OpenIn,
    /// What the "this tab / new tab" question offers first: the last answer.
    pub last_new_tab: bool,
    /// How several open repositories are shown.
    pub projects: Projects,
    /// Flat panes (hairlines, no gaps) instead of islands.
    pub flat: bool,
    /// Panels, menus and folds move instead of jumping (#56).
    pub animations: bool,
    /// Ask GitHub whether a newer Artoo is out (#135).
    pub check_updates: bool,
    /// What the Inbox and the agents bring while Artoo is not in front goes to the system's
    /// notifications too (#26).
    pub system_notifications: bool,
    /// The Dock's icon counts the Inbox's unread (#196).
    pub dock_count: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            editor: None,
            mark_style: MarkStyle::default(),
            default_mode: DiffMode::default(),
            auto_refresh: true,
            fold_generated: true,
            auto_reviewed: false,
            zoom: 100,
            ui_zoom: 100,
            rich_comments: true,
            keys_seen: false,
            open_in: OpenIn::Ask,
            last_new_tab: false,
            projects: Projects::Rail,
            flat: false,
            animations: true,
            check_updates: true,
            system_notifications: false,
            dock_count: true,
        }
    }
}

impl Settings {
    pub const ZOOM_MIN: u16 = 60;
    pub const ZOOM_MAX: u16 = 250;
    pub const ZOOM_STEP: u16 = 10;

    pub fn load() -> Self {
        Self::parse(&read("settings.txt"))
    }

    pub fn save(&self) {
        save("settings.txt", self.to_string());
    }

    fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for (key, value) in pairs(text) {
            match key {
                "mark_style" => {
                    settings.mark_style = MarkStyle::from_key(value).unwrap_or_default()
                }
                "default_mode" => {
                    settings.default_mode = DiffMode::from_key(value).unwrap_or_default()
                }
                "auto_refresh" => settings.auto_refresh = value != "false",
                "fold_generated" => settings.fold_generated = value != "false",
                "auto_reviewed" => settings.auto_reviewed = value == "true",
                "rich_comments" => settings.rich_comments = value != "false",
                "keys_seen" => settings.keys_seen = value == "true",
                "open_in" => settings.open_in = OpenIn::from_key(value).unwrap_or_default(),
                "last_new_tab" => settings.last_new_tab = value == "true",
                "projects" => settings.projects = Projects::from_key(value).unwrap_or_default(),
                "flat" => settings.flat = value == "true",
                "animations" => settings.animations = value != "false",
                "check_updates" => settings.check_updates = value != "false",
                "system_notifications" => settings.system_notifications = value == "true",
                "dock_count" => settings.dock_count = value != "false",
                "ui_zoom" => {
                    settings.ui_zoom = value.parse().map_or(100, |z: u16| {
                        z.clamp(Settings::ZOOM_MIN, Settings::ZOOM_MAX)
                    })
                }
                "zoom" => {
                    settings.zoom = value.parse().map_or(100, |z: u16| {
                        z.clamp(Settings::ZOOM_MIN, Settings::ZOOM_MAX)
                    })
                }
                "editor" => settings.editor = Some(value.to_owned()).filter(|v| !v.is_empty()),
                _ => {}
            }
        }
        settings
    }
}

impl std::fmt::Display for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "mark_style={}", self.mark_style.key())?;
        writeln!(f, "default_mode={}", self.default_mode.key())?;
        writeln!(f, "auto_refresh={}", self.auto_refresh)?;
        writeln!(f, "fold_generated={}", self.fold_generated)?;
        writeln!(f, "auto_reviewed={}", self.auto_reviewed)?;
        writeln!(f, "rich_comments={}", self.rich_comments)?;
        writeln!(f, "keys_seen={}", self.keys_seen)?;
        writeln!(f, "open_in={}", self.open_in.key())?;
        writeln!(f, "last_new_tab={}", self.last_new_tab)?;
        writeln!(f, "projects={}", self.projects.key())?;
        writeln!(f, "flat={}", self.flat)?;
        writeln!(f, "animations={}", self.animations)?;
        writeln!(f, "check_updates={}", self.check_updates)?;
        writeln!(f, "system_notifications={}", self.system_notifications)?;
        writeln!(f, "dock_count={}", self.dock_count)?;
        writeln!(f, "zoom={}", self.zoom)?;
        writeln!(f, "ui_zoom={}", self.ui_zoom)?;
        if let Some(editor) = &self.editor {
            writeln!(f, "editor={editor}")?;
        }
        Ok(())
    }
}

/// How a diff is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewOptions {
    /// One column with removals above additions, instead of side by side.
    pub unified: bool,
    /// What is marked inside changed lines. Starts as the default mode; not remembered.
    pub mode: DiffMode,
    /// Long lines wrap instead of scrolling horizontally.
    pub wrap: bool,
    /// Every unchanged line, instead of three around each change.
    pub full_context: bool,
    /// Whitespace-only changes count as unchanged.
    pub ignore_whitespace: bool,
    /// Changed files as a flat list instead of a folder tree.
    pub list_files: bool,
    /// One file at a time instead of every file of the diff in one scroll.
    pub one_file: bool,
    /// With every file in one scroll, a file marked reviewed folds to its header.
    pub fold_reviewed: bool,
}

impl ViewOptions {
    pub fn load(settings: &Settings) -> Self {
        let mut options = Self::parse(&read("view.txt"));
        options.mode = settings.default_mode;
        options
    }

    pub fn save(&self) {
        save("view.txt", self.to_string());
    }

    fn parse(text: &str) -> Self {
        let mut options = Self::default();
        for (key, value) in pairs(text) {
            let value = value == "true";
            match key {
                "unified" => options.unified = value,
                "wrap" => options.wrap = value,
                "full_context" => options.full_context = value,
                "ignore_whitespace" => options.ignore_whitespace = value,
                "list_files" => options.list_files = value,
                "one_file" => options.one_file = value,
                "fold_reviewed" => options.fold_reviewed = value,
                _ => {}
            }
        }
        options
    }
}

impl std::fmt::Display for ViewOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "unified={}", self.unified)?;
        writeln!(f, "wrap={}", self.wrap)?;
        writeln!(f, "full_context={}", self.full_context)?;
        writeln!(f, "ignore_whitespace={}", self.ignore_whitespace)?;
        writeln!(f, "list_files={}", self.list_files)?;
        writeln!(f, "one_file={}", self.one_file)?;
        writeln!(f, "fold_reviewed={}", self.fold_reviewed)
    }
}

/// `key=value` lines; anything else is skipped.
fn pairs(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.trim(), v.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_is_on_until_turned_off_for_a_repository() {
        let off = turned_off("", "/p/app", true);
        assert_eq!(off, "/p/app\n");
        assert_eq!(turned_off(&off, "/p/web", true), "/p/app\n/p/web\n");
        assert_eq!(turned_off(&off, "/p/app", false), "\n");
        assert_eq!(turned_off(&off, "/p/app", true), "/p/app\n");
    }

    #[test]
    fn settings_round_trip_and_default_to_the_designed_choices() {
        assert_eq!(Settings::parse(""), Settings::default());
        assert_eq!(Settings::default().default_mode, DiffMode::Words);
        assert_eq!(Settings::default().mark_style, MarkStyle::Tinted);
        assert!(Settings::default().rich_comments);
        let settings = Settings {
            editor: Some("zed".to_owned()),
            mark_style: MarkStyle::Underlined,
            default_mode: DiffMode::Structural,
            auto_refresh: false,
            fold_generated: false,
            auto_reviewed: true,
            zoom: 130,
            ui_zoom: 120,
            rich_comments: false,
            keys_seen: true,
            open_in: OpenIn::NewTab,
            last_new_tab: true,
            projects: Projects::Menu,
            flat: true,
            animations: false,
            check_updates: false,
            system_notifications: true,
            dock_count: false,
        };
        assert_eq!(Settings::parse(&settings.to_string()), settings);
    }

    #[test]
    fn a_value_is_set_among_the_others() {
        assert_eq!(with_value("", "a", "1"), "a=1\n");
        assert_eq!(with_value("a=1\nb=2\n", "a", "3"), "b=2\na=3\n");
    }

    #[test]
    fn layout_round_trips_and_ignores_junk() {
        let layout = Layout {
            sidebar: 400.,
            branches: 120.,
            show_sidebar: false,
            fold_places: true,
            menu: 300.,
            tool: "agents".into(),
            tool_width: 300.,
            list: 380.,
            spaces: "code,inbox".into(),
        };
        assert_eq!(Layout::parse(&layout.to_string()), layout);
        assert_eq!(Layout::parse("comments_open=true\n").tool, "discussions");
        assert_eq!(
            Layout::parse("files=abc\nnope\nwidth=3\n"),
            Layout::default()
        );
    }
}
