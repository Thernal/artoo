//! ⌘K's window in the tab (#52, Design `docs/specs/command-palette.md`): the shell's (`search::SearchWindow`, in the
//! tab's services, D20 of #171), the tab handing it what its parts offer, opening it as its actions ask — clearing what
//! it covers —, giving it the keyboard while it is open (`typing::Input::Palette`, first among the inputs) and drawing
//! it over itself. What the window lists and does is the shell's; the tab's own lists are `palette_lists`.

use super::Workspace;
use artoo_shell_api::search::SearchWindow;
use gpui::{AnyView, App, Context, KeyDownEvent, Window};
use std::rc::Rc;

impl Workspace {
    fn search_window(&self) -> Option<Rc<dyn SearchWindow>> {
        self.services.get::<Rc<dyn SearchWindow>>()
    }

    /// The scopes and the lists the tab's parts offer, its own among them, handed to the window.
    pub(super) fn offer_searches(&self, cx: &mut App) {
        if let Some(window) = self.search_window() {
            let searches = (self.parts.iter()).flat_map(|p| p.searches.iter().cloned());
            let lists = (self.parts.iter()).flat_map(|p| p.lists.iter().cloned());
            window.offer(searches.collect(), lists.collect(), cx);
        }
    }

    /// ⌘K opened on scope `id` (`search::ALL`, an offered one's), moved to its tab, closed when it is the tab open;
    /// or opened on list `id` alone. What it covers is cleared: a field, the find, a menu.
    pub(super) fn search(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.field = None;
        self.rv().clear_find(cx);
        if let Some(branches) = self.branches() {
            branches.drop_find(cx);
        }
        self.ctx_menu.update(cx, |m, cx| m.shut(cx));
        if let Some(window) = self.search_window() {
            window.search(id, cx);
        }
        cx.notify();
    }

    /// ⌘K closed without a pick, what a list tried put back.
    pub(super) fn close_palette(&mut self, cx: &mut Context<Self>) {
        if let Some(window) = self.search_window() {
            window.close(cx);
        }
        cx.notify();
    }

    pub(super) fn palette_open(&self, cx: &App) -> bool {
        self.search_window().is_some_and(|w| w.is_open(cx))
    }

    pub(super) fn palette_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(search) = self.search_window() {
            search.key(event, window, cx);
        }
        cx.notify();
    }

    pub(super) fn palette_step(&mut self, down: bool, cx: &mut Context<Self>) {
        if let Some(window) = self.search_window() {
            window.step(down, cx);
        }
    }

    /// The text selected in ⌘K's field, for Copy.
    pub(super) fn palette_selected(&self, cx: &App) -> Option<String> {
        self.search_window()?.selected(cx)
    }

    /// ⌘K's window while it is open, over the tab.
    pub(super) fn render_palette(&self, cx: &App) -> Option<AnyView> {
        self.search_window()?.view(cx)
    }

    /// Sets the field's text, as typing it would (UI checks).
    #[cfg(feature = "snapshot")]
    pub(super) fn type_in_palette(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(window) = self.search_window() {
            window.type_text(text, cx);
        }
    }

    /// The page's commands, its acts first (`Frame::here`, #192): on a request what it asks of you — the
    /// discussion's when one is ringed —, on a list what starts one, in Code what reads the file.
    pub(super) fn here_commands(&self) -> Vec<&'static str> {
        use super::nav::Page;
        use super::spaces::{BRANCHES, CODE, INBOX, REQUESTS};
        if self.settings_open {
            return Vec::new();
        }
        let page = self
            .nav_page()
            .and_then(|p| self.item_of(p))
            .and_then(|p| Page::from_route(&p));
        match (self.nav.space, page) {
            (_, Some(Page::Request(_))) => {
                let mut here = Vec::new();
                if self.thread_at.is_some() {
                    here.extend(["requests.reply", "requests.resolve"]);
                }
                here.extend([
                    "requests.approve",
                    "requests.submit",
                    "requests.merge",
                    "shell.copy-link",
                    "requests.next",
                ]);
                here
            }
            (_, Some(Page::Branch(_))) => vec![
                "agents.send",
                "requests.create",
                "branches.compare",
                "shell.copy-link",
                "shell.open-in",
            ],
            (INBOX, _) => vec!["requests.reply", "shell.copy-link", "shell.open-toast"],
            (REQUESTS, _) => vec!["requests.create", "shell.switch", "shell.recent"],
            (BRANCHES, _) => vec!["shell.switch", "branches.compare", "requests.create"],
            (CODE, _) => vec![
                "review.find-in-files",
                "review.structure",
                "review.bookmarks",
                "shell.open-in",
                "shell.copy-link",
            ],
            _ => Vec::new(),
        }
    }

    /// The words a command says in ⌘K as the tab stands, when not its own (`Frame::command_words`): Copy link names
    /// what is on screen, Pin tab says Unpin on a pinned one.
    pub(super) fn command_words(&self, id: &str, cx: &App) -> Option<String> {
        match id {
            "shell.copy-link" => Some(format!("Copy link to {}", self.page_kind())),
            "shell.pin-tab" if self.shown_tab_pinned(cx) => Some("Unpin tab".to_owned()),
            _ => None,
        }
    }
}
