//! Which input has the keyboard (#168). An input can be open and keep its text without having
//! the keyboard: one at a time has it, and the key context and the key router follow that one, not
//! which inputs are open. It loses the keyboard to a press outside its box, to another page (a
//! composer also to another file), to another input, to esc while it holds text; it gets it back
//! from a press in its box, from its page coming back, from the action that opened it. A card over
//! everything (⌘K, Go to file, the list beside a folder …) holds the keyboard while it is open and gives it back on closing.
//! The rule is the frame's (`Keys`' own methods); which inputs there are, which are open and where the window is are
//! the tab's.

use gpui::App;

/// An input that takes typed keys.
pub trait Input: Copy + Eq {
    /// Over everything until it closes: it is never open without the keyboard, and closing it
    /// gives the keyboard back to the input that had it before.
    fn modal(self) -> bool;
    /// Of the page it was opened on: elsewhere it keeps its text, not the keyboard.
    fn local(self) -> bool;
    /// Left on another page, it waits there to be given the keyboard back; one that does not closes.
    fn waits(self) -> bool;
}

#[derive(Clone)]
struct Holder<I, S> {
    input: I,
    /// None until the next look: the action that opened it may still go to its page.
    at: Option<S>,
}

/// The input with the keyboard, the one a card over everything took it from, and the last one to
/// let it go.
pub struct Keyboard<I, S> {
    holder: Option<Holder<I, S>>,
    before: Option<Holder<I, S>>,
    left: Option<Holder<I, S>>,
    /// `left` lost it to another page or file: it is given back there.
    left_away: bool,
    /// The inputs open at the last look: one opened since takes the keyboard.
    open: Vec<I>,
}

impl<I, S> Default for Keyboard<I, S> {
    fn default() -> Self {
        Keyboard {
            holder: None,
            before: None,
            left: None,
            left_away: false,
            open: Vec::new(),
        }
    }
}

/// The tab's inputs and where it is; the rule is the provided methods.
pub trait Keys {
    type Input: Input;
    /// Where the keyboard was taken: the page, the file.
    type Spot: Clone;

    fn keyboard(&self) -> &Keyboard<Self::Input, Self::Spot>;
    fn keyboard_mut(&mut self) -> &mut Keyboard<Self::Input, Self::Spot>;
    /// Every input, in the order one of several opened at once takes the keyboard (a card first anyway).
    fn inputs(&self, cx: &App) -> Vec<Self::Input>;
    /// Whether `input` is open, with the keyboard or not.
    fn input_open(&self, input: Self::Input, cx: &App) -> bool;
    /// Where the window is now.
    fn spot(&self, cx: &App) -> Self::Spot;
    /// Whether local `input`, which took the keyboard at `at`, is on screen there.
    fn still_at(&self, input: Self::Input, at: &Self::Spot, cx: &App) -> bool;
    /// `input`, not a card, took the keyboard.
    fn took(&mut self, input: Self::Input);
    /// `input`, which does not wait, lost the keyboard to another page: it closes.
    fn dropped(&mut self, input: Self::Input);

    /// The input with the keyboard: keys go to it, and single-key actions wait.
    fn keyboard_holder(&self, cx: &App) -> Option<Self::Input> {
        // A card over everything, also one opened since the last look.
        if let Some(h) = &self.keyboard().holder
            && h.input.modal()
            && self.input_open(h.input, cx)
        {
            return Some(h.input);
        }
        if let Some(card) = self
            .inputs(cx)
            .into_iter()
            .find(|i| i.modal() && self.input_open(*i, cx))
        {
            return Some(card);
        }
        let h = self.keyboard().holder.as_ref()?;
        (self.input_open(h.input, cx) && here(self, h, cx)).then_some(h.input)
    }

    /// `input` takes the keyboard: from a press in its box, or the action that opens it.
    fn keyboard_take(&mut self, input: Self::Input, cx: &App) {
        if self
            .keyboard()
            .holder
            .as_ref()
            .is_some_and(|h| h.input == input)
            && self.keyboard_holder(cx) == Some(input)
        {
            return;
        }
        let had = self
            .keyboard_mut()
            .holder
            .replace(Holder { input, at: None });
        if input.modal() {
            // A card over a card keeps the first one's: it is what comes back.
            if !had.as_ref().is_some_and(|h| h.input.modal()) {
                self.keyboard_mut().before = had;
            }
        } else {
            self.took(input);
            let keyboard = self.keyboard_mut();
            keyboard.before = None;
            keyboard.left = None;
        }
    }

    /// The input with the keyboard lets it go, its text kept: nothing has it. Which one it was, a card never.
    fn keyboard_let_go(&mut self, cx: &App) -> Option<Self::Input> {
        if self.keyboard_holder(cx).is_none_or(Input::modal) {
            return None;
        }
        let keyboard = self.keyboard_mut();
        let h = keyboard.holder.take()?;
        let input = h.input;
        keyboard.left = Some(h);
        keyboard.left_away = false;
        Some(input)
    }

    /// `input`, the last to let the keyboard go and on screen where it had it, takes it back.
    fn keyboard_give_back(&mut self, input: Self::Input, cx: &App) -> bool {
        match self.keyboard_mut().left.take() {
            Some(h) if h.input == input && self.input_open(input, cx) && here(self, &h, cx) => {
                self.keyboard_mut().holder = Some(h);
                true
            }
            left => {
                self.keyboard_mut().left = left;
                false
            }
        }
    }

    /// Each frame and each key: an input opened since the last look takes the keyboard; a card
    /// closed gives it back; a page left takes it from its inputs, a page come back to returns it.
    fn keyboard_sync(&mut self, cx: &App) {
        let open: Vec<Self::Input> = self
            .inputs(cx)
            .into_iter()
            .filter(|i| self.input_open(*i, cx))
            .collect();
        let was = std::mem::replace(&mut self.keyboard_mut().open, open.clone());
        // A card opened with a field is over it.
        if let Some(input) = open
            .into_iter()
            .filter(|i| !was.contains(i))
            .min_by_key(|i| !i.modal())
        {
            self.keyboard_take(input, cx);
        }
        let spot = self.spot(cx);
        if let Some(h) = self.keyboard_mut().holder.as_mut()
            && h.at.is_none()
        {
            h.at = Some(spot);
        }
        if let Some(h) = self.keyboard().holder.clone() {
            if !self.input_open(h.input, cx) {
                let keyboard = self.keyboard_mut();
                keyboard.holder = if h.input.modal() {
                    keyboard.before.take()
                } else {
                    None
                };
            } else if !here(self, &h, cx) {
                self.keyboard_mut().holder = None;
                if h.input.waits() {
                    let keyboard = self.keyboard_mut();
                    keyboard.left = Some(h);
                    keyboard.left_away = true;
                } else {
                    self.dropped(h.input);
                }
            }
        }
        if self.keyboard().holder.is_none()
            && self.keyboard().left_away
            && let Some(input) = self.keyboard().left.as_ref().map(|h| h.input)
        {
            self.keyboard_give_back(input, cx);
        }
    }
}

/// Whether `h` is on screen where it took the keyboard: a composer on its page and file.
fn here<K: Keys + ?Sized>(keys: &K, h: &Holder<K::Input, K::Spot>, cx: &App) -> bool {
    let Some(at) = &h.at else {
        return true;
    };
    !h.input.local() || keys.still_at(h.input, at, cx)
}
