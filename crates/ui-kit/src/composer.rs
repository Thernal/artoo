//! The composer (#9, #51, #129, #168): a comment being written — under a line of the diff, on a page, a file or a
//! folder in Code, for the agent, a merge request, a reply or an edit — and its own editing: completion, markdown
//! helpers, preview —, the one input component it is drawn in and a new comment's foot (who sees it, the ways to
//! send it). Who it goes to, where it is drawn, its keys routed and what it writes sent are the tab's.

use crate::complete::Choice;
use crate::discussion::{self, Kind, Source};
use crate::emoji_picker::{self, Picker};
use crate::input::{self, Edit};
use crate::{hover, icons, markdown, px, theme};
use artoo_core::markdown::Helper;
use artoo_core::textedit::TextEdit;
use gpui::{AnyElement, App, Div, Keystroke, Pixels, Point, Rgba, SharedString, div, prelude::*};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// ☺'s tooltip: ⌃⌘E on macOS, where it is bound; elsewhere those keys are ⇧⌘E's (code), and it has none.
const EMOJI_TIP: &str = if artoo_core::platform::MAC {
    "Emoji  ⌃⌘E"
} else {
    "Emoji"
};

/// Who a comment is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    /// Kept on this Mac and copied for a coding agent.
    Agent,
    /// Posted to a merge request on GitLab, where everyone on it sees it.
    Request(u64),
    /// A note on request `0`'s page for the agent, kept on this Mac (#241): on its description's paragraph `1`,
    /// counted from 1, or with 0 on the request.
    Overview(u64, u32),
    /// A reply to discussion number `0` of the open merge request.
    Reply(usize),
    /// A new text for one's own comment.
    Edit(Change),
    /// A purple note on discussion `0` of the request, kept on this Mac for the agent (#50 G).
    ThreadNote(usize),
    /// The reviewer's note answering the agent's question on comment `0` (#10).
    Answer(u64),
    /// Text's own value on the agent's form of comment `0` (#10 C).
    AnswerValue(u64),
    /// Written in the Inbox: a reply to the selected notice's discussion, or a comment on its
    /// request when it has none (`Workspace::inbox_view.reply`).
    InboxReply,
}

/// The comment an edit changes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Change {
    /// A comment for the agent, by its id.
    Agent(u64),
    /// Note `note` of discussion number `thread` of the open merge request.
    Note { thread: usize, note: u64 },
    /// One's pending comment `0` on the open merge request, before the review is sent (#200).
    Draft(u64),
    /// The description of request `0`, edited in place (Design round 46).
    Description(u64),
}

/// A comment being written.
pub struct Compose {
    pub path: String,
    pub old: bool,
    /// The first line of a comment on several lines; it sits under its last, `line`.
    pub first: Option<u32>,
    pub line: u32,
    pub code: String,
    /// The text, its caret and selection.
    pub edit: TextEdit,
    pub at: String,
    pub target: Target,
    /// Why the last attempt to post failed.
    pub error: Option<String>,
    /// The text is shown formatted instead of as written (⌘⇧P); typing goes on and the preview
    /// follows it.
    pub preview: bool,
    /// The completion list's selected row, and where a list was closed with esc (#51, #9).
    pub pick: usize,
    pub closed: Option<usize>,
    /// Written on a document read as a page: its file in the diff, the composer under the block
    /// (#11).
    pub page: Option<usize>,
    /// Written on a file read in Code, at its commit (None: the working tree) — a note on a plain
    /// line, under it in the file (#75).
    pub file: Option<Option<git2::Oid>>,
    /// The agent's note it answers, carried into the comment (#151).
    pub answers: Option<String>,
    /// What is attached to it (#214), each a tile under the text.
    pub attachments: Vec<Attachment>,
    /// The emoji picker, open over its ☺ (#240).
    pub emoji: Option<Picker>,
}

/// A file attached to a comment being written (#214): a screenshot pasted, a file dropped on the composer or picked
/// with its clip. The comment's owner uploads it and says how it goes (`Compose::upload_progress`, `uploaded`), and its Markdown goes into the text where the
/// caret was when it was attached.
#[derive(Clone, Debug, PartialEq)]
pub struct Attachment {
    pub id: u64,
    pub path: PathBuf,
    pub name: String,
    /// Its size, in bytes.
    pub size: u64,
    /// Where the caret was: its Markdown goes there.
    pub at: usize,
    pub state: Upload,
}

/// How far an attachment's upload has come.
#[derive(Clone, Debug, PartialEq)]
pub enum Upload {
    /// Going, how far (0 to 1).
    Going(f32),
    /// Done: the Markdown put into the text.
    Done(String),
    /// Failed, and why: its tile amber, `Try again`.
    Failed(String),
}

impl Attachment {
    /// Whether it is a picture: its tile shows it.
    pub fn image(&self) -> bool {
        let ext = (self.path.extension())
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff"
        )
    }

    /// `182 KB · 60%` while it goes, `182 KB` once done.
    pub fn said(&self) -> String {
        let size = size_words(self.size);
        match &self.state {
            Upload::Going(p) => format!("{size} · {}%", (p * 100.).round() as u32),
            _ => size,
        }
    }
}

/// `182 KB`, `1.4 MB`.
pub fn size_words(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{} KB", (bytes + 512) / 1024),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.),
    }
}

/// ⌘V (Ctrl+V elsewhere) with a picture or files on the clipboard and no text: a screenshot. The picture saved under
/// `dir` (`pasted-<time>.png`), or the files copied in Finder, to attach.
pub fn pasted_files(key: &Keystroke, dir: &Path, cx: &App) -> Vec<PathBuf> {
    let m = key.modifiers;
    if key.key != "v" || !m.secondary() || m.number_of_modifiers() != 1 {
        return Vec::new();
    }
    let Some(item) = cx.read_from_clipboard() else {
        return Vec::new();
    };
    let mut files = Vec::new();
    let mut text = false;
    for entry in item.entries() {
        match entry {
            gpui::ClipboardEntry::String(s) => text |= !s.text().trim().is_empty(),
            gpui::ClipboardEntry::ExternalPaths(paths) => {
                files.extend(paths.paths().iter().cloned())
            }
            gpui::ClipboardEntry::Image(image) if files.is_empty() => {
                let ext = match image.format {
                    gpui::ImageFormat::Jpeg => "jpg",
                    gpui::ImageFormat::Webp => "webp",
                    gpui::ImageFormat::Gif => "gif",
                    gpui::ImageFormat::Svg => "svg",
                    gpui::ImageFormat::Bmp => "bmp",
                    gpui::ImageFormat::Tiff => "tiff",
                    _ => "png",
                };
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis());
                let path = dir.join(format!("pasted-{now}.{ext}"));
                if std::fs::create_dir_all(dir).is_ok()
                    && std::fs::write(&path, &image.bytes).is_ok()
                {
                    files.push(path);
                }
            }
            gpui::ClipboardEntry::Image(_) => {}
        }
    }
    // Text with it (a copied link to a picture, a cell): the text is what was meant.
    if text && files.iter().all(|f| f.starts_with(dir)) {
        for f in files.iter().filter(|f| f.starts_with(dir)) {
            let _ = std::fs::remove_file(f);
        }
        return Vec::new();
    }
    files
}

impl Compose {
    /// `path` attached where the caret is (#214); its id, by which its owner says how its upload goes.
    pub fn attach(&mut self, path: PathBuf) -> u64 {
        let id = self.attachments.iter().map(|a| a.id).max().unwrap_or(0) + 1;
        let name = (path.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.attachments.push(Attachment {
            id,
            size: std::fs::metadata(&path).map_or(0, |m| m.len()),
            path,
            name,
            at: self.edit.caret(),
            state: Upload::Going(0.),
        });
        id
    }

    /// Attachment `id`'s upload has gone `p` of the way.
    pub fn upload_progress(&mut self, id: u64, p: f32) {
        if let Some(a) = self.attachments.iter_mut().find(|a| a.id == id)
            && matches!(a.state, Upload::Going(_))
        {
            a.state = Upload::Going(p.clamp(0., 1.));
        }
    }

    /// Attachment `id` uploaded: `markdown` put where the caret was when it was attached, on a line of its own.
    pub fn uploaded(&mut self, id: u64, markdown: String) {
        let Some(ix) = self.attachments.iter().position(|a| a.id == id) else {
            return;
        };
        let text = self.edit.text().to_owned();
        let mut at = self.attachments[ix].at.min(text.len());
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        let mut insert = markdown.clone();
        if at > 0 && !text[..at].ends_with('\n') {
            insert.insert(0, '\n');
        }
        if at < text.len() && !text[at..].starts_with('\n') {
            insert.push('\n');
        }
        let caret = self.edit.caret();
        let caret = if caret >= at {
            caret + insert.len()
        } else {
            caret
        };
        self.edit.replace(at..at, &insert, caret..caret);
        for other in self
            .attachments
            .iter_mut()
            .filter(|a| a.id != id && a.at >= at)
        {
            other.at += insert.len();
        }
        self.attachments[ix].state = Upload::Done(markdown);
    }

    /// Attachment `id`'s upload failed: its tile amber, `Try again`.
    pub fn upload_failed(&mut self, id: u64, why: String) {
        if let Some(a) = self.attachments.iter_mut().find(|a| a.id == id) {
            a.state = Upload::Failed(why);
        }
    }

    /// Attachment `id` tried again: its file, to upload again.
    pub fn retry(&mut self, id: u64) -> Option<PathBuf> {
        let a = self.attachments.iter_mut().find(|a| a.id == id)?;
        a.state = Upload::Going(0.);
        Some(a.path.clone())
    }

    /// ✕ on attachment `id`: its tile goes, and its Markdown out of the text.
    pub fn unattach(&mut self, id: u64) {
        let Some(ix) = self.attachments.iter().position(|a| a.id == id) else {
            return;
        };
        if let Upload::Done(markdown) = &self.attachments[ix].state
            && let Some(at) = self.edit.text().find(markdown.as_str())
        {
            let text = self.edit.text();
            let end = at + markdown.len();
            let end = if text[end..].starts_with('\n') {
                end + 1
            } else {
                end
            };
            let caret = self.edit.caret();
            let caret = if caret >= end {
                caret - (end - at)
            } else {
                caret.min(at)
            };
            self.edit.replace(at..end, "", caret..caret);
        }
        self.attachments.remove(ix);
    }

    /// An upload still going or failed: `Comment` waits for it, so no comment goes out without its image.
    pub fn waiting(&self) -> bool {
        (self.attachments.iter()).any(|a| !matches!(a.state, Upload::Done(_)))
    }

    /// The request at the end of whose Overview it is written, for everyone on it or for the agent (#241).
    pub fn overview(&self) -> Option<u64> {
        match self.target {
            Target::Overview(iid, 0) => Some(iid),
            Target::Request(iid)
                if self.path.is_empty() && self.page.is_none() && self.file.is_none() =>
            {
                Some(iid)
            }
            _ => None,
        }
    }

    /// The discussion of the open request a reply, an edit or a note for the agent is in.
    pub fn thread(&self) -> Option<usize> {
        match self.target {
            Target::Reply(ix) | Target::ThreadNote(ix) => Some(ix),
            Target::Edit(Change::Note { thread, .. }) => Some(thread),
            _ => None,
        }
    }

    /// That discussion moved to `ix` in the list.
    pub fn set_thread(&mut self, ix: usize) {
        match &mut self.target {
            Target::Reply(i) | Target::ThreadNote(i) => *i = ix,
            Target::Edit(Change::Note { thread, .. }) => *thread = ix,
            _ => {}
        }
    }

    /// ↑ ↓ ↵ ⇥ esc while the list of choices for the token at `at` is open: None when it did not take the key,
    /// else whether the text changed.
    pub fn completion_key(
        &mut self,
        key: &Keystroke,
        at: usize,
        choices: &[Choice],
    ) -> Option<bool> {
        if key.modifiers.number_of_modifiers() > 0 {
            return None;
        }
        match key.key.as_str() {
            "down" => self.pick = (self.pick + 1).min(choices.len() - 1),
            "up" => self.pick = self.pick.saturating_sub(1),
            "enter" | "tab" => {
                let choice = &choices[self.pick.min(choices.len() - 1)];
                let caret = self.edit.caret();
                let end = at + choice.insert.len();
                self.edit.replace(at..caret, &choice.insert, end..end);
                self.pick = 0;
                return Some(true);
            }
            "escape" => self.closed = Some(at),
            _ => return None,
        }
        Some(false)
    }

    /// ⌘B ⌘I ⇧⌘X ⇧⌘E ⇧⌘K and ⇧⌘P (Ctrl on Windows and Linux), no other modifier.
    pub fn shortcut(key: &Keystroke) -> Option<Short> {
        let m = key.modifiers;
        if !m.secondary() || m.number_of_modifiers() != 1 + u8::from(m.shift) {
            return None;
        }
        match (key.key.as_str(), m.shift) {
            ("b", false) => Some(Short::Helper(Helper::Bold)),
            ("i", false) => Some(Short::Helper(Helper::Italic)),
            ("x", true) => Some(Short::Helper(Helper::Strike)),
            ("e", true) => Some(Short::Helper(Helper::Code)),
            ("k", true) => Some(Short::Helper(Helper::Link)),
            ("p", true) => Some(Short::Preview),
            _ => None,
        }
    }

    /// A markdown helper of the toolbar, on the selection or at the caret.
    pub fn apply(&mut self, helper: Helper) {
        artoo_core::markdown::apply(&mut self.edit, helper);
    }

    /// ☺ (#240): the emoji picker opens, its search empty.
    pub fn open_emoji(&mut self) {
        self.emoji = Some(Picker::default());
    }

    /// Emoji `name` where the caret is, or over the selection, as one step ⌘Z takes back; one the list lacks
    /// (typed `:name:`) as written. The picker closes.
    pub fn put_emoji(&mut self, name: &str) {
        let glyph = match artoo_core::emoji::get(name) {
            Some(glyph) => glyph.to_owned(),
            None => format!(":{name}:"),
        };
        let at = self
            .edit
            .selection()
            .unwrap_or_else(|| self.edit.caret()..self.edit.caret());
        let end = at.start + glyph.len();
        self.edit.replace(at, &glyph, end..end);
        self.emoji = None;
    }

    /// ⌃⌘E, ☺'s key: on macOS alone (`EMOJI_TIP`).
    pub fn emoji_key(key: &Keystroke) -> bool {
        let m = key.modifiers;
        artoo_core::platform::MAC
            && key.key == "e"
            && m.control
            && m.platform
            && m.number_of_modifiers() == 2
    }

    /// The text shown formatted, or as written.
    pub fn toggle_preview(&mut self) {
        self.preview = !self.preview;
    }

    /// `text` put at the caret (a reply that mentions the author).
    pub fn insert(&mut self, text: &str) {
        self.edit.insert(text);
    }

    /// A click in the text: the caret goes there; ⇧ extends the selection, a double click takes the word; a drag
    /// (`drag`) selects.
    pub fn place(&mut self, at: usize, shift: bool, clicks: usize, drag: bool, lower: bool) {
        self.edit.click(at, clicks, shift || drag);
        if clicks == 1 && !shift && !drag {
            self.edit.set_downstream(lower);
        }
    }

    /// Key `key` on the text: ↵ a new line that keeps a list going, ⇧↵ a plain one.
    pub fn edit_key(&mut self, key: &Keystroke, cx: &mut App) -> Typed {
        match input::edit_text(&mut self.edit, key, cx) {
            // Written in, it keeps what was written and lets the keyboard go (#168).
            Edit::Escape if !self.edit.text().trim().is_empty() => Typed::Leave,
            Edit::Escape => Typed::Cancel,
            Edit::Ignored => Typed::Ignored,
            Edit::Enter { shift: true } => {
                self.edit.insert("\n");
                Typed::Changed
            }
            Edit::Enter { shift: false } => {
                artoo_core::markdown::newline(&mut self.edit);
                Typed::Changed
            }
            Edit::Changed | Edit::Selected => Typed::Changed,
        }
    }
}

/// A key of the composer's own beside its text (`Compose::shortcut`).
pub enum Short {
    Helper(Helper),
    /// ⇧⌘P: the text formatted or as written.
    Preview,
}

/// What a key did to the text (`Compose::edit_key`).
pub enum Typed {
    /// Esc on text written: it is kept, the keyboard let go.
    Leave,
    /// Esc on nothing written: the comment goes.
    Cancel,
    /// Not a key of the text.
    Ignored,
    /// The text, its caret or selection changed: the box may grow.
    Changed,
}

/// What a press in the composer asks of the tab (`Compose::render_input`).
#[derive(Clone)]
pub enum ComposeAct {
    /// A markdown helper of its toolbar.
    Helper(Helper),
    /// The text formatted, or as written.
    Preview,
    /// The folded helpers' menu, at a window position: `tools`, then Preview.
    Format {
        at: Point<Pixels>,
        tools: Vec<Helper>,
    },
    /// A press or a drag in the text (`Compose::place`).
    Place {
        at: usize,
        shift: bool,
        clicks: usize,
        drag: bool,
        lower: bool,
    },
    /// The clip (`Attach a file or an image ⇧⌘A`): files to pick (#214).
    Clip,
    /// Files dropped on it (#214).
    Dropped(Vec<PathBuf>),
    /// ✕ on attachment `0` (`Compose::unattach`).
    Unattach(u64),
    /// `Try again` on attachment `0` (`Compose::retry`).
    Retry(u64),
    /// ☺ (#240): the emoji picker opens.
    Emoji,
    /// The emoji picker picked this name: it goes into the text.
    PutEmoji(String),
    /// A press outside the emoji picker.
    CloseEmoji,
    /// A change to the emoji picker (a click in its search).
    EmojiPicker(emoji_picker::Edit),
}

/// What answers the composer's presses.
pub type ComposeActs = Rc<dyn Fn(ComposeAct, &mut App)>;

/// How the tab has the composer drawn: who sees it, what is along its edge, what it owns of the window.
pub struct Look {
    /// The outline, in the audience's colour.
    pub tone: Rgba,
    pub placeholder: SharedString,
    /// Who reads it, a row over the text (#200: the `To:` chip).
    pub head: Option<AnyElement>,
    /// Who sees it, first along the bottom edge.
    pub lead: Option<AnyElement>,
    /// The markdown helpers, after `lead`.
    pub tools: Vec<Helper>,
    /// The buttons, at the right end.
    pub tail: Vec<AnyElement>,
    /// The helpers fold into one menu (`Room::S`).
    pub folded: bool,
    /// It has the keyboard: the caret shows.
    pub focused: bool,
    /// Where the text's layout is kept for ↑ ↓ by the lines as drawn, unless a card owns the arrows.
    pub lines: Option<input::Lines>,
    /// The completion list, when one is open for it.
    pub completion: Option<Div>,
    /// What measures its width, for its buttons' room.
    pub measure: AnyElement,
    /// Where a relative link in the preview points.
    pub link_base: Option<String>,
    /// Its owner takes files (#214): the clip among its tools, a paste or a drop attaches. Not on a GitHub request.
    pub clip: bool,
}

impl Compose {
    /// The one input component, open: one outline in the audience's colour, the text (or its
    /// preview), and along its bottom edge `lead`, the markdown `tools`, Preview, then `tail`. The tab wraps it in
    /// the box that gives it the keyboard.
    pub fn render_input(&self, look: Look, act: ComposeActs, cx: &App) -> Div {
        let Look {
            tone,
            placeholder,
            head,
            lead,
            tools,
            tail,
            folded,
            focused,
            lines,
            completion,
            measure,
            link_base,
            clip,
        } = look;
        let compose = self;
        let text = if compose.preview {
            div()
                .min_h(px(66.))
                .px(px(10.))
                .py(px(7.))
                .child(if compose.edit.text().trim().is_empty() {
                    div().text_color(theme::muted()).child("Nothing to preview")
                } else {
                    div().child(markdown::body(
                        compose.edit.text(),
                        true,
                        "md-compose",
                        link_base.as_deref(),
                        None,
                        cx,
                    ))
                })
                .into_any_element()
        } else {
            self.render_text(placeholder, true, focused, lines, act.clone())
        };
        let tool = |h: Helper, act: ComposeActs| {
            let (icon, tip) = markdown::helper_look(h);
            div()
                .id(("md-tool", h as usize))
                .size(px(26.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .child(icons::icon(icon).size(px(15.)).text_color(theme::muted()))
                .tooltip_show_delay(hover::LONG)
                .tooltip(move |_, cx| cx.new(|_| hover::Tip(tip)).into())
                .on_click(move |_, _, cx| act(ComposeAct::Helper(h), cx))
        };
        // ☺ after List (#240): the reactions' picker, over its button — at S over T, which holds the key's row.
        let mut popup = compose.emoji.as_ref().map(|picker| {
            let (pick, close, change) = (act.clone(), act.clone(), act.clone());
            let popup = picker.render(emoji_picker::Draw {
                id: u64::MAX,
                // Text takes every emoji, on either host.
                github: false,
                keys: "↵ puts it in · esc",
                pick: Rc::new(move |name, cx| pick(ComposeAct::PutEmoji(name), cx)),
                close: Rc::new(move |cx| close(ComposeAct::CloseEmoji, cx)),
                change: Rc::new(move |edit, cx| change(ComposeAct::EmojiPicker(edit), cx)),
            });
            div().absolute().left_0().top_0().size_0().child(
                gpui::deferred(
                    gpui::anchored()
                        .anchor(gpui::Anchor::BottomLeft)
                        .offset(gpui::point(px(0.), px(-6.)))
                        .snap_to_window_with_margin(px(8.))
                        .child(popup),
                )
                .with_priority(1),
            )
        });
        let open = popup.is_some();
        let smile = |popup: Option<Div>, act: ComposeActs| {
            let button = div()
                .id("compose-emoji")
                .size(px(26.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .when(open, |s| s.bg(theme::selected()))
                .hover(|s| s.bg(theme::hover()))
                .child(icons::icon("smile").size(px(15.)).text_color(if open {
                    theme::text()
                } else {
                    theme::muted()
                }))
                .tooltip_show_delay(hover::LONG)
                .tooltip(|_, cx| cx.new(|_| hover::Tip(EMOJI_TIP)).into())
                // On the press: an open picker has just closed itself on it, and stays closed.
                .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    if !open {
                        act(ComposeAct::Emoji, cx);
                    }
                });
            div().relative().flex_none().children(popup).child(button)
        };
        let preview = compose.preview;
        let toggle = act.clone();
        // The clip, last among the tools (#214): a file or an image picked; a paste or a drop does the same.
        let clip_tool = clip.then(|| {
            let act = act.clone();
            div()
                .id("compose-clip")
                .size(px(26.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .child(
                    icons::icon("paperclip")
                        .size(px(15.))
                        .text_color(theme::muted()),
                )
                .tooltip_show_delay(hover::LONG)
                .tooltip(|_, cx| {
                    cx.new(|_| hover::Tip("Attach a file or an image  ⇧⌘A; paste or drop one"))
                        .into()
                })
                .on_click(move |_, _, cx| act(ComposeAct::Clip, cx))
        });
        // One row at every width (Design shell round 6, #129): who sees it and the writing helpers
        // on the left, the buttons on the right; at S the helpers fold into T, a menu of them.
        let (open_format, format) = (act.clone(), tools.clone());
        let helpers = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.))
            .children(lead.map(|lead| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .mr(px(4.))
                    .child(lead)
                    .children((!folded).then(|| div().w(px(1.)).h(px(14.)).bg(theme::selected())))
            }))
            .children(folded.then(|| {
                div()
                    .id("md-format")
                    .size(px(26.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(preview, |s| s.bg(theme::selected()))
                    .hover(|s| s.bg(theme::hover()))
                    .child(icons::icon("type").size(px(15.)).text_color(theme::muted()))
                    .tooltip_show_delay(hover::LONG)
                    .tooltip(|_, cx| {
                        cx.new(|_| hover::Tip("Format: bold, code, link, list, emoji, preview"))
                            .into()
                    })
                    .on_click(move |event, _, cx| {
                        let (at, tools) = (event.position(), format.clone());
                        open_format(ComposeAct::Format { at, tools }, cx);
                    })
            }))
            .children((folded && open).then(|| div().relative().flex_none().children(popup.take())))
            .when(!folded, |mut s| {
                for &h in &tools {
                    s = s.child(tool(h, act.clone()));
                    if h == Helper::Bullet {
                        s = s.child(smile(popup.take(), act.clone()));
                    }
                }
                s
            })
            .when(!folded, |s| {
                s.child(
                    div()
                        .id("md-preview")
                        .size(px(26.))
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .when(preview, |s| s.bg(theme::selected()))
                        .hover(|s| s.bg(theme::hover()))
                        .child(icons::icon("eye").size(px(15.)).text_color(if preview {
                            theme::text()
                        } else {
                            theme::muted()
                        }))
                        .tooltip_show_delay(hover::LONG)
                        .tooltip(|_, cx| cx.new(|_| hover::Tip("Preview  ⌘⇧P")).into())
                        .on_click(move |_, _, cx| toggle(ComposeAct::Preview, cx)),
                )
            })
            .children(clip_tool);
        let edge = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(6.))
            .pt(px(4.))
            .pb(px(6.))
            .child(helpers)
            .child(div().flex_1().min_w_0())
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(6.))
                    .children(tail),
            );
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .rounded(px(8.))
            .border_1()
            .border_color(tone)
            .bg(theme::base())
            .overflow_hidden()
            .font_family(theme::ui_font())
            .text_size(px(13.))
            .line_height(px(20.))
            .text_color(theme::text())
            // Its own width decides its buttons' (the tab's `compose_narrow`).
            .child(measure)
            .children(head.map(|head| {
                div()
                    .flex()
                    .items_center()
                    .min_w_0()
                    .px(px(6.))
                    .pt(px(6.))
                    .child(head)
            }))
            .child(text)
            .children(self.tiles(act.clone()))
            .children(completion)
            .children(compose.error.clone().map(|e| {
                div()
                    .px(px(10.))
                    .pb(px(4.))
                    .text_size(px(12.))
                    .text_color(theme::removed())
                    .child(e)
            }))
            .child(edge)
            // A file dropped on it attaches (#214).
            .when(clip, |s| {
                let act = act.clone();
                s.on_drop(move |paths: &gpui::ExternalPaths, _, cx| {
                    act(ComposeAct::Dropped(paths.paths().to_vec()), cx)
                })
                .drag_over::<gpui::ExternalPaths>(|s, _, _, _| s.border_color(theme::focus()))
            })
    }

    /// Its attachments, a row of tiles under the text (#214): a thumbnail for an image, the name, `182 KB · 60%`
    /// and an upload bar, ✕ to remove it. A failed one amber, with `Try again`.
    fn tiles(&self, act: ComposeActs) -> Option<Div> {
        if self.attachments.is_empty() {
            return None;
        }
        let tile = |a: &Attachment| {
            let failed = matches!(a.state, Upload::Failed(_));
            let id = a.id;
            let thumb = if a.image() {
                gpui::img(a.path.clone())
                    .w(px(64.))
                    .h(px(40.))
                    .rounded(px(4.))
                    .object_fit(gpui::ObjectFit::Cover)
                    .into_any_element()
            } else {
                div()
                    .w(px(40.))
                    .h(px(40.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .bg(theme::hover())
                    .child(icons::file_icon(&a.name, 18.))
                    .into_any_element()
            };
            let bar = match &a.state {
                Upload::Going(p) => Some(
                    div()
                        .h(px(3.))
                        .rounded(px(2.))
                        .bg(theme::selected())
                        .overflow_hidden()
                        .child(
                            div()
                                .h_full()
                                .w(gpui::relative(p.max(0.02)))
                                .bg(theme::accent()),
                        ),
                ),
                _ => None,
            };
            let about = div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .min_w(px(120.))
                .max_w(px(220.))
                .text_size(px(11.))
                .line_height(px(14.))
                .child(
                    hover::cut(("compose-tile-name", id as usize), a.name.clone())
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme::text()),
                )
                .child(match &a.state {
                    Upload::Failed(why) => hover::cut(
                        ("compose-tile-why", id as usize),
                        format!("{} · {why}", size_words(a.size)),
                    )
                    .text_color(theme::warning()),
                    _ => div()
                        .id(("compose-tile-size", id as usize))
                        .text_color(theme::faint())
                        .child(a.said()),
                })
                .children(bar);
            let button =
                |key: &'static str, icon: &'static str, tip: &'static str, what: ComposeAct| {
                    let act = act.clone();
                    div()
                        .id((key, id as usize))
                        .size(px(22.))
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::hover()))
                        .child(icons::icon(icon).size(px(13.)).text_color(theme::muted()))
                        .tooltip_show_delay(hover::LONG)
                        .tooltip(move |_, cx| cx.new(|_| hover::Tip(tip)).into())
                        .on_click(move |_, _, cx| act(what.clone(), cx))
                };
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(8.))
                .p(px(4.))
                .rounded(px(6.))
                .border_1()
                .border_color(if failed {
                    theme::warning()
                } else {
                    theme::border()
                })
                .bg(if failed {
                    theme::warning_bg()
                } else {
                    theme::panel()
                })
                .child(thumb)
                .child(about)
                .children(failed.then(|| {
                    button(
                        "compose-retry",
                        "refresh",
                        "Try again",
                        ComposeAct::Retry(id),
                    )
                }))
                .child(button(
                    "compose-unattach",
                    "x",
                    "Remove",
                    ComposeAct::Unattach(id),
                ))
        };
        Some(
            div()
                .flex()
                .flex_wrap()
                .gap(px(8.))
                .px(px(10.))
                .pb(px(6.))
                .children(self.attachments.iter().map(tile)),
        )
    }

    /// The composer's text as a field: the text wrapping in its box, the selection tinted, the
    /// caret drawn where the text's layout puts it; a click places the caret, a drag selects.
    fn render_text(
        &self,
        placeholder: SharedString,
        compact: bool,
        focused: bool,
        lines: Option<input::Lines>,
        act: ComposeActs,
    ) -> AnyElement {
        let compose = self;
        // A reply grows out of its one-line field, so its text starts where the field's did.
        let (pad_x, pad_y, min_h) = if compact {
            (10., 6., 64.)
        } else {
            (12., 10., 88.)
        };
        use gpui::{Bounds, HighlightStyle, MouseButton, StyledText, canvas, fill, point, size};
        let text = compose.edit.text();
        // A trailing space gives the caret a place on a last, empty line.
        let styled = StyledText::new(format!("{text} ")).with_highlights(
            compose.edit.selection().map(|sel| {
                (
                    sel,
                    HighlightStyle {
                        background_color: Some(theme::selection().into()),
                        ..Default::default()
                    },
                )
            }),
        );
        let layout = styled.layout().clone();
        if let Some(lines) = lines {
            *lines.borrow_mut() = Some(layout.clone());
        }
        let (for_caret, for_down, for_move) = (layout.clone(), layout.clone(), layout);
        let (caret, downstream) = (compose.edit.caret(), compose.edit.downstream());
        let (down, drag) = (act.clone(), act);
        div()
            .relative()
            .min_h(px(min_h))
            .px(px(pad_x))
            .py(px(pad_y))
            .bg(theme::base())
            .cursor_text()
            .child(div().w_full().min_w_0().child(styled))
            .children(text.is_empty().then(|| {
                div()
                    .absolute()
                    .top(px(pad_y))
                    .left(px(pad_x + 2.))
                    // Bounded on the right, so a long hint wraps rather than being cut (#129).
                    .right(px(pad_x))
                    .text_color(theme::muted())
                    .child(placeholder)
            }))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        if !focused {
                            return;
                        }
                        let at = input::caret_point(&for_caret, caret, downstream)
                            .unwrap_or(bounds.origin + point(px(pad_x), px(pad_y)));
                        window.paint_quad(fill(
                            Bounds::new(at, size(px(2.), px(20.))),
                            theme::accent(),
                        ));
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                let at = input::nearest_index(&for_down, event.position);
                let (shift, clicks) = (event.modifiers.shift, event.click_count);
                let lower = input::lower_line(&for_down, at, event.position.y);
                let drag = false;
                down(
                    ComposeAct::Place {
                        at,
                        shift,
                        clicks,
                        drag,
                        lower,
                    },
                    cx,
                );
            })
            .on_mouse_move(move |event, _, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    let at = input::nearest_index(&for_move, event.position);
                    let place = ComposeAct::Place {
                        at,
                        shift: true,
                        clicks: 1,
                        drag: true,
                        lower: false,
                    };
                    drag(place, cx);
                }
            })
            .into_any_element()
    }
}

/// Who reads a new comment, as its `To:` chip says it (#200).
pub struct Audience {
    /// In words: `Everyone on !398`, a chat's name, `Your agent`.
    pub words: String,
    /// A chat's letter on its hue; `None`, the request's icon (everyone on it) or the agent's purple.
    pub mark: Option<(char, Rgba)>,
    /// A request's page: both segments show, a click on the one not lit switches to it, as ⇥ does.
    pub both: bool,
    /// ⇥ switches who reads it: a new comment to the branch's chat, a reply to a note for your agent, and back.
    pub switch: bool,
}

/// What a comment's foot is drawn with that the tab knows.
pub struct FootLook {
    /// Who reads it: the `To:` chip over the text (#200).
    pub audience: Audience,
    /// The box's room (#129).
    pub room: Room,
    /// The comment can wait for the review (Add to review): not one on a whole request on GitHub.
    pub review: bool,
}

/// A click on a comment's foot.
#[derive(Clone, Copy)]
pub enum FootEvent {
    Cancel,
    /// The `To:` chip's ▾ at a point: who can read it (#200).
    Agent,
    /// The request's segment of the audience: everyone on it reads the comment.
    Everyone,
    /// Added to the review, pending until it is submitted: a comment on a line, a reply (#200).
    AddToReview,
    /// The agent's ▾ at a point: the chat, Copy (#50 F).
    More(Point<Pixels>),
    /// The primary: added for the agent, or posted.
    Add,
    /// The primary's own ▾ at S, at a point: what folded into it.
    Folded(Point<Pixels>),
}

/// What answers a foot's clicks.
pub type OnFoot = Rc<dyn Fn(FootEvent, &mut App)>;

/// While an upload goes (#214), what would post it does nothing: no comment goes out without its image.
fn waiting_foot(waits: bool, on: OnFoot) -> OnFoot {
    if !waits {
        return on;
    }
    Rc::new(move |event, cx| {
        if !event.posts() {
            on(event, cx)
        }
    })
}

impl FootEvent {
    /// It sends the comment somewhere: posted, added to the review, or for the agent.
    pub fn posts(&self) -> bool {
        matches!(self, FootEvent::Add | FootEvent::AddToReview)
    }
}

impl Compose {
    /// Who sees the comment: your agent or everyone on the request.
    pub fn source(&self) -> Source {
        match self.target {
            Target::Agent
            | Target::Overview(..)
            | Target::Edit(Change::Agent(_))
            | Target::ThreadNote(_)
            | Target::Answer(_)
            | Target::AnswerValue(_) => Source::Agent,
            Target::Request(_)
            | Target::Reply(_)
            | Target::Edit(Change::Note { .. } | Change::Draft(_) | Change::Description(_))
            | Target::InboxReply => Source::GitLab,
        }
    }

    /// The audience over a new comment's text (#200, Design D1): two segments — everyone on the request, your
    /// agent's chat — the one that reads it lit with its words (a chat's letter on its hue), the other its icon
    /// alone with its tooltip; a click on that one switches (on a request's page), as ⇥ does. Another chat is
    /// chosen from the send button's ▾ (#236).
    pub fn audience_chip(&self, audience: &Audience, on: OnFoot) -> AnyElement {
        let tone = self.source().color();
        let agent = self.source() == Source::Agent;
        let both = audience.both;
        let switch = if audience.switch {
            "  ·  ⇥ switches"
        } else {
            ""
        };
        let words = audience.words.clone();
        let lit_words = move || {
            hover::cut("compose-audience-words", words.clone())
                .min_w_0()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme::text())
        };
        let everyone = |lit: bool| {
            let said: SharedString = if lit {
                format!("Who reads it: {}{switch}", audience.words).into()
            } else {
                format!("Everyone on the request · a comment on the host{switch}").into()
            };
            let on = on.clone();
            div()
                .id("compose-audience-everyone")
                .h(px(22.))
                .min_w_0()
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(5.))
                .text_size(px(12.))
                .whitespace_nowrap()
                .when(lit, |s| s.bg(theme::selected()))
                .when(both && !lit, |s| {
                    s.cursor_pointer().hover(|s| s.bg(theme::hover()))
                })
                .child(
                    icons::icon("users")
                        .size(px(13.))
                        .flex_none()
                        .text_color(if lit { tone } else { theme::muted() }),
                )
                .when(lit, |s| s.child(lit_words()))
                .tooltip_show_delay(hover::LONG)
                .tooltip(move |_, cx| cx.new(|_| hover::TipOwned(said.clone())).into())
                .when(both && !lit, |s| {
                    s.on_click(move |_, _, cx| on(FootEvent::Everyone, cx))
                })
        };
        let bot = |lit: bool| {
            let said: SharedString = if lit {
                format!("Who reads it: {}{switch}", audience.words).into()
            } else {
                format!("A note for your agent, it stays on this Mac{switch}").into()
            };
            let on = on.clone();
            let hue = audience.mark.map_or(tone, |(_, hue)| hue);
            div()
                .id("compose-audience-agent")
                .h(px(22.))
                .min_w_0()
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(5.))
                .text_size(px(12.))
                .whitespace_nowrap()
                .when(lit, |s| s.bg(theme::selected()))
                .when(both && !lit, |s| {
                    s.cursor_pointer().hover(|s| s.bg(theme::hover()))
                })
                .child(match audience.mark {
                    Some((letter, hue)) if lit => {
                        crate::monogram::letter(letter, hue, 16.).into_any_element()
                    }
                    _ => icons::icon("bot")
                        .size(px(13.))
                        .flex_none()
                        .text_color(if lit { hue } else { theme::muted() })
                        .into_any_element(),
                })
                .when(lit, |s| s.child(lit_words()))
                .tooltip_show_delay(hover::LONG)
                .tooltip(move |_, cx| cx.new(|_| hover::TipOwned(said.clone())).into())
                .when(both && !lit, |s| {
                    s.on_click(move |_, _, cx| on(FootEvent::Agent, cx))
                })
        };
        div()
            .id("compose-audience")
            .h(px(26.))
            .max_w_full()
            .min_w_0()
            .p(px(1.))
            .flex()
            .items_center()
            .gap(px(2.))
            .rounded(px(6.))
            .border_1()
            .border_color(theme::selected())
            // Where the request lists who can read it, both segments; elsewhere only the one that does.
            .when(both || !agent, |s| s.child(everyone(!agent)))
            .when(both || agent, |s| s.child(bot(agent)))
            .into_any_element()
    }

    /// A new comment's foot by its room (Design shell round 6, #129): Cancel, the other ways to send and the
    /// primary; who reads it is the `To:` chip over the text (#200, `audience_chip`). For the agent the primary keeps
    /// the comment as a draft, listed under Drafts (its own send is on the comment, #220, #243), its ▾ holding Choose the chat…, Copy as markdown.
    pub fn foot(&self, look: FootLook, on: OnFoot) -> Vec<AnyElement> {
        let waits = self.waiting();
        let on = waiting_foot(waits, on);
        let source = self.source();
        let tone = source.color();
        let agent = matches!(self.target, Target::Agent | Target::Overview(..));
        let small = look.room == Room::S;
        let request = match self.target {
            Target::Request(iid) => Some(iid),
            _ => None,
        };
        let cancel = on.clone();
        // Cancel is a boxed ✕ at every room, named in its tooltip (#236).
        let mut tail = vec![
            discussion::icon_button(
                "compose-cancel",
                "x",
                "Cancel",
                Some("esc"),
                None,
                Kind::Plain,
                true,
            )
            .h(px(28.))
            .px(px(7.))
            .on_click(move |_, _, cx| cancel(FootEvent::Cancel, cx))
            .into_any_element(),
        ];
        let add = on.clone();
        // A comment on a line for the agent is kept as a draft; its own send is on the comment (#220).
        let primary = if agent { "Add to drafts" } else { "Comment" };
        let said: SharedString = if waits {
            "Waits for the upload".into()
        } else if agent {
            "Keep it as a draft, in the Agents tab's Drafts  ⌘↵\nIts own send is on the comment"
                .into()
        } else {
            "Comment  ⌘↵".into()
        };
        let add_button = discussion::button(
            "compose-add",
            primary,
            (!small && !agent).then_some("⌘↵"),
            Kind::Primary(tone),
        )
        .h(px(28.))
        .px(px(10.))
        // An upload going: dimmed, and it says why (#214).
        .when(waits, |s| s.opacity(0.5).cursor_default())
        .tooltip_show_delay(hover::LONG)
        .tooltip(move |_, cx| cx.new(|_| hover::TipOwned(said.clone())).into())
        .on_click(move |_, _, cx| add(FootEvent::Add, cx));
        let more = agent || (request.is_some() && look.review);
        tail.push(if !more {
            add_button.into_any_element()
        } else {
            div()
                .flex()
                .flex_none()
                .gap(px(1.))
                .child(add_button.rounded_r(px(0.)))
                .child(
                    discussion::chevron_button(
                        "compose-add-more",
                        Kind::Primary(tone),
                        if agent {
                            ("Choose the chat… · Copy as markdown", None)
                        } else {
                            (
                                "Add to review  ⇧⌘↵",
                                Some("It goes with your review when you submit it"),
                            )
                        },
                    )
                    .h(px(28.))
                    .px(px(6.))
                    .rounded_l(px(0.))
                    .on_click(move |event, _, cx| {
                        on(
                            if agent {
                                FootEvent::More(event.position())
                            } else {
                                FootEvent::Folded(event.position())
                            },
                            cx,
                        )
                    }),
                )
                .into_any_element()
        });
        tail
    }

    /// A reply's foot, the same as a new comment's (#129) at `room`: which of the two it writes on a discussion,
    /// a reply or a note for the agent, and ⇥ to switch (#50 G); Cancel and the primary (Send answer in the Inbox,
    /// `inbox`). With its placeholder.
    pub fn reply_foot(
        &self,
        room: Room,
        inbox: bool,
        on: OnFoot,
    ) -> (Vec<AnyElement>, &'static str) {
        let waits = self.waiting();
        let on = waiting_foot(waits, on);
        // ⇥ turned it into a note for the agent: purple, and it stays on this Mac (#50 G).
        let agent = self.source() == Source::Agent;
        let tone = self.source().color();
        let (cancel, review) = (on.clone(), on.clone());
        // Narrow, Cancel is its ×.
        let narrow = room != Room::L;
        let mut tail = vec![
            if narrow {
                discussion::icon_button(
                    "reply-cancel",
                    "x",
                    "Cancel",
                    Some("esc"),
                    None,
                    Kind::Quiet,
                    true,
                )
            } else {
                discussion::button("reply-cancel", "Cancel", None, Kind::Quiet)
                    .tooltip_show_delay(hover::LONG)
                    .tooltip(|_, cx| cx.new(|_| hover::Tip("Esc")).into())
            }
            .h(px(28.))
            .px(px(if narrow { 6. } else { 10. }))
            .on_click(move |_, _, cx| cancel(FootEvent::Cancel, cx))
            .into_any_element(),
        ];
        // A reply on the request can wait for the review too (#200): ⇧⌘↵.
        if matches!(self.target, Target::Reply(_)) && !inbox {
            tail.push(
                discussion::icon_button(
                    "reply-review",
                    "layers",
                    "Add to review",
                    Some("⇧⌘↵"),
                    Some("Pending until you submit the review"),
                    Kind::Quiet,
                    narrow,
                )
                .h(px(28.))
                .px(px(if narrow { 6. } else { 10. }))
                .on_click(move |_, _, cx| review(FootEvent::AddToReview, cx))
                .into_any_element(),
            );
        }
        let (send, more) = (on.clone(), on.clone());
        // A reply on a discussion has its own ▾: Reply and resolve ⌥⌘↵, or Reply and reopen (#199); under L the
        // room it takes is its key's, said in the tooltip.
        let with_more = matches!(self.target, Target::Reply(_)) && !agent;
        let cap = room == Room::L || (room == Room::M && !with_more);
        let reply = discussion::button(
            "reply-send",
            if agent && inbox && matches!(self.target, Target::Answer(_)) {
                "Send answer"
            } else if agent {
                "Add to drafts"
            } else {
                "Reply"
            },
            cap.then_some("⌘↵"),
            Kind::Primary(tone),
        )
        .h(px(28.))
        .px(px(10.))
        .when(waits, |s| s.opacity(0.5).cursor_default())
        .when(!cap || waits, |s| {
            s.tooltip_show_delay(hover::LONG).tooltip(move |_, cx| {
                cx.new(|_| {
                    hover::Tip(if waits {
                        "Waits for the upload"
                    } else {
                        "Reply  ⌘↵"
                    })
                })
                .into()
            })
        })
        .on_click(move |_, _, cx| send(FootEvent::Add, cx));
        tail.push(if with_more {
            div()
                .flex()
                .flex_none()
                .gap(px(1.))
                .child(reply.rounded_r(px(0.)))
                .child(
                    discussion::icon_button(
                        "reply-more",
                        "chevron",
                        "Reply and resolve, or reopen",
                        None,
                        None,
                        Kind::Primary(tone),
                        true,
                    )
                    .h(px(28.))
                    .px(px(6.))
                    .rounded_l(px(0.))
                    .on_click(move |event, _, cx| more(FootEvent::Folded(event.position()), cx)),
                )
                .into_any_element()
        } else {
            reply.into_any_element()
        });
        let placeholder = match (self.target, room) {
            (Target::ThreadNote(_), _) => "Note for your agent",
            (Target::Answer(_), Room::S) => "Note with your answer",
            (Target::Answer(_), _) => "A note for the agent with your answer",
            (Target::AnswerValue(_), _) => "Your own value",
            _ => "Reply…",
        };
        (tail, placeholder)
    }

    /// An edit's foot: Cancel and Save, its keys unless the diff is `narrow`.
    pub fn edit_foot(&self, narrow: bool, on: OnFoot) -> Vec<AnyElement> {
        let cancel = on.clone();
        vec![
            discussion::button("edit-cancel", "Cancel", None, Kind::Quiet)
                .h(px(28.))
                .px(px(10.))
                .tooltip_show_delay(hover::LONG)
                .tooltip(|_, cx| cx.new(|_| hover::Tip("Esc")).into())
                .on_click(move |_, _, cx| cancel(FootEvent::Cancel, cx))
                .into_any_element(),
            discussion::button(
                "edit-save",
                "Save",
                (!narrow).then_some("⌘↵"),
                Kind::Primary(self.source().color()),
            )
            .h(px(28.))
            .px(px(10.))
            .on_click(move |_, _, cx| on(FootEvent::Add, cx))
            .into_any_element(),
        ]
    }

    /// The box `input` of a comment on several lines: which, over it.
    pub fn with_lines(&self, input: Div) -> Div {
        match self.first {
            Some(first) => div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .font_family(theme::ui_font())
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(format!("Lines {first}–{}", self.line)),
                )
                .child(input),
            None => input,
        }
    }
}

/// What a composer's foot shows, by its box's width (Design shell round 6, 2026-10-24, #129). The
/// foot is one row at each: what does not fit folds, nothing wraps and nothing is cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Room {
    /// 720 px and up: who sees it in words, the helpers, every button with its label and keys.
    L,
    /// The secondary buttons are their icons, who sees it its dot, the keys go.
    M,
    /// Under 440 px: the helpers fold into T, Send and ▾ into the primary's own ▾; the hint is
    /// short. (Drawn at 420; a request's audience chip with its ▾ needs 423 for M's row.)
    S,
}

/// The room of a box `width` wide; 0 is not measured yet. A tight diff is never L.
pub fn room_for(width: f32, tight: bool) -> Room {
    match width {
        w if w > 0. && w < 440. => Room::S,
        w if tight || (w > 0. && w < 720.) => Room::M,
        _ => Room::L,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_composer_folds_its_foot_by_its_own_width() {
        assert_eq!(room_for(0., false), Room::L);
        assert_eq!(room_for(760., false), Room::L);
        assert_eq!(room_for(630., false), Room::M);
        assert_eq!(room_for(440., false), Room::M);
        assert_eq!(room_for(439., false), Room::S);
        assert_eq!(room_for(136., false), Room::S);
        assert_eq!(room_for(900., true), Room::M);
        assert_eq!(room_for(300., true), Room::S);
    }
}
