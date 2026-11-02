//! The working tree against `HEAD`: a pinned entry above the commits, kept current while an agent
//! works. Read-only — Artoo never stages or commits. The history polls it (`History::start_working_poll`); its chip
//! on the Commits label is the branches' (`artoo_branches::side::working_chip`).

use super::{Selection, Workspace};
use gpui::Context;

impl Workspace {
    /// Starts looking at the working tree of the open repository.
    pub(super) fn start_working_poll(&mut self, cx: &mut Context<Self>) {
        let (plain, shown) = (self.plain, self.on_screen.clone());
        self.history
            .update(cx, |h, cx| h.start_working_poll(plain, shown, cx));
    }

    /// The working tree changed (`history::Event::Working`): what shows it reads it again.
    pub(super) fn working_changed(&mut self, cx: &mut Context<Self>) {
        self.review.update(cx, |rv, _| rv.blames.working_changed());
        if self.nav.space == super::spaces::Space::Branches && self.at_root_list() {
            self.read_tree_files(cx);
        }
        // Code's tree of the working tree follows it (#75).
        let working = self.code().is_some_and(|c| c.revision(cx).is_none());
        if self.nav.space == super::spaces::Space::Code && working {
            self.read_code_files(true, cx);
        }
        if self.history.read(cx).selection == Selection::WorkingTree {
            // Reload in place: the selected file stays.
            let keep = self.review.read(cx).current_path();
            self.load_working_tree(keep, cx);
        }
        cx.notify();
    }

    /// The working tree's poll came round (`history::Event::Beat`): the agents' answers ride on it, and pictures
    /// that came in.
    pub(super) fn beat(&mut self, cx: &mut Context<Self>) {
        self.poll_agents(cx);
        if artoo_ui_kit::pictures::get(cx).is_some_and(|p| p.take_fresh()) {
            cx.notify();
        }
    }

    /// The working tree opened as the page it is: kept in the history, and shown unless it is.
    pub(super) fn open_working_tree(&mut self, cx: &mut Context<Self>) {
        self.go_page(super::nav::Page::WorkingTree, cx);
    }

    pub(super) fn load_working_tree(&mut self, keep: Option<String>, cx: &mut Context<Self>) {
        if self.root.is_none() {
            return;
        }
        let only_unstaged = self.history.read(cx).wt.only_unstaged;
        // A reload finds a file partly staged at its place, in the group it was in (#22).
        let review = self.review_handle();
        self.history.update(cx, |h, cx| {
            h.show_working_tree(only_unstaged, keep, &*review, cx)
        });
        cx.notify();
    }

    pub(super) fn set_only_unstaged(&mut self, on: bool, cx: &mut Context<Self>) {
        self.history.update(cx, |h, _| h.wt.only_unstaged = on);
        let keep = self.review.read(cx).current_path();
        self.load_working_tree(keep, cx);
    }
}
