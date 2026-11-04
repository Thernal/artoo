//! Code (#75, #165 stage 4): the project's tree beside its page — the folder page with its README rendered, or
//! files as the editor's tabs — read at a revision: the working tree by default, HEAD or a branch picked on the
//! tree's chip. A plain folder, not a git repository, has only this space, its files as they are on disk.
//!
//! Its pages (`code.file`, `code.folder`) are drawn in every tab through the shell's registry. The tab asks it
//! through `artoo_code_api::Code`, which it offers in the tab's services; it asks the tab through the
//! `artoo_code_api::Host` it finds there, and reads its file through the review's `FileView`.

mod outline;
mod space;

use artoo_shell_api::{Frame, Parts};
use gpui::{App, AppContext, Window};
use std::rc::Rc;

/// Code in every tab: its pages, the Outline window, and the handle the tab asks it through.
pub fn install(cx: &mut App) {
    artoo_shell_api::add_parts(cx, parts);
}

/// Code's parts in a tab that offers it a host: its pages and the Outline window; none in one that does not.
fn parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let services = frame.services();
    let Some(host) = services.get::<Rc<dyn artoo_code_api::Host>>() else {
        return Parts::default();
    };
    let tool = outline::tool(frame.clone(), host.clone(), cx);
    let code = cx.new(|_| space::CodeSpace::new(host, frame));
    services.set::<Rc<dyn artoo_code_api::Code>>(Rc::new(space::Handle(code.downgrade())));
    Parts {
        pages: Some(space::CodeSpace::pages(&code)),
        tool: Some(tool),
        ..Default::default()
    }
}
