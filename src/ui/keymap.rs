//! Rebindable keys. The defaults are the bindings the app ships (`install`); a preset (#52: Artoo,
//! Android Studio, VS Code) changes some of them, and `keymap.txt` in the app support folder
//! replaces the keys of an action on top, one line each: `NextFile = ] | l` (keys split by ` | `,
//! nothing after `=` leaves the action without a key; `Preset = vscode` names the preset).
//! Settings → Keyboard writes it.

use artoo_core::{platform, storage};
use gpui::{App, DummyKeyboardMapper, Global, KeyBinding};
use std::collections::{BTreeMap, HashSet};

/// The keys as written in code, the preset over them, and the user's changes.
pub struct Keymap {
    defaults: Vec<KeyBinding>,
    /// Whether the keys are macOS's (`platform::MAC`); a test reads Windows and Linux's on a Mac.
    mac: bool,
    preset: Preset,
    /// Action name (`NextFile`) → its keys (`cmd-]`, `l`).
    overrides: BTreeMap<String, Vec<String>>,
}

/// Keys to start from, as another tool has them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Preset {
    #[default]
    Artoo,
    AndroidStudio,
    VsCode,
}

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::Artoo, Preset::AndroidStudio, Preset::VsCode];

    pub fn label(self) -> &'static str {
        match self {
            Preset::Artoo => "Artoo",
            Preset::AndroidStudio => "Android Studio",
            Preset::VsCode => "VS Code",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Preset::Artoo => "artoo",
            Preset::AndroidStudio => "android-studio",
            Preset::VsCode => "vscode",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        // `gitlance`: these keys' name before Artoo's.
        if key == "gitlance" {
            return Some(Preset::Artoo);
        }
        Self::ALL.into_iter().find(|p| p.key() == key)
    }

    /// What it changes from the shipped keys, written for macOS (`map` puts them in this OS's
    /// terms).
    fn changes(self) -> &'static [(&'static str, &'static [&'static str])] {
        match self {
            Preset::Artoo => &[],
            // Go to File ⌘⇧O, Find Action ⌘⇧A, File Structure ⌘F12, Find in Files ⌘⇧F (the
            // commit search keeps ⌘⇧C), Hide All Tool Windows ⌘⇧F12, Synchronize ⌥⌘Y (X55).
            Preset::AndroidStudio => &[
                ("SearchFiles", &["cmd-shift-o"]),
                ("SearchActions", &["cmd-shift-a"]),
                ("ShowStructure", &["cmd-f12"]),
                // ⇧⌘A is Find Action there: the Agents window takes ⌥⇧⌘A.
                ("ToggleAgents", &["cmd-alt-shift-a"]),
                ("FindInFiles", &["cmd-shift-f"]),
                ("FindCommits", &[]),
                ("FocusDiff", &["cmd-shift-f12"]),
                ("Refresh", &["cmd-alt-y"]),
            ],
            // Toggle Side Bar ⌘B (the menu folds with ⌥⌘0 there), Open Recent ⌃R, Find in Files
            // ⌘⇧F, Go Back ⌃− and Go Forward ⌃⇧− beside ⌘[ ⌘].
            Preset::VsCode => &[
                ("ToggleSidebar", &["cmd-b"]),
                ("FoldMenu", &["cmd-alt-0"]),
                ("OpenRecent", &["ctrl-r"]),
                ("FindInFiles", &["cmd-shift-f"]),
                ("FindCommits", &[]),
                ("GoBack", &["ctrl--", "cmd-["]),
                ("GoForward", &["ctrl-shift--", "cmd-]"]),
            ],
        }
    }

    /// Where Windows and Linux take other keys than `changes`, written the same way: the tool's
    /// own keys there, or a ⌃ key that `cmd-` → `ctrl-` would give another action too.
    fn pc_changes(self) -> &'static [(&'static str, &'static [&'static str])] {
        match self {
            Preset::Artoo | Preset::AndroidStudio => &[],
            // Go Back and Go Forward are Alt+← Alt+→ there (Ctrl+− is Zoom out); Ctrl+R is Open
            // Recent, so Refresh takes F5.
            Preset::VsCode => &[
                ("GoBack", &["alt-left", "cmd-["]),
                ("GoForward", &["alt-right", "cmd-]"]),
                ("Refresh", &["f5"]),
            ],
        }
    }

    /// The actions it gives other keys, on any OS.
    #[cfg(test)]
    pub fn actions(self) -> impl Iterator<Item = &'static str> {
        self.changes()
            .iter()
            .chain(self.pc_changes())
            .map(|(name, _)| *name)
    }

    /// Its changes in macOS's keys (`mac`) or in Windows and Linux's: action → keys.
    fn map(self, mac: bool) -> BTreeMap<String, Vec<String>> {
        let pc = if mac { &[][..] } else { self.pc_changes() };
        self.changes()
            .iter()
            .chain(pc)
            .map(|(name, keys)| {
                let keys = keys
                    .iter()
                    .map(|&k| {
                        if mac {
                            k.to_owned()
                        } else {
                            platform::pc_keys(k)
                        }
                    })
                    .collect();
                ((*name).to_owned(), keys)
            })
            .collect()
    }
}

impl Global for Keymap {}

/// One action as Settings → Keyboard lists it.
pub struct Entry {
    pub name: &'static str,
    /// `Next file`.
    pub label: String,
    /// Its keys, as GPUI writes them (`cmd-shift-enter`).
    pub keys: Vec<String>,
    pub changed: bool,
    /// For a changed one, the keys it had before: the preset's or the shipped ones.
    pub was: Vec<String>,
    /// Another action on one of its keys where both apply: (key, the other action's label).
    pub clash: Option<(String, String)>,
}

/// Binds the defaults with the user's changes on top.
pub fn install(defaults: Vec<KeyBinding>, cx: &mut App) {
    cx.set_global(Keymap::from_text(
        defaults,
        platform::MAC,
        &storage::read("keymap.txt"),
    ));
    apply(cx);
}

/// The preset and the changes in `keymap.txt`.
fn read(text: &str) -> (Preset, BTreeMap<String, Vec<String>>) {
    let mut overrides = parse(text);
    let preset = overrides
        .remove("Preset")
        .and_then(|keys| keys.first().and_then(|k| Preset::from_key(k)))
        .unwrap_or_default();
    (preset, overrides)
}

fn write(map: &Keymap) {
    let mut text = String::new();
    if map.preset != Preset::Artoo {
        text.push_str(&format!("Preset = {}\n", map.preset.key()));
    }
    text.push_str(&render(&map.overrides));
    storage::save("keymap.txt", text);
}

/// Starts from `preset`; the user's changes stay on top.
pub fn set_preset(preset: Preset, cx: &mut App) {
    cx.global_mut::<Keymap>().preset = preset;
    write(cx.global::<Keymap>());
    apply(cx);
}

/// Drops every change: the preset's keys again.
pub fn reset(cx: &mut App) {
    cx.global_mut::<Keymap>().overrides.clear();
    write(cx.global::<Keymap>());
    apply(cx);
}

/// Gives action `name` the keys `keys` (`None`: back to its defaults) and binds again.
pub fn set(name: &str, keys: Option<Vec<String>>, cx: &mut App) {
    let map = cx.global_mut::<Keymap>();
    match keys {
        Some(keys) => {
            map.overrides.insert(name.to_owned(), keys);
        }
        None => {
            map.overrides.remove(name);
        }
    }
    write(cx.global::<Keymap>());
    apply(cx);
}

fn apply(cx: &mut App) {
    let bindings = cx.global::<Keymap>().effective();
    cx.clear_key_bindings();
    cx.bind_keys(bindings);
}

/// `artoo::NextFile` → `NextFile`.
fn short(name: &'static str) -> &'static str {
    name.rsplit("::").next().unwrap_or(name)
}

impl Keymap {
    /// `defaults` with the preset and the changes `text` (`keymap.txt`'s form) names, in macOS's keys or (`mac` false)
    /// Windows and Linux's.
    pub fn from_text(defaults: Vec<KeyBinding>, mac: bool, text: &str) -> Self {
        let (preset, overrides) = read(text);
        Self {
            defaults,
            mac,
            preset,
            overrides,
        }
    }

    /// `defaults` with `preset`'s keys and no change of the user's.
    #[cfg(test)]
    pub fn with_preset(defaults: Vec<KeyBinding>, mac: bool, preset: Preset) -> Self {
        Self {
            defaults,
            mac,
            preset,
            overrides: BTreeMap::new(),
        }
    }

    pub fn preset(&self) -> Preset {
        self.preset
    }

    /// The keys Artoo ships for action `name`, before a preset or the user's changes.
    pub fn shipped(&self, name: &str) -> Vec<String> {
        self.defaults
            .iter()
            .filter(|d| short(d.action().name()) == name)
            .map(keystrokes)
            .collect()
    }

    /// How many actions the user changed.
    pub fn changed_count(&self) -> usize {
        self.overrides.len()
    }

    /// The preset's changes with the user's on top: action → keys.
    fn layer(&self) -> BTreeMap<String, Vec<String>> {
        let mut layer = self.preset.map(self.mac);
        layer.extend(self.overrides.clone());
        layer
    }

    /// Every action's keys as they are now, in `keymap.txt`'s form (Export).
    pub fn export(&self) -> String {
        let mut text = format!("# Artoo keys · preset {}\n", self.preset.label());
        for e in self.entries() {
            text.push_str(&format!("{} = {}\n", e.name, e.keys.join(" | ")));
        }
        text
    }

    /// The bindings in force: an action with changed keys gets them, in the context of its first
    /// default binding. The changed ones come last, so where a key is also shipped for another
    /// action (a clash the Keyboard page shows), the user's choice is the one that acts.
    fn effective(&self) -> Vec<KeyBinding> {
        let mut out = Vec::new();
        let mut changed = Vec::new();
        let mut replaced = HashSet::new();
        let layer = self.layer();
        for b in &self.defaults {
            let name = short(b.action().name());
            let Some(keys) = layer.get(name) else {
                out.push(b.clone());
                continue;
            };
            if !replaced.insert(name) {
                continue;
            }
            changed.extend(keys.iter().filter_map(|k| {
                KeyBinding::load(
                    k,
                    b.action().boxed_clone(),
                    b.predicate(),
                    false,
                    None,
                    &DummyKeyboardMapper,
                )
                .ok()
            }));
        }
        out.extend(changed);
        out
    }

    /// Every action with a key (or that had one), by label.
    pub fn entries(&self) -> Vec<Entry> {
        let effective = self.effective();
        let mut seen = HashSet::new();
        let mut entries: Vec<Entry> = self
            .defaults
            .iter()
            .filter_map(|b| {
                let name = short(b.action().name());
                seen.insert(name).then(|| {
                    let mine: Vec<&KeyBinding> = effective
                        .iter()
                        .filter(|e| short(e.action().name()) == name)
                        .collect();
                    let keys: Vec<String> = mine.iter().map(|e| keystrokes(e)).collect();
                    let clash = mine.iter().find_map(|e| {
                        let key = keystrokes(e);
                        effective
                            .iter()
                            .filter(|o| short(o.action().name()) != name)
                            .find(|o| keystrokes(o) == key && overlap(e, o))
                            .map(|o| (key.clone(), label(short(o.action().name()))))
                    });
                    let changed = self.overrides.contains_key(name);
                    let was = if changed {
                        self.preset.map(self.mac).remove(name).unwrap_or_else(|| {
                            self.defaults
                                .iter()
                                .filter(|d| short(d.action().name()) == name)
                                .map(keystrokes)
                                .collect()
                        })
                    } else {
                        Vec::new()
                    };
                    Entry {
                        name,
                        label: label(name),
                        keys,
                        changed,
                        was,
                        clash,
                    }
                })
            })
            .collect();
        entries.sort_by(|a, b| a.label.cmp(&b.label));
        entries
    }

    /// The key to show for `action` when the user changed it (`""` when it has none now); `None`
    /// keeps the shipped one.
    pub fn shown(&self, action: &dyn gpui::Action) -> Option<String> {
        self.layer()
            .get(short(action.name()))
            .map(|keys| keys.first().map(|k| symbols(k)).unwrap_or_default())
    }

    /// The key `action` has now, as the app writes keys: the user's or the preset's change, else the one it
    /// ships with; `""` when it has none.
    pub fn key_now(&self, action: &dyn gpui::Action) -> String {
        self.shown(action).unwrap_or_else(|| {
            self.shipped_key(action)
                .map(|k| symbols(&k))
                .unwrap_or_default()
        })
    }

    /// The first key `action` ships with, as bindings write it (`cmd-shift-r`); `None` when it ships with none.
    pub fn shipped_key(&self, action: &dyn gpui::Action) -> Option<String> {
        self.shipped(short(action.name())).into_iter().next()
    }

    /// The keys the user changed: (label, keys).
    pub fn changes(&self) -> Vec<(String, Vec<String>)> {
        self.overrides
            .iter()
            .map(|(name, keys)| (label(name), keys.clone()))
            .collect()
    }
}

fn keystrokes(b: &KeyBinding) -> String {
    b.keystrokes()
        .iter()
        .map(|k| k.inner().unparse())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether two bindings can be live at once. Contexts here are `Workspace` with `Typing`,
/// `Palette` and `Caret` refinements: the same context, or one of them plain `Workspace` (or
/// none), counts as overlapping; two different refinements are taken to be meant apart.
fn overlap(a: &KeyBinding, b: &KeyBinding) -> bool {
    let context = |k: &KeyBinding| k.predicate().map(|p| p.to_string());
    let (ca, cb) = (context(a), context(b));
    let plain = |c: &Option<String>| c.as_deref().is_none_or(|c| c == "Workspace");
    ca == cb || plain(&ca) || plain(&cb)
}

/// `NextFile` → `Next file`.
fn label(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// `cmd-shift-enter` → `⇧⌘↵`, as the app writes keys (macOS order: ⌃ ⌥ ⇧ ⌘).
pub fn symbols(keys: &str) -> String {
    keys.split(' ')
        .map(|stroke| {
            let parts: Vec<&str> = stroke.split('-').collect();
            // `cmd--` is ⌘ and the minus key.
            let (mods, key) = match parts.as_slice() {
                [.., "", ""] => (&parts[..parts.len() - 2], "-"),
                [mods @ .., key] => (mods, *key),
                [] => (&parts[..0], ""),
            };
            let mut out = String::new();
            for (word, sign) in [("ctrl", "⌃"), ("alt", "⌥"), ("shift", "⇧"), ("cmd", "⌘")]
            {
                if mods.contains(&word) {
                    out.push_str(sign);
                }
            }
            // F1 … F12 as written on the key.
            let function = key.len() > 1
                && key.starts_with('f')
                && key[1..].bytes().all(|b| b.is_ascii_digit());
            if function {
                out.push('F');
                out.push_str(&key[1..]);
                return out;
            }
            out.push_str(match key {
                "enter" => "↵",
                "escape" => "esc",
                "backspace" => "⌫",
                "delete" => "⌦",
                "tab" => "⇥",
                "space" => "Space",
                "up" => "↑",
                "down" => "↓",
                "left" => "←",
                "right" => "→",
                "pageup" => "PgUp",
                "pagedown" => "PgDn",
                "home" => "Home",
                "end" => "End",
                other => other,
            });
            out
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse(text: &str) -> BTreeMap<String, Vec<String>> {
    text.lines()
        .filter_map(|line| {
            let (name, keys) = line.split_once('=')?;
            let name = name.trim();
            (!name.is_empty() && !name.starts_with('#')).then(|| {
                let keys = keys
                    .split(" | ")
                    .map(str::trim)
                    .filter(|k| !k.is_empty())
                    .map(str::to_owned)
                    .collect();
                (name.to_owned(), keys)
            })
        })
        .collect()
}

fn render(overrides: &BTreeMap<String, Vec<String>>) -> String {
    overrides
        .iter()
        .map(|(name, keys)| format!("{name} = {}\n", keys.join(" | ")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    gpui::actions!(test, [NextFile, PreviousFile, Comment]);

    fn map(overrides: &str) -> Keymap {
        Keymap {
            mac: platform::MAC,
            preset: Preset::Artoo,
            defaults: vec![
                KeyBinding::new("l", NextFile, Some("Workspace && !Typing")),
                KeyBinding::new("]", NextFile, Some("Workspace && !Typing")),
                KeyBinding::new("h", PreviousFile, Some("Workspace && !Typing")),
                KeyBinding::new("c", Comment, Some("Workspace && !Typing")),
            ],
            overrides: parse(overrides),
        }
    }

    #[test]
    fn keys_picked_under_gitlances_name_stay_picked() {
        assert_eq!(Preset::from_key("gitlance"), Some(Preset::Artoo));
        assert_eq!(Preset::from_key("artoo"), Some(Preset::Artoo));
        assert_eq!(Preset::from_key("vscode"), Some(Preset::VsCode));
    }

    #[test]
    fn a_change_replaces_every_key_of_its_action() {
        // alt, not cmd: cmd is written super- on Linux and win- on Windows.
        let entries = map("NextFile = alt-down | j\n").entries();
        let next = entries.iter().find(|e| e.name == "NextFile").unwrap();
        assert_eq!(next.keys, vec!["alt-down", "j"]);
        assert!(next.changed);
        assert_eq!(next.label, "Next file");
        assert!(next.clash.is_none());
    }

    #[test]
    fn a_changed_key_is_bound_last_so_it_wins() {
        let bound = map("NextFile = c\n").effective();
        assert_eq!(short(bound.last().unwrap().action().name()), "NextFile");
    }

    #[test]
    fn a_key_taken_twice_is_a_clash() {
        let entries = map("Comment = h\n").entries();
        let comment = entries.iter().find(|e| e.name == "Comment").unwrap();
        assert_eq!(comment.clash, Some(("h".into(), "Previous file".into())));
    }

    #[test]
    fn an_empty_change_unbinds() {
        let entries = map("Comment =\n").entries();
        assert!(
            entries
                .iter()
                .find(|e| e.name == "Comment")
                .unwrap()
                .keys
                .is_empty()
        );
    }

    #[test]
    fn the_file_reads_back_what_it_wrote() {
        let overrides = parse("NextFile = cmd-] | l\nComment =\n");
        assert_eq!(parse(&render(&overrides)), overrides);
    }

    #[test]
    fn keys_are_written_as_the_app_writes_them() {
        assert_eq!(symbols("cmd-shift-enter"), "⇧⌘↵");
        assert_eq!(symbols("alt-down"), "⌥↓");
        assert_eq!(symbols("cmd--"), "⌘-");
        assert_eq!(symbols("ctrl-1"), "⌃1");
        assert_eq!(symbols("cmd-f3"), "⌘F3");
    }
}
