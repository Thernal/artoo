//! Code (#75, #165 stage 4): the project's tree beside its page — the folder page with its README rendered, or
//! files as the editor's tabs — read at a revision: the working tree by default, HEAD or a branch picked on the
//! tree's chip. A plain folder, not a git repository, has only this space, its files as they are on disk.
//!
//! Its pages (`code.file`, `code.folder`) are drawn in every tab through the shell's registry. The tab asks it
//! through `artoo_code_api::Code`, which it offers in the tab's services; it asks the tab through the
//! `artoo_code_api::Host` it finds there, and reads its file through the review's `FileView`.

mod outline;
mod revisions;
mod search;
mod space;

use artoo_shell_api::spaces::{self, Space, SpaceInfo};
use artoo_shell_api::{Action, Frame, Menu, Parts, when};
use gpui::{App, AppContext, Window};
use std::rc::Rc;

gpui::actions!(
    artoo,
    [
        /// ⌃⌘2: Code.
        GoToCode
    ]
);

/// Code's space, second in the menu, and the key that goes to it; in every tab its pages, the Outline window, and
/// the handle the tab asks it through.
pub fn install(cx: &mut App) {
    spaces::add(
        cx,
        SpaceInfo {
            space: Space(artoo_code_api::SPACE),
            label: "Code",
            icon: "folder",
            order: 20,
            go: "code.space",
        },
    );
    artoo_shell_api::add_action(
        cx,
        Action::new("code.space", GoToCode)
            .words("Code")
            .order(10)
            .key("ctrl-cmd-2", when::TAB)
            .menu(Menu::Go, "Code")
            .at(0, 1),
    );
    artoo_shell_api::add_parts(cx, parts);
}

/// Code's parts in a tab that offers it a host: its pages and the Outline window; none in one that does not.
fn parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let services = frame.services();
    let Some(host) = services.get::<Rc<dyn artoo_code_api::Host>>() else {
        return Parts::default();
    };
    let tool = outline::tool(frame.clone(), host.clone(), cx);
    let goes = frame.clone();
    let code = cx.new(|_| space::CodeSpace::new(host.clone(), frame));
    services.set::<Rc<dyn artoo_code_api::Code>>(Rc::new(space::Handle(code.downgrade())));
    Parts {
        searches: vec![search::scope(
            code.downgrade(),
            Rc::downgrade(&services),
            host,
        )],
        actions: vec![(
            Box::new(GoToCode),
            Rc::new(move |_, cx| goes.go_space(artoo_code_api::SPACE, cx)),
        )],
        lists: vec![revisions::list(code.downgrade())],
        pages: Some(space::CodeSpace::pages(&code)),
        tool: Some(tool),
        ..Default::default()
    }
}
