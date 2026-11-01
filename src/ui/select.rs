//! Selecting text in the diff like in an editor: a click puts the caret, a drag selects characters,
//! a double click a word, a triple click the line, ⇧-click extends. A selection lives on one side of
//! the diff and runs between two positions (line and byte offset into that line's text).

use super::{CopySelection, SelectAll, Workspace};
use artoo_review::select::{Pos, Sel, word_at};
use gpui::{App, ClipboardItem, Context, Window};

impl Workspace {
    /// A press on a line at byte `byte`: the caret, a word or the line, by click count; ⇧ extends.
    pub(super) fn select_press(
        &mut self,
        old: bool,
        line: u32,
        byte: usize,
        clicks: usize,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        self.ctx_menu.update(cx, |m, cx| m.shut(cx));
        self.note_sel = None;
        let at = Pos { line, byte };
        let text = self.review.read(cx).line_text(old, line);
        self.review.update(cx, |rv, _| rv.selecting = true);
        self.review.update(cx, |rv, _| {
            rv.sel = match (rv.sel, clicks, shift) {
                (Some(sel), 1, true) if sel.old == old => Some(Sel { head: at, ..sel }),
                (_, 2, _) => {
                    let w = word_at(&text, byte);
                    Some(Sel {
                        old,
                        anchor: Pos {
                            line,
                            byte: w.start,
                        },
                        head: Pos { line, byte: w.end },
                    })
                }
                (_, n, _) if n >= 3 => Some(Sel {
                    old,
                    anchor: Pos { line, byte: 0 },
                    head: Pos {
                        line,
                        byte: text.len(),
                    },
                }),
                _ => Some(Sel {
                    old,
                    anchor: at,
                    head: at,
                }),
            }
        });
        cx.notify();
    }

    /// The pointer moved over a line with the button down: the selection reaches it.
    pub(super) fn select_drag(
        &mut self,
        old: bool,
        line: u32,
        byte: usize,
        cx: &mut Context<Self>,
    ) {
        if !self.review.read(cx).selecting {
            return;
        }
        let head = Pos { line, byte };
        let moved = self.review.update(cx, |rv, _| {
            if let Some(sel) = &mut rv.sel
                && sel.old == old
                && sel.head != head
            {
                sel.head = head;
                return true;
            }
            false
        });
        if moved {
            cx.notify();
        }
    }

    /// ⌘C.
    pub(super) fn copy_selection(
        &mut self,
        _: &CopySelection,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A selection in the field being typed in wins; then one in a comment's text; then the
        // diff's.
        let text = self
            .field_selection(cx)
            .or_else(|| self.note_sel.as_ref().and_then(|s| s.text()))
            .or_else(|| self.review.read(cx).selected_text());
        if let Some(text) = text {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// The text selected in the field that has the keyboard (never in the token's).
    fn field_selection(&self, cx: &App) -> Option<String> {
        use super::typing::Input;
        let shared = match self.typing(cx)? {
            Input::Compose => {
                return self
                    .compose
                    .as_ref()?
                    .edit
                    .selected_text()
                    .map(str::to_owned);
            }
            Input::Field => self.field != Some(super::lists::Field::Token),
            Input::Part(id) => return self.part_input(id).and_then(|i| (i.selected)(cx)),
            Input::Palette | Input::FindFiles | Input::Submit => true,
            _ => false,
        };
        shared
            .then(|| self.field_edit.selected_text().map(str::to_owned))
            .flatten()
    }

    /// ⌘A: the whole side of the diff that is being read (the new one, unless the old is selected).
    pub(super) fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let Some(data) = &self.review.read(cx).data else {
            return;
        };
        let old = self.review.read(cx).sel.is_some_and(|s| s.old);
        let side = data.side(old);
        let last = side.count().max(1);
        let sel = Sel {
            old,
            anchor: Pos { line: 1, byte: 0 },
            head: Pos {
                line: last,
                byte: side.line(last).0.len(),
            },
        };
        self.review.update(cx, |rv, _| rv.sel = Some(sel));
        cx.notify();
    }
}

impl Workspace {
    /// How the text of comments can be selected: the selection now, and presses back to here.
    pub(super) fn selecting(&self, this: gpui::WeakEntity<Self>) -> super::markdown::Selecting {
        let link = this.clone();
        let mention = this.clone();
        super::markdown::Selecting {
            current: self.note_sel.clone(),
            on: std::rc::Rc::new(move |press, cx| {
                this.update(cx, |this, cx| this.note_press(press, cx)).ok();
            }),
            link: std::rc::Rc::new(move |url, cx| {
                link.update(cx, |this, cx| this.open_url_here(url, cx))
                    .unwrap_or(false)
            }),
            mention: std::rc::Rc::new(move |hit, cx| {
                mention
                    .update(cx, |this, cx| this.hover_mention(hit, cx))
                    .ok();
            }),
        }
    }

    /// A press in a comment's text: the caret (nothing selected yet), a word, the block, or
    /// growing the selection with ⇧ or a drag. The diff's selection goes.
    fn note_press(&mut self, press: super::markdown::Press, cx: &mut Context<Self>) {
        use super::markdown::{Press, TextSel};
        match press {
            Press::Down {
                note,
                block,
                at,
                clicks,
                shift,
                texts,
            } => {
                self.review.update(cx, |rv, _| rv.sel = None);
                self.ctx_menu.update(cx, |m, cx| m.shut(cx));
                let text = texts.borrow().get(block).cloned().unwrap_or_default();
                let same = self.note_sel.as_ref().is_some_and(|s| s.note == note);
                self.note_sel = Some(match (clicks, shift) {
                    (1, true) if same => {
                        let mut s = self.note_sel.clone().expect("checked");
                        s.head = (block, at);
                        s
                    }
                    (2, _) => {
                        let w = word_at(&text, at);
                        TextSel {
                            note,
                            anchor: (block, w.start),
                            head: (block, w.end),
                            texts,
                        }
                    }
                    (c, _) if c >= 3 => TextSel {
                        note,
                        anchor: (block, 0),
                        head: (block, text.len()),
                        texts,
                    },
                    _ => TextSel {
                        note,
                        anchor: (block, at),
                        head: (block, at),
                        texts,
                    },
                });
            }
            Press::Drag { note, block, at } => match self.note_sel.as_mut() {
                Some(s) if s.note == note => s.head = (block, at),
                _ => return,
            },
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(a: (u32, usize), b: (u32, usize)) -> Sel {
        Sel {
            old: false,
            anchor: Pos {
                line: a.0,
                byte: a.1,
            },
            head: Pos {
                line: b.0,
                byte: b.1,
            },
        }
    }

    #[test]
    fn a_selection_covers_part_of_its_first_and_last_lines() {
        let s = sel((2, 4), (4, 3));
        assert_eq!(s.in_line(1, 10), None);
        assert_eq!(s.in_line(2, 10), Some(4..10));
        assert_eq!(s.in_line(3, 10), Some(0..10));
        assert_eq!(s.in_line(4, 10), Some(0..3));
        assert_eq!(s.in_line(5, 10), None);
    }

    #[test]
    fn the_ends_are_the_same_whichever_way_it_was_dragged() {
        let a = sel((2, 4), (4, 3));
        let b = sel((4, 3), (2, 4));
        assert_eq!(a.ends(), b.ends());
        assert_eq!(b.in_line(2, 10), Some(4..10));
    }

    #[test]
    fn a_caret_selects_nothing() {
        let s = sel((2, 4), (2, 4));
        assert!(s.is_empty());
        assert_eq!(s.in_line(2, 10), None);
    }

    #[test]
    fn a_double_click_takes_the_word_under_it() {
        let text = "let max_len = 10;";
        assert_eq!(word_at(text, 5), 4..11);
        assert_eq!(word_at(text, 3), 3..4);
        assert_eq!(word_at(text, text.len()), text.len() - 1..text.len());
        assert_eq!(word_at("", 0), 0..0);
    }
}
