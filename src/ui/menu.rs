//! A context menu at the pointer (right click on a commit, a file or a line; the diff's View
//! menu), the tab's side: the menu is the kit's (`artoo_ui_kit::menu`); what an entry does is said where the menu
//! is made, as a closure the tab runs once the menu has closed (`Act`), so this module knows no feature.

use super::Workspace;
use crate::editor;
use artoo_ui_kit::menu::Menu;
use gpui::{AppContext, ClipboardItem, Context, Pixels, Point, Window};
use std::path::PathBuf;
use std::rc::Rc;

/// What a menu's entry does in the tab, once the menu has closed.
#[derive(Clone)]
pub struct Act(Rc<Run>);

/// What an `Act` runs.
type Run = dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>);

impl Act {
    pub fn new(
        run: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
    ) -> Self {
        Self(Rc::new(run))
    }

    /// `text` copied.
    pub fn copy(text: impl Into<String>) -> Self {
        let text = text.into();
        Self::new(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.clone())))
    }

    /// A link or a number copied, and a toast saying what (#162): `said` is `Link to !398`.
    pub fn copy_said(text: String, said: String) -> Self {
        Self::new(move |this, _, cx| this.copy_said(text.clone(), &said, cx))
    }

    /// `url` opened in the browser.
    pub fn open_url(url: impl Into<String>) -> Self {
        let url = url.into();
        Self::new(move |_, _, cx| cx.open_url(&url))
    }

    /// `path` shown in Finder.
    pub fn reveal(path: PathBuf) -> Self {
        Self::new(move |_, _, cx| cx.reveal_path(&path))
    }

    /// A part's item (`artoo_shell_api::MenuItem`), run with the window once this update is over.
    pub fn part(run: artoo_shell_api::Run) -> Self {
        Self::new(move |_, window, cx| {
            let run = run.clone();
            window.defer(cx, move |window, cx| run(window, cx));
        })
    }
}

/// A menu's entry doing one of the tab's acts.
pub type Entry = artoo_ui_kit::menu::Entry<Act>;

/// The menu's width, unless its entries need more room (#164).
pub(super) use artoo_ui_kit::menu::WIDTH;

/// A tab's context menu, its entries run by the tab.
pub(super) fn menu(cx: &mut Context<Workspace>) -> gpui::Entity<Menu<Act>> {
    let tab = cx.weak_entity();
    cx.new(|_| {
        Menu::new(Rc::new(move |act, window, cx| {
            tab.update(cx, |this, cx| this.run_act_in(act, window, cx))
                .ok();
        }))
    })
}

impl Workspace {
    pub(super) fn open_menu(
        &mut self,
        at: Point<Pixels>,
        groups: Vec<Vec<Entry>>,
        cx: &mut Context<Self>,
    ) {
        let titled = groups.into_iter().map(|g| (None, g)).collect();
        self.open_titled(at, titled, cx);
    }

    /// A menu whose groups have headings (the View menu: Changes, Show, Files, Panels).
    pub(super) fn open_titled(
        &mut self,
        at: Point<Pixels>,
        groups: Vec<(Option<&'static str>, Vec<Entry>)>,
        cx: &mut Context<Self>,
    ) {
        self.ctx_menu
            .update(cx, |m, cx| m.open(at, groups, WIDTH, cx));
    }

    pub(super) fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.ctx_menu.update(cx, |m, cx| m.close(cx));
    }

    /// Runs `act` from a menu click, the menu closed first.
    pub(super) fn run_act_in(&mut self, act: Act, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menu(cx);
        (act.0)(self, window, cx);
        cx.notify();
    }

    /// `text` copied, and a toast saying what (`said`: `Link to !398`).
    pub(super) fn copy_said(&mut self, text: String, said: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.toast_info(format!("{said} copied").into(), cx);
    }

    /// Opens `path` (relative to the repository) at `line` in the chosen editor.
    pub(super) fn open_in_editor(&mut self, path: &str, line: u32, cx: &gpui::App) {
        let (Some(root), Some(editor)) = (&self.root, self.editor(cx)) else {
            self.error = Some(
                format!(
                    "No supported editor found. Install {}, or choose one in Settings.",
                    editor::names(artoo_shell_api::editors(cx))
                )
                .into(),
            );
            return;
        };
        self.error = editor.open(&root.join(path), line).err().map(Into::into);
    }
}
