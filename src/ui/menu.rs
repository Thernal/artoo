//! A context menu at the pointer (right click on a commit, a file or a line; the diff's View
//! menu), the tab's side: the menu is the shell's (`artoo_shell::menu`), the actions its entries run
//! the tab's: copy, open in the web or the editor, toggle a view option.

use super::{Opt, Workspace};
use crate::editor;
use crate::storage::DiffMode;
use artoo_shell::menu::Menu;
use gpui::{AppContext, ClipboardItem, Context, Pixels, Point};
use std::rc::Rc;

#[derive(Clone)]
pub enum Act {
    Copy(String),
    /// A link or a number copied, and a toast saying what (#162): `said` is `Link to !398`.
    CopySaid {
        text: String,
        said: String,
    },
    OpenUrl(String),
    OpenEditor {
        path: String,
        line: u32,
    },
    Comment {
        old: bool,
        line: u32,
    },
    SetMode(DiffMode),
    Toggle(Opt),
    CreateRequest(usize),
    Annotate,
    /// Settings → Diff → Mark reviewed as I scroll past, from the View menu.
    AutoReviewed,
    /// The menu of spaces folds to icons, or opens.
    FoldPlaces,
    /// A tool window on the right opens or closes.
    Tool(super::tools::Tool),
    /// The project rail's menu (#49): move a project, show its folder, a window of its own, close.
    MoveProject {
        from: usize,
        to: usize,
    },
    Reveal(std::path::PathBuf),
    NewWindow(std::path::PathBuf),
    CloseProject(usize),
    /// The agent comment being written (#50 F, its ▾): sent alone, to a chat picked, or copied.
    AgentSendOnly,
    AgentChooseChat,
    AgentCopy,
    /// A narrow composer's folded buttons (#129): Send all, Add to review, a markdown helper,
    /// Preview.
    AgentSendAll,
    AddToReview(u64),
    ComposeHelper(crate::markdown::Helper),
    ComposePreview,
    /// The theme being edited is built on this one (its extends ▾).
    ThemeBase(String),
    /// Open in's ▾ (#78): the page's file in this app, this once; its app kept for files with
    /// this extension (or let go); Settings, where the editor is chosen.
    OpenWith(super::open_in::App),
    /// A note for the agent on a folder of Code's tree (its right click, #97).
    FolderNote(String),
    /// The card naming a project and giving it a hue (`labeling.rs`).
    NameProject(std::path::PathBuf),
    /// The requests list shows the open, the merged or the closed (#114).
    RequestView(crate::mr::MrState),
    AlwaysOpen(String),
    EditorSettings,
    /// Settings → Appearance → Fonts and size (#24): the interface's face or (`code`) the code's,
    /// None its default; Other… lists every face installed.
    Face {
        code: bool,
        face: Option<String>,
    },
    OtherFaces(bool),
    /// Set up's Code colours (#143): a code theme, None following the window.
    CodeTheme(Option<String>),
    /// A reason a request cannot merge, where it is fixed (#164).
    MergeFix(crate::merge::Fix),
    /// A part's item (`artoo_shell_api::MenuItem`), run once the menu has closed.
    Part(artoo_shell_api::Run),
}

/// A menu's entry doing one of the tab's acts.
pub type Entry = artoo_shell::menu::Entry<Act>;

/// The menu's width, unless its entries need more room (#164).
pub(super) use artoo_shell::menu::WIDTH;

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

    pub(super) fn run_act(&mut self, act: Act, cx: &mut Context<Self>) {
        self.close_menu(cx);
        match act {
            Act::Copy(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
            Act::CopySaid { text, said } => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.toast_info(format!("{said} copied").into(), cx);
            }
            Act::OpenUrl(url) => cx.open_url(&url),
            Act::OpenEditor { path, line } => self.open_in_editor(&path, line),
            Act::Comment { old, line } => self.start_comment(old, line, cx),
            Act::SetMode(mode) => self.set_mode(mode, cx),
            Act::Toggle(opt) => self.toggle(opt, cx),
            Act::CreateRequest(ix) => self.open_create(Some(ix), cx),
            Act::Annotate => self.toggle_annotate(cx),
            Act::AutoReviewed => {
                self.settings.auto_reviewed = !self.settings.auto_reviewed;
                self.save_settings(cx);
            }
            Act::FoldPlaces => self.toggle_menu(cx),
            Act::Tool(tool) => self.toggle_tool(tool, cx),
            Act::Reveal(path) => cx.reveal_path(&path),
            Act::NewWindow(path) => cx.defer(move |cx| super::open_new_window(path, cx)),
            Act::AgentSendOnly => self.add_and_send(true, cx),
            Act::AgentChooseChat => self.add_and_pick_chat(cx),
            Act::AgentCopy => self.add_and_copy(cx),
            Act::AgentSendAll => self.add_and_send(false, cx),
            Act::AddToReview(iid) => self.post_comment(iid, true, cx),
            Act::ComposeHelper(helper) => self.apply_helper(helper, cx),
            Act::ComposePreview => self.toggle_preview(cx),
            Act::ThemeBase(id) => self.set_theme_base(&id, cx),
            Act::OpenWith(app) => self.open_with_app(app, cx),
            Act::RequestView(state) => self.set_request_view(state, cx),
            Act::FolderNote(path) => {
                self.open_folder_page(path.clone(), cx);
                self.start_folder_comment(&path, cx);
            }
            Act::NameProject(path) => self.open_label(super::labeling::Target::Project(path), cx),
            Act::AlwaysOpen(ext) => self.always_open(ext, cx),
            Act::EditorSettings => self.open_settings(cx),
            Act::Face { code, face } => self.choose_face(code, face, cx),
            Act::OtherFaces(code) => self.open_all_faces(code, cx),
            Act::CodeTheme(id) => self.choose_code_theme(id, cx),
            Act::MergeFix(fix) => self.go_fix(fix, cx),
            // Need the window: `run_act_in` runs them.
            Act::MoveProject { .. } | Act::CloseProject(_) | Act::Part(_) => {}
        }
        cx.notify();
    }

    /// Runs `act` from a menu click, where the window is at hand for what the shell does.
    pub(super) fn run_act_in(
        &mut self,
        act: Act,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        match act {
            Act::MoveProject { from, to } => {
                self.close_menu(cx);
                self.with_shell(window, cx, move |s, _, cx| s.move_tab(from, to, cx));
                cx.notify();
            }
            Act::CloseProject(ix) => {
                self.close_menu(cx);
                self.with_shell(window, cx, move |s, window, cx| s.close(ix, window, cx));
                cx.notify();
            }
            Act::Part(run) => {
                self.close_menu(cx);
                window.defer(cx, move |window, cx| run(window, cx));
                cx.notify();
            }
            act => self.run_act(act, cx),
        }
    }

    /// Opens `path` (relative to the repository) at `line` in the chosen editor.
    pub(super) fn open_in_editor(&mut self, path: &str, line: u32) {
        let (Some(root), Some(editor)) = (&self.root, editor::pick(self.settings.editor)) else {
            self.error = Some(
                "No supported editor found. Install Android Studio, Zed or VS Code, or choose one in Settings."
                    .into(),
            );
            return;
        };
        self.error = editor.open(&root.join(path), line).err().map(Into::into);
    }
}
