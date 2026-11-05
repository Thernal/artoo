//! The welcome's contract (#54): the window with no repository open — the ways in, a way's card in their place, the
//! repositories opened lately. Open a repository is the tab's (the folder picker), the other ways their owners'
//! (`Parts::ways`: connecting a host the hosts', cloning the shell's), each with its card; the frame draws the page
//! and the cards' frames, their fields' insides the owners'.

use crate::{Frame, render};
use gpui::{AnyElement, App, Window, div, prelude::*};
use std::path::PathBuf;
use std::rc::Rc;

/// What a press does.
pub type Press = Rc<dyn Fn(&mut Window, &mut App)>;

/// A way in besides opening a folder (`Parts::ways`): connect a host, clone from a URL. The ways are drawn under
/// Open a repository in their `order`; a click opens the way's card, drawn in place of the ways while it is open.
#[derive(Clone)]
pub struct Way {
    /// The element's id (`welcome-connect`).
    pub id: &'static str,
    pub icon: &'static str,
    pub title: &'static str,
    /// A word on it, under its title.
    pub text: &'static str,
    pub order: u32,
    /// A click on it: its card opened, its first field given the keyboard.
    pub go: Press,
    /// Its card while it is open.
    pub card: CardOf,
}

/// A way's card while it is open (`Way::card`), given the part's input that has the tab's keyboard; none while it
/// is closed.
pub type CardOf = Rc<dyn Fn(Option<&'static str>, &mut App) -> Option<Card>>;

/// A way's card: its title and a word on it, its fields, what the last try said, Back and its button.
pub struct Card {
    pub title: &'static str,
    pub text: &'static str,
    pub fields: Vec<Field>,
    pub said: Option<(String, Tone)>,
    /// Back, and esc in a field: the ways again.
    pub back: Press,
    /// Its button, with ↵: the element's id, its words, what it does.
    pub go: (&'static str, &'static str, Press),
}

/// How what a card says reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    /// Under way, or what is being done.
    Muted,
    /// It worked.
    Good,
    /// It did not, and why.
    Bad,
}

/// A field of a card, under its label: one of the tab's inputs (`Input`), its box given the keyboard by a press.
pub struct Field {
    pub label: &'static str,
    /// A link beside its label: the element's id, its words, what a click does (Create one ↗).
    pub link: Option<(&'static str, &'static str, Press)>,
    /// The element's id.
    pub id: &'static str,
    /// The part's input it is (`Input::id`).
    pub input: &'static str,
    /// Whether it has the keyboard.
    pub active: bool,
    /// What is inside its box: the kit's field.
    pub inside: AnyElement,
}

/// The page as the tab sees it.
pub struct Page {
    /// The ways its parts offer, in their order.
    pub ways: Vec<Way>,
    /// The way's card that is open, in place of the ways.
    pub card: Option<Card>,
    /// The repositories opened lately.
    pub recent: Vec<PathBuf>,
}

/// What a click on it does that is the tab's.
pub trait WelcomeHost {
    /// Open a repository: the folder picker.
    fn open(&self, window: &mut Window, cx: &mut App);
    /// A repository opened lately.
    fn recent(&self, path: PathBuf, window: &mut Window, cx: &mut App);
}

/// Draws the page: the frame's.
pub trait WelcomeRenderer {
    fn page(&self, page: Page, host: Rc<dyn WelcomeHost>, frame: Rc<dyn Frame>) -> AnyElement;
}

/// The page, drawn by the frame installed.
pub fn page(page: Page, host: Rc<dyn WelcomeHost>, frame: Rc<dyn Frame>, cx: &App) -> AnyElement {
    match render::renderer::<dyn WelcomeRenderer>(cx) {
        Some(renderer) => renderer.page(page, host, frame),
        None => div().into_any_element(),
    }
}
