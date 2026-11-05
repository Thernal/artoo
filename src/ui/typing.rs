//! Which input has the keyboard (#168). An input can be open and keep its text without having
//! the keyboard: one at a time has it, and the key context and the key router follow that one, not
//! which inputs are open. It loses the keyboard to a press outside its box, to another page (a
//! composer also to another file), to another input, to esc while it holds text; it gets it back
//! from a press in its box, from its page coming back, from the action that opened it. A card over
//! everything (⌘K, Go to file, the list beside a folder …) holds the keyboard while it is open and gives it back on closing.
//! The rule is `keyboard.rs`'; the inputs, which are open and where the tab is are here.

use super::Workspace;
use super::keyboard::{self, Keys};
use super::lists::Field;
use super::spaces::Space;
use gpui::{App, Context, MouseButton, MouseDownEvent, WeakEntity, prelude::*};

/// Every input that takes typed keys.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Input {
    /// ⌘K and the palettes sharing its box.
    Palette,
    /// Go to file, and find in files (`findfiles.rs`).
    FindFiles,
    /// Which app opens a file, asked (`opening.rs`).
    OpenAsk,
    /// The reactions' grid and its search.
    Reactions,
    /// A feature's card over the window (`artoo_shell_api::Overlay`): the new version's (#135),
    /// which chat a note goes to (`Agents::pick_chat`).
    Overlay,
    /// What is beside a folder of Code's path (`beside.rs`).
    Beside,
    /// The card before a merge goes, its message (`merging.rs`, #164).
    Merge,
    /// A one-line field: a list's filter, the token, the host, a name being given … —
    /// `Workspace::field` says which, and is set only while it has the keyboard.
    Field,
    /// A comment, a reply or a note being written.
    Compose,
    /// Submit review's summary.
    Submit,
    /// A feature's tool window being written in (`artoo_shell_api::Tool`): Report a problem's title and
    /// description.
    Tool,
    /// A part's field (`artoo_shell_api::Input`), by its id: the commits' search, the find bar over the diff, the line typed after
    /// `c`, the New merge request form.
    Part(&'static str),
}

impl Input {
    const ALL: [Input; 11] = [
        Input::Palette,
        Input::FindFiles,
        Input::OpenAsk,
        Input::Reactions,
        Input::Overlay,
        Input::Beside,
        Input::Merge,
        Input::Field,
        Input::Compose,
        Input::Submit,
        Input::Tool,
    ];
}

impl keyboard::Input for Input {
    fn modal(self) -> bool {
        matches!(
            self,
            Input::Palette
                | Input::FindFiles
                | Input::OpenAsk
                | Input::Reactions
                | Input::Overlay
                | Input::Beside
                | Input::Merge
        )
    }

    /// Submit and a feature's tool window are in the tool window, which stays from page to page.
    fn local(self) -> bool {
        !self.modal() && !matches!(self, Input::Submit | Input::Tool)
    }

    /// A one-line field is set only while it has the keyboard.
    fn waits(self) -> bool {
        self != Input::Field
    }
}

/// Where the keyboard was taken: Settings or a space's page, and the diff's file.
#[derive(Clone, PartialEq, Debug)]
pub struct Spot {
    settings: bool,
    space: Option<Space>,
    page: Option<artoo_core::route::Route>,
    file: Option<String>,
}

/// The input with the keyboard, the tab's (`keyboard.rs`).
pub(super) type Keyboard = keyboard::Keyboard<Input, Spot>;

impl Keys for Workspace {
    type Input = Input;
    type Spot = Spot;

    fn keyboard(&self) -> &Keyboard {
        &self.keyboard
    }

    fn keyboard_mut(&mut self) -> &mut Keyboard {
        &mut self.keyboard
    }

    /// The tab's own, then the parts' fields.
    fn inputs(&self, _: &App) -> Vec<Input> {
        Input::ALL
            .into_iter()
            .chain(
                self.parts
                    .iter()
                    .flat_map(|p| &p.inputs)
                    .map(|i| Input::Part(i.id)),
            )
            .collect()
    }

    fn input_open(&self, input: Input, cx: &App) -> bool {
        match input {
            Input::Palette => self.palette_open(cx),
            Input::FindFiles => self.rv().find_files_open(cx),
            Input::OpenAsk => self.open_ask.is_some(),
            Input::Reactions => self.requests_api().is_some_and(|r| r.grid_open(cx)),
            Input::Overlay => self.overlay_open(),
            Input::Beside => self.beside.is_some(),
            Input::Merge => self.requests_api().is_some_and(|r| r.merge_card_open(cx)),
            Input::Field => self.field.is_some(),
            Input::Compose => self.compose.is_some(),
            Input::Part(id) => self.part_input(id).is_some_and(|i| (i.open)(cx)),
            Input::Submit => self.submitting(cx),
            Input::Tool => self.tool_typing(),
        }
    }

    fn spot(&self, cx: &App) -> Spot {
        Spot {
            settings: self.settings_open,
            space: self.space(),
            page: self.nav_page().cloned(),
            file: self.current_path(cx),
        }
    }

    /// A composer on its page and file.
    fn still_at(&self, input: Input, at: &Spot, cx: &App) -> bool {
        let diff = self.shown_diff(cx);
        let file = (diff.as_ref())
            .and_then(|d| d.files.get(self.diff_file(cx)))
            .map(|f| f.path());
        at.settings == self.settings_open
            && at.space == self.space()
            && at.page.as_ref() == self.nav_page()
            && (input != Input::Compose || at.file.as_deref() == file)
    }

    fn took(&mut self, input: Input) {
        if input != Input::Field {
            self.field = None;
        }
    }

    fn dropped(&mut self, input: Input) {
        if input == Input::Field {
            self.field = None;
        }
    }
}

impl Workspace {
    /// The input with the keyboard: keys go to it, and single-key actions wait.
    pub(super) fn typing(&self, cx: &App) -> Option<Input> {
        self.keyboard_holder(cx)
    }

    pub(super) fn has_keyboard(&self, input: Input, cx: &App) -> bool {
        self.typing(cx) == Some(input)
    }

    /// The part's input with the keyboard, by its id.
    pub(super) fn part_typing(&self, cx: &App) -> Option<&'static str> {
        match self.typing(cx) {
            Some(Input::Part(id)) => Some(id),
            _ => None,
        }
    }

    /// `input` takes the keyboard: from a press in its box, or the action that opens it.
    pub(super) fn take_keyboard(&mut self, input: Input, cx: &App) {
        self.keyboard_take(input, cx)
    }

    /// The input with the keyboard lets it go, its text kept: nothing has it. One left empty
    /// closes — a find, a filter, the line number.
    pub(super) fn leave_input(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.keyboard_let_go(cx) else {
            return;
        };
        match input {
            Input::Field => self.field = None,
            Input::Part(id) => {
                if let Some(left) = self.part_input(id).map(|i| i.left.clone()) {
                    left(cx);
                }
            }
            _ => {}
        }
        cx.notify();
    }

    /// `input`, the last to let the keyboard go and on screen where it had it, takes it back.
    pub(super) fn give_back(&mut self, input: Input, cx: &App) -> bool {
        self.keyboard_give_back(input, cx)
    }

    /// Each frame and each key (`keyboard.rs`).
    pub(super) fn sync_keyboard(&mut self, cx: &App) {
        self.keyboard_sync(cx)
    }
}

/// The box of `input`: a press in it gives it the keyboard, a press outside takes it away.
pub(super) fn input_box<E: InteractiveElement>(el: E, input: Input, cx: &Context<Workspace>) -> E {
    input_box_of(el, input, cx.entity().downgrade())
}

/// `input_box` for a view drawn without its context (a row of a list).
pub(super) fn input_box_of<E: InteractiveElement>(
    el: E,
    input: Input,
    this: WeakEntity<Workspace>,
) -> E {
    let out = this.clone();
    el.on_mouse_down(MouseButton::Left, move |_, _, cx| {
        this.update(cx, |this, cx| {
            this.take_keyboard(input, cx);
            cx.notify();
        })
        .ok();
    })
    .on_mouse_down_out(move |_, _, cx| {
        out.update(cx, |this, cx| {
            if this.has_keyboard(input, cx) {
                this.leave_input(cx);
            }
        })
        .ok();
    })
}

/// The box of `field`: a press outside it takes the keyboard away; a click in it gives it back
/// (the box's own `start_field`).
pub(super) fn field_box_out<E: InteractiveElement>(
    el: E,
    field: Field,
    cx: &Context<Workspace>,
) -> E {
    el.on_mouse_down_out(
        cx.listener(move |this: &mut Workspace, _: &MouseDownEvent, _, cx| {
            if this.field == Some(field) && this.has_keyboard(Input::Field, cx) {
                this.leave_input(cx);
            }
        }),
    )
}

#[cfg(test)]
mod tests {
    /// The key handlers `find_key` routes to by `Input`: the only places a key may edit a text, a part's field
    /// through its own (`part_input_key`).
    const ROUTED: [&str; 9] = [
        "ffind_key",
        "palette_key",
        "beside_key",
        "merge_key",
        "reaction_key",
        "submit_key",
        "compose_key",
        "filter_key",
        "part_input_key",
    ];

    /// A text edited from keys outside the router would take keys whether or not it has the
    /// keyboard (#168): every function that edits one is a routed handler, and each is routed.
    #[test]
    fn every_text_is_edited_through_the_router() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui");
        let mut stray = Vec::new();
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            // The editors themselves, and this test's own words.
            if !name.ends_with(".rs") || name == "input.rs" || name == "typing.rs" {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            let mut func = String::new();
            for line in source.lines() {
                let code = line.trim_start();
                if let Some(at) = code.find("fn ")
                    && (at == 0 || code[..at].ends_with(") ") || code.starts_with("pub"))
                {
                    func = code[at + 3..]
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                }
                let edits = [
                    "input::edit(",
                    "input::edit_area",
                    "input::edit_text(",
                    "input::edit\n",
                ]
                .iter()
                .any(|p| format!("{code}\n").contains(p));
                if edits && !ROUTED.contains(&func.as_str()) {
                    stray.push(format!("{name}: {func}"));
                }
            }
        }
        assert!(
            stray.is_empty(),
            "edits a text outside the key router: {stray:?}"
        );
        let router = std::fs::read_to_string(dir.join("find.rs")).unwrap();
        let router = &router[router.find("fn find_key(").unwrap()..];
        let router = &router[..router.find("None => {}").unwrap()];
        for handler in ROUTED {
            assert!(
                router.contains(&format!("self.{handler}(")),
                "{handler} is not routed"
            );
        }
    }
}
