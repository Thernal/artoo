//! The keyboard card's keys, grouped by what one is doing; the frame draws the card
//! (`artoo_shell_api::shortcuts`).

use super::Workspace;
use super::keymap::{self, Keymap, Preset};
use artoo_shell_api::shortcuts::{self, Group, Keys};
use gpui::{Context, prelude::*};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;

/// The card was shown at launch in this run already (a new tab does not show it again).
pub(super) static SHOWN: AtomicBool = AtomicBool::new(false);

/// (group, [(keys, what it does)]).
const GROUPS: &[Group] = &[
    (
        "Move",
        &[
            ("⌃⌘1 ⌃⌘2 ⌃⌘3 ⌃⌘4", "Inbox, Code, Branches, Merge requests"),
            ("Tab ⇧Tab", "next / previous island"),
            ("⌃1", "switch to a branch"),
            ("⌃2 ⌃3 ⌃4", "commits, files, diff"),
            ("j k", "next / previous commit"),
            ("h l  ← →", "previous / next file"),
            (
                "[  ]",
                "on a commit: the one before / after it; else the file",
            ),
            ("n p", "next / previous change"),
            (
                "v  ⇧v",
                "mark the file reviewed and go on · toggle the mark",
            ),
            ("⇧n ⇧p", "next / previous discussion"),
            ("⌥⇧V", "mark all files reviewed / unmark all"),
            ("⌥↓ ⌥↑", "next unreviewed / previous merge request"),
            ("Space ⇧Space", "page down / up"),
            ("esc", "one page up · ⌘W closes what is open"),
            ("⌘[ ⌘]", "back / forward, across spaces"),
        ],
    ),
    (
        "Comment",
        &[
            ("c", "comment by line number"),
            ("↑ ↓ in it", "step the line"),
            ("Tab in the box", "agent / merge request"),
            ("⌘↵", "add or post"),
            (
                "⇧⌘↵",
                "keep pending · submit review (comment, approve, request changes)",
            ),
            ("⌥N ⌥P", "next / previous of your pending comments"),
            ("a", "approve the merge request · take the approval back"),
            (
                "⌘⇧N",
                "the Inbox: mentions, review requests, replies (j k ↵ e)",
            ),
            ("r  x  o", "reply to, resolve, fold a discussion"),
            ("e", "edit your comment in the discussion"),
            ("⇧e", "react to the discussion's newest note"),
            ("⌘⇧D", "show or hide the Discussions window · esc hides it"),
            ("⌥M", "comments as markdown / raw text"),
            ("⌥⌘A", "send the comments for your agent to its chat"),
            ("⇧⌘A", "show or hide the Agents window"),
            ("⌘B ⌘I ⌘⇧X ⌘⇧E ⌘⇧K", "bold, italic, strike, code, link"),
            ("⌘⇧P in the box", "preview the markdown"),
        ],
    ),
    (
        "Find",
        &[
            ("⌘K", "search everywhere · ⇥ the next tab, > actions"),
            ("⌘P  ⌘⇧C  ⌘⇧B", "find a file · a commit · a branch"),
            ("⌘⇧R  ⌘⇧P", "find a merge request · an action"),
            ("⌘F", "find in the diff · filter a space's list"),
            ("⌘⌥F", "search all files of the diff"),
            ("⌘⇧F", "search commits"),
            (
                "⌥⌘P",
                "filter the changed files by path; in Code, pin the tab",
            ),
            ("⌥⌘C", "compare two refs"),
            ("⌥⇧⌘C", "copy the link to this page"),
            ("⌘E", "recent places"),
            ("⌘⇧O", "functions and types this file changes"),
            ("B  ⇧B", "bookmark a line · list the bookmarks"),
        ],
    ),
    (
        "View",
        &[
            ("⌥U ⌥D ⌥Z", "split, mode, wrap"),
            ("⌥E ⌥W", "full file, whitespace"),
            ("⌥⌘B", "annotate: who wrote each line"),
            ("⌘J ⌥⌘2", "side panel, fold Changes"),
            ("⌘B", "fold the menu to icons"),
            ("⌘.", "zen: the diff alone, the keys of a review below"),
            ("⌘+  ⌘−  ⌘0", "interface bigger, smaller, actual size"),
            ("⌘T ⇧⌘W ⌘1…9", "tabs"),
            ("⌥⌘M", "new merge request"),
            ("⌥⌘O ⌥⌘R", "the file or folder in its app · in Finder"),
            ("?", "this card"),
        ],
    ),
];

impl Workspace {
    pub(super) fn toggle_shortcuts(&mut self, cx: &mut Context<Self>) {
        self.shortcuts = !self.shortcuts;
        cx.notify();
    }

    pub(super) fn render_shortcuts(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if !self.shortcuts {
            return None;
        }
        let (tab, menus) = (cx.entity().downgrade(), cx.entity().downgrade());
        let keymap = cx.global::<Keymap>();
        let keys = Keys {
            presets: Preset::ALL
                .into_iter()
                .map(|p| (p.label(), p == keymap.preset()))
                .collect(),
            changes: keymap
                .changes()
                .into_iter()
                .map(|(label, keys)| (label, keys.iter().map(|k| keymap::symbols(k)).collect()))
                .collect(),
        };
        Some(shortcuts::card(
            GROUPS,
            keys,
            Rc::new(move |cx| {
                tab.update(cx, |this, cx| {
                    this.shortcuts = false;
                    cx.notify()
                })
                .ok();
            }),
            // A preset picked: the menus show its keys.
            Rc::new(move |ix, cx| {
                keymap::set_preset(Preset::ALL[ix], cx);
                menus
                    .update(cx, |this, cx| {
                        this.refresh_menus(cx);
                        cx.notify()
                    })
                    .ok();
            }),
            cx,
        ))
    }
}
