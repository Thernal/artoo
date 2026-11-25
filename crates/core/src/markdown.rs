//! Markdown in comments: GitLab writes discussions in it, and a comment written here may use it too.
//! `parse` turns a body into blocks the UI draws; `apply` and `newline` are the composer's helpers,
//! which put the markers in for the writer at the caret or around the selection. A changed `.md`
//! file is read with `parse_lines` (each block with its source lines) and marked by `mddiff`.

use crate::mdhtml::{self, Tok};
use crate::textedit::TextEdit;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Paragraph(Inline),
    Heading(u8, Inline),
    /// A fenced or indented code block; `lang` is the fence's first word (`rust`, `suggestion:-0+0`).
    Code {
        lang: String,
        text: String,
        /// In a changed document: the lines (0-based) added and removed (#11).
        marks: Vec<(usize, Mark)>,
    },
    Quote(Vec<Block>),
    List {
        /// The first number of an ordered list.
        start: Option<u64>,
        items: Vec<Item>,
    },
    Table {
        head: Vec<Inline>,
        rows: Vec<Vec<Inline>>,
    },
    Rule,
    /// HTML's `<details>` (#231): its `<summary>` line and what it holds.
    Details {
        summary: Inline,
        body: Vec<Block>,
    },
    /// HTML's `align="center"` / `<center>` (#231): the blocks it holds, in the middle.
    Centered(Vec<Block>),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Item {
    /// `Some(done)` for a task list item.
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
    /// In a changed document: the item was added or removed (#11).
    pub mark: Option<Mark>,
}

/// A run of text with its styled ranges.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inline {
    pub text: String,
    pub spans: Vec<Span>,
    /// In a changed document: the words added and removed (#11).
    pub marks: Vec<(Range<usize>, Mark)>,
}

/// What a changed document's diff says of a piece of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Added,
    Removed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub range: Range<usize>,
    pub style: Style,
    /// Where a link goes.
    pub link: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    /// An image's alt text; its `link` is the image.
    pub image: bool,
}

impl Inline {
    fn push(&mut self, text: &str, style: Style, link: Option<&String>) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(text);
        if style == Style::default() && link.is_none() {
            return;
        }
        let end = self.text.len();
        match self.spans.last_mut() {
            Some(s) if s.range.end == start && s.style == style && s.link.as_ref() == link => {
                s.range.end = end
            }
            _ => self.spans.push(Span {
                range: start..end,
                style,
                link: link.cloned(),
            }),
        }
    }
}

enum Frame {
    Quote(Vec<Block>),
    List(Option<u64>, Vec<Item>),
    Item(Item),
    Table {
        head: Vec<Inline>,
        rows: Vec<Vec<Inline>>,
        row: Vec<Inline>,
    },
    /// From HTML (#231).
    Centered(Vec<Block>),
    Details {
        summary: Option<Inline>,
        blocks: Vec<Block>,
    },
}

#[derive(Default)]
struct Builder {
    root: Vec<Block>,
    frames: Vec<Frame>,
    /// The paragraph, heading or table cell being filled.
    inline: Option<Inline>,
    heading: Option<u8>,
    code: Option<(String, String)>,
    /// An HTML block's lines, read as one when it ends (a tag may span lines).
    html: Option<String>,
    /// Frames opened by HTML tags, parallel to `frames`: only those a closing tag may close.
    from_html: Vec<bool>,
    /// A `<pre>` being read, a `<script>` or `<style>` being skipped.
    pre: Option<String>,
    skip: Option<String>,
    /// Open `<p>`, `<div>`, `<center>`…: name, and whether it opened a `Centered` frame.
    boxes: Vec<(String, bool)>,
    /// A table cell or a `<summary>` is being filled: no block breaks inside.
    cell: bool,
    summary: bool,
    /// Styles and links opened by HTML tags; a paragraph's end drops what was left open.
    hbold: u32,
    hitalic: u32,
    hstrike: u32,
    hcode: u32,
    hlinks: Vec<Option<String>>,
    bold: u32,
    italic: u32,
    strike: u32,
    image: u32,
    links: Vec<String>,
}

impl Builder {
    fn style(&self, code: bool) -> Style {
        Style {
            bold: self.bold + self.hbold > 0,
            italic: self.italic + self.hitalic > 0,
            strike: self.strike + self.hstrike > 0,
            code: code || self.hcode > 0,
            image: self.image > 0,
        }
    }

    fn block(&mut self, block: Block) {
        match self.frames.last_mut() {
            Some(Frame::Quote(blocks)) => blocks.push(block),
            Some(Frame::Item(item)) => item.blocks.push(block),
            Some(Frame::Centered(blocks)) | Some(Frame::Details { blocks, .. }) => {
                blocks.push(block)
            }
            _ => self.root.push(block),
        }
    }

    /// Text in a tight list item comes without a paragraph around it; it gets one here.
    fn close_loose(&mut self) {
        if let Some(inline) = self.inline.take() {
            self.block(Block::Paragraph(inline));
        }
    }

    fn text(&mut self, text: &str, code: bool) {
        if let Some((_, body)) = self.code.as_mut() {
            return body.push_str(text);
        }
        let style = self.style(code);
        let link = match self.hlinks.last() {
            Some(link) => link.clone(),
            None => self.links.last().cloned(),
        };
        self.inline
            .get_or_insert_with(Inline::default)
            .push(text, style, link.as_ref());
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.inline = Some(Inline::default()),
            Tag::TableCell => {
                self.cell = true;
                self.inline = Some(Inline::default())
            }
            Tag::Heading { level, .. } => {
                self.close_loose();
                self.heading = Some(level as u8);
                self.inline = Some(Inline::default());
            }
            Tag::BlockQuote(_) => {
                self.close_loose();
                self.push(Frame::Quote(Vec::new()), false);
            }
            Tag::CodeBlock(kind) => {
                self.close_loose();
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(info) => info
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_owned(),
                    pulldown_cmark::CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((lang, String::new()));
            }
            Tag::HtmlBlock => {
                self.brk();
                self.html = Some(String::new());
            }
            Tag::List(start) => {
                self.close_loose();
                self.push(Frame::List(start, Vec::new()), false);
            }
            Tag::Item => self.push(Frame::Item(Item::default()), false),
            Tag::Table(_) => self.push(
                Frame::Table {
                    head: Vec::new(),
                    rows: Vec::new(),
                    row: Vec::new(),
                },
                false,
            ),
            Tag::Emphasis => self.italic += 1,
            Tag::Strong => self.bold += 1,
            Tag::Strikethrough => self.strike += 1,
            Tag::Link { dest_url, .. } => self.links.push(dest_url.into_string()),
            Tag::Image { dest_url, .. } => {
                self.links.push(dest_url.into_string());
                self.image += 1;
                self.text("🖼 ", false);
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        // A frame an HTML tag left open inside this one closes with it.
        if matches!(
            tag,
            TagEnd::Item | TagEnd::List(_) | TagEnd::BlockQuote(_) | TagEnd::Table
        ) {
            self.close_html_frames();
        }
        match tag {
            TagEnd::Paragraph => {
                self.close_loose();
                self.drop_html_styles();
            }
            TagEnd::Heading(_) => {
                self.drop_html_styles();
                let inline = self.inline.take().unwrap_or_default();
                let level = self.heading.take().unwrap_or(1);
                self.block(Block::Heading(level, inline));
            }
            TagEnd::BlockQuote(_) => {
                self.close_loose();
                if let Some(Frame::Quote(blocks)) = self.pop() {
                    self.block(Block::Quote(blocks));
                }
            }
            TagEnd::CodeBlock => {
                if let Some((lang, text)) = self.code.take() {
                    let text = text.strip_suffix('\n').unwrap_or(&text).to_owned();
                    self.block(Block::Code {
                        lang,
                        text,
                        marks: Vec::new(),
                    });
                }
            }
            TagEnd::HtmlBlock => {
                if let Some(html) = self.html.take() {
                    self.html(&html);
                }
                self.brk();
            }
            TagEnd::Item => {
                self.close_loose();
                self.drop_html_styles();
                if let Some(Frame::Item(item)) = self.pop()
                    && let Some(Frame::List(_, items)) = self.frames.last_mut()
                {
                    items.push(item);
                }
            }
            TagEnd::List(_) => {
                if let Some(Frame::List(start, items)) = self.pop() {
                    self.block(Block::List { start, items });
                }
            }
            TagEnd::TableCell => {
                self.cell = false;
                self.drop_html_styles();
                let cell = self.inline.take().unwrap_or_default();
                if let Some(Frame::Table { row, .. }) = self.frames.last_mut() {
                    row.push(cell);
                }
            }
            TagEnd::TableHead => {
                if let Some(Frame::Table { head, row, .. }) = self.frames.last_mut() {
                    *head = std::mem::take(row);
                }
            }
            TagEnd::TableRow => {
                if let Some(Frame::Table { rows, row, .. }) = self.frames.last_mut() {
                    rows.push(std::mem::take(row));
                }
            }
            TagEnd::Table => {
                if let Some(Frame::Table { head, rows, .. }) = self.pop() {
                    self.block(Block::Table { head, rows });
                }
            }
            TagEnd::Emphasis => self.italic = self.italic.saturating_sub(1),
            TagEnd::Strong => self.bold = self.bold.saturating_sub(1),
            TagEnd::Strikethrough => self.strike = self.strike.saturating_sub(1),
            TagEnd::Link => {
                self.links.pop();
            }
            TagEnd::Image => {
                self.links.pop();
                self.image = self.image.saturating_sub(1);
            }
            _ => {}
        }
    }
}

// ---- HTML inside Markdown (#231) -------------------------------------------------------------

/// Is this `align="center"`, or a `text-align: center` style?
fn centered(attrs: &[(String, String)]) -> bool {
    Tok::attr(attrs, "align").is_some_and(|a| a.trim().eq_ignore_ascii_case("center"))
        || Tok::attr(attrs, "style").is_some_and(|s| {
            let s: String = s
                .split_whitespace()
                .collect::<String>()
                .to_ascii_lowercase();
            s.contains("text-align:center")
        })
}

/// A link a page may follow: no script or data address.
fn safe_href(href: &str) -> Option<String> {
    let h = href.trim();
    let lower = h.to_ascii_lowercase();
    let bad = ["javascript:", "data:", "vbscript:"];
    (!h.is_empty() && !bad.iter().any(|b| lower.starts_with(b))).then(|| h.to_owned())
}

impl Builder {
    fn push(&mut self, frame: Frame, html: bool) {
        self.frames.push(frame);
        self.from_html.push(html);
    }

    fn pop(&mut self) -> Option<Frame> {
        self.from_html.pop();
        self.frames.pop()
    }

    fn top_is_html(&self) -> bool {
        self.from_html.last() == Some(&true)
    }

    /// A break between blocks. Where only a line of text fits (a cell, a heading, a summary) it is
    /// a space.
    fn brk(&mut self) {
        if self.cell || self.summary || self.heading.is_some() {
            if self.inline.as_ref().is_some_and(|i| !i.text.ends_with(' ')) {
                self.text(" ", false);
            }
        } else {
            self.close_loose();
        }
    }

    /// What a paragraph's end leaves of HTML styles and links opened and never closed.
    fn drop_html_styles(&mut self) {
        self.hbold = 0;
        self.hitalic = 0;
        self.hstrike = 0;
        self.hcode = 0;
        self.hlinks.clear();
    }

    /// The end of the body: whatever HTML left open is closed where it stands.
    fn finish(&mut self) {
        if let Some(text) = self.pre.take() {
            self.pre_block(text);
        }
        self.close_loose();
        self.cell = false;
        self.summary = false;
        while !self.frames.is_empty() {
            self.finish_top();
        }
        self.close_loose();
    }

    fn close_html_frames(&mut self) {
        self.close_loose();
        while self.top_is_html() {
            self.finish_top();
        }
    }

    /// Closes the innermost frame, whatever opened it, into the blocks.
    fn finish_top(&mut self) {
        if matches!(self.frames.last(), Some(Frame::Table { .. })) {
            self.end_cell();
            self.end_row();
        }
        self.close_loose();
        let Some(frame) = self.pop() else { return };
        match frame {
            Frame::Quote(blocks) => self.block(Block::Quote(blocks)),
            Frame::Centered(blocks) => self.block(Block::Centered(blocks)),
            Frame::Details { summary, blocks } => {
                let summary = summary.unwrap_or_else(|| Inline {
                    text: "Details".to_owned(),
                    ..Default::default()
                });
                self.block(Block::Details {
                    summary,
                    body: blocks,
                })
            }
            Frame::List(start, items) => self.block(Block::List { start, items }),
            Frame::Item(item) => match self.frames.last_mut() {
                Some(Frame::List(_, items)) => items.push(item),
                _ => item.blocks.into_iter().for_each(|b| self.block(b)),
            },
            Frame::Table { head, rows, .. } => self.block(Block::Table { head, rows }),
        }
    }

    /// A closing tag: the innermost frame of that kind HTML opened, and what is inside it.
    fn close_frame(&mut self, is: fn(&Frame) -> bool) {
        self.brk();
        let found = self
            .frames
            .iter()
            .zip(&self.from_html)
            .rposition(|(f, &html)| html && is(f));
        // Only through frames HTML opened: a Markdown one is not closed by a stray tag.
        let reach = self.from_html.iter().rposition(|h| !h).map_or(0, |i| i + 1);
        if let Some(at) = found.filter(|&at| at >= reach) {
            while self.frames.len() > at {
                self.finish_top();
            }
        }
    }

    fn end_cell(&mut self) {
        if self.cell && matches!(self.frames.last(), Some(Frame::Table { .. })) {
            self.cell = false;
            let cell = self.inline.take().unwrap_or_default();
            if let Some(Frame::Table { row, .. }) = self.frames.last_mut() {
                row.push(cell);
            }
        }
    }

    /// A row of an HTML table: the first is the head.
    fn end_row(&mut self) {
        if let Some(Frame::Table { head, rows, row }) = self.frames.last_mut()
            && !row.is_empty()
        {
            let done = std::mem::take(row);
            if head.is_empty() {
                *head = done
            } else {
                rows.push(done)
            }
        }
    }

    fn pre_block(&mut self, text: String) {
        let text = text.trim_end_matches('\n').to_owned();
        self.block(Block::Code {
            lang: String::new(),
            text,
            marks: Vec::new(),
        });
    }

    /// Raw HTML: text, tags and comments in the order they come.
    fn html(&mut self, src: &str) {
        for tok in mdhtml::tokens(src) {
            if let Some(name) = &self.skip {
                if matches!(&tok, Tok::Close(n) if n == name) {
                    self.skip = None;
                }
                continue;
            }
            if let Some(pre) = self.pre.as_mut() {
                match tok {
                    Tok::Text(t) => pre.push_str(
                        t.strip_prefix('\n')
                            .filter(|_| pre.is_empty())
                            .unwrap_or(&t),
                    ),
                    Tok::Close(n) if n == "pre" => {
                        let text = self.pre.take().unwrap_or_default();
                        self.pre_block(text);
                    }
                    _ => {}
                }
                continue;
            }
            match tok {
                Tok::Text(t) => self.html_text(&t),
                Tok::Open { name, attrs } => self.open(&name, &attrs),
                Tok::Close(name) => self.close(&name),
            }
        }
    }

    /// Text between tags: runs of white space are one space, none at a block's start.
    fn html_text(&mut self, t: &str) {
        let mut out = String::with_capacity(t.len());
        for word in t.split_inclusive(char::is_whitespace) {
            let trimmed = word.trim_end_matches(char::is_whitespace);
            out.push_str(trimmed);
            if trimmed.len() != word.len() && !out.ends_with(' ') {
                out.push(' ');
            }
        }
        if t.starts_with(char::is_whitespace) && !out.starts_with(' ') {
            out.insert(0, ' ');
        }
        let empty = self.inline.as_ref().is_none_or(|i| i.text.is_empty());
        let out = if empty { out.trim_start() } else { &out };
        if !out.is_empty() {
            self.text(out, false);
        }
    }

    fn open(&mut self, name: &str, attrs: &[(String, String)]) {
        match name {
            "br" => {
                if self.inline.as_ref().is_some_and(|i| !i.text.is_empty()) {
                    self.text("\n", false)
                }
            }
            "hr" => {
                self.brk();
                if !self.cell && !self.summary && self.heading.is_none() {
                    self.block(Block::Rule);
                }
            }
            "b" | "strong" => self.hbold += 1,
            "i" | "em" | "cite" | "dfn" | "var" => self.hitalic += 1,
            "s" | "del" | "strike" => self.hstrike += 1,
            "code" | "kbd" | "samp" | "tt" => self.hcode += 1,
            "a" => self
                .hlinks
                .push(Tok::attr(attrs, "href").and_then(safe_href)),
            "img" => {
                let Some(src) = Tok::attr(attrs, "src").filter(|s| !s.trim().is_empty()) else {
                    return;
                };
                self.hlinks.push(Some(src.trim().to_owned()));
                self.image += 1;
                self.text("🖼 ", false);
                self.text(Tok::attr(attrs, "alt").unwrap_or_default(), false);
                self.image -= 1;
                self.hlinks.pop();
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.brk();
                if !self.cell && !self.summary && self.heading.is_none() {
                    self.heading = Some(name.as_bytes()[1] - b'0');
                    self.inline = Some(Inline::default());
                }
            }
            "p" | "div" | "center" | "section" | "article" | "header" | "footer" | "main"
            | "aside" | "nav" | "figure" | "figcaption" | "address" | "dl" | "dt" | "dd" => {
                self.brk();
                let framed = (name == "center" || centered(attrs))
                    && !self.cell
                    && !self.summary
                    && self.heading.is_none();
                if framed {
                    self.push(Frame::Centered(Vec::new()), true);
                }
                self.boxes.push((name.to_owned(), framed));
            }
            "blockquote" => {
                self.brk();
                if !self.cell && !self.summary && self.heading.is_none() {
                    self.push(Frame::Quote(Vec::new()), true);
                }
            }
            "ul" | "ol" => {
                self.brk();
                if !self.cell && !self.summary && self.heading.is_none() {
                    let start = (name == "ol")
                        .then(|| Tok::attr(attrs, "start").and_then(|s| s.trim().parse().ok()))
                        .map(|s: Option<u64>| s.unwrap_or(1));
                    self.push(Frame::List(start, Vec::new()), true);
                }
            }
            "li" => {
                self.brk();
                if self.cell || self.summary || self.heading.is_some() {
                    return;
                }
                if self.top_is_html() && matches!(self.frames.last(), Some(Frame::Item(_))) {
                    self.finish_top();
                }
                if !matches!(self.frames.last(), Some(Frame::List(..))) {
                    self.push(Frame::List(None, Vec::new()), true);
                }
                self.push(Frame::Item(Item::default()), true);
            }
            "table" => {
                self.brk();
                if !self.cell && !self.summary && self.heading.is_none() {
                    let table = Frame::Table {
                        head: Vec::new(),
                        rows: Vec::new(),
                        row: Vec::new(),
                    };
                    self.push(table, true);
                }
            }
            "tr" => {
                self.end_cell();
                self.end_row();
            }
            "td" | "th" => {
                self.end_cell();
                if matches!(self.frames.last(), Some(Frame::Table { .. })) && self.top_is_html() {
                    self.cell = true;
                    self.inline = Some(Inline::default());
                } else {
                    self.brk();
                }
            }
            "details" => {
                self.brk();
                if !self.cell && !self.summary && self.heading.is_none() {
                    self.push(
                        Frame::Details {
                            summary: None,
                            blocks: Vec::new(),
                        },
                        true,
                    );
                }
            }
            "summary" => {
                self.brk();
                if self.top_is_html()
                    && matches!(
                        self.frames.last(),
                        Some(Frame::Details { summary: None, .. })
                    )
                {
                    self.summary = true;
                    self.inline = Some(Inline::default());
                }
            }
            "pre" => {
                self.brk();
                if !self.cell && !self.summary && self.heading.is_none() {
                    self.pre = Some(String::new());
                }
            }
            "script" | "style" | "title" => self.skip = Some(name.to_owned()),
            _ => {}
        }
    }

    fn close(&mut self, name: &str) {
        match name {
            "b" | "strong" => self.hbold = self.hbold.saturating_sub(1),
            "i" | "em" | "cite" | "dfn" | "var" => self.hitalic = self.hitalic.saturating_sub(1),
            "s" | "del" | "strike" => self.hstrike = self.hstrike.saturating_sub(1),
            "code" | "kbd" | "samp" | "tt" => self.hcode = self.hcode.saturating_sub(1),
            "a" => {
                self.hlinks.pop();
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                if let Some(level) = self.heading.take() {
                    let inline = self.inline.take().unwrap_or_default();
                    self.block(Block::Heading(level, inline));
                }
            }
            "p" | "div" | "center" | "section" | "article" | "header" | "footer" | "main"
            | "aside" | "nav" | "figure" | "figcaption" | "address" | "dl" | "dt" | "dd" => {
                self.brk();
                if let Some(at) = self.boxes.iter().rposition(|(n, _)| n == name) {
                    while self.boxes.len() > at {
                        let (_, framed) = self.boxes.pop().unwrap_or_default();
                        if framed {
                            self.close_frame(|f| matches!(f, Frame::Centered(_)));
                        }
                    }
                }
            }
            "blockquote" => self.close_frame(|f| matches!(f, Frame::Quote(_))),
            "ul" | "ol" => self.close_frame(|f| matches!(f, Frame::List(..))),
            "li" => {
                self.brk();
                if self.top_is_html() && matches!(self.frames.last(), Some(Frame::Item(_))) {
                    self.finish_top();
                }
            }
            "table" => self.close_frame(|f| matches!(f, Frame::Table { .. })),
            "tr" => {
                self.end_cell();
                self.end_row();
            }
            "td" | "th" => self.end_cell(),
            "details" => self.close_frame(|f| matches!(f, Frame::Details { .. })),
            "summary" if self.summary => {
                self.summary = false;
                let line = self.inline.take().unwrap_or_default();
                if let Some(Frame::Details { summary, .. }) = self.frames.last_mut() {
                    *summary = Some(line);
                }
            }
            _ => {}
        }
    }
}

const OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_GFM);

impl Builder {
    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            // `:tada:` as the hosts show it; code keeps it as written.
            Event::Text(t) if self.code.is_none() => {
                self.text(&crate::emoji::shortcodes(&t), false)
            }
            Event::Text(t) => self.text(&t, false),
            Event::Code(t) => self.text(&t, true),
            Event::Html(t) => match self.html.as_mut() {
                Some(html) => html.push_str(&t),
                None => self.html(&t),
            },
            Event::InlineHtml(t) => self.html(&t),
            Event::SoftBreak => self.text(" ", false),
            Event::HardBreak => self.text("\n", false),
            Event::Rule => {
                self.close_loose();
                self.block(Block::Rule);
            }
            Event::TaskListMarker(done) => {
                if let Some(Frame::Item(item)) = self.frames.last_mut() {
                    item.task = Some(done);
                }
            }
            Event::FootnoteReference(t) => self.text(&format!("[^{t}]"), false),
            _ => {}
        }
    }
}

/// The blocks of a markdown body, GitLab-flavoured: tables, task lists and strikethrough included.
pub fn parse(source: &str) -> Vec<Block> {
    let mut b = Builder::default();
    // Text in one piece, so a `:name:` is never split.
    for event in pulldown_cmark::TextMergeStream::new(Parser::new_ext(source, OPTIONS)) {
        b.event(event);
    }
    b.finish();
    b.root
}

/// The top-level blocks of a document, each with the source lines it takes (1-based, first and
/// last): a changed `.md` file is compared block by block and commented on at a block's line.
pub fn parse_lines(source: &str) -> Vec<(Block, (u32, u32))> {
    let mut b = Builder::default();
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (event, range) in Parser::new_ext(source, OPTIONS).into_offset_iter() {
        if depth == 0 && b.frames.is_empty() {
            start = range.start;
        }
        match &event {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
        let before = b.root.len();
        b.event(event);
        ranges.extend((before..b.root.len()).map(|_| start..range.end));
    }
    let before = b.root.len();
    b.finish();
    ranges.extend((before..b.root.len()).map(|_| start..source.len()));
    // Bytes, not text: a block's last byte may sit inside a wide letter (`ə`, `—`).
    let bytes = source.as_bytes();
    let line = |at: usize| {
        bytes[..at.min(bytes.len())]
            .iter()
            .filter(|&&c| c == b'\n')
            .count() as u32
            + 1
    };
    b.root
        .into_iter()
        .zip(ranges)
        .map(|(block, r)| {
            // A list's range takes the blank line after it.
            let text = bytes.get(r.clone()).unwrap_or_default();
            let end = r.start + text.trim_ascii_end().len();
            let first = line(r.start);
            (block, (first, line(end.saturating_sub(1)).max(first)))
        })
        .collect()
}

/// A file read as a document: `.md` and `.markdown` (#11).
pub fn is_document(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, ext)| matches!(ext.to_ascii_lowercase().as_str(), "md" | "markdown"))
}

/// Where the images of a document point, as written: markdown images and HTML `<img>` tags.
pub fn images(source: &str) -> Vec<String> {
    Parser::new_ext(source, OPTIONS)
        .flat_map(|e| match e {
            Event::Start(Tag::Image { dest_url, .. }) => vec![dest_url.into_string()],
            Event::Html(t) | Event::InlineHtml(t) => {
                html_images(&t).into_iter().map(|(src, _)| src).collect()
            }
            _ => Vec::new(),
        })
        .collect()
}

/// The `<img>` tags of some HTML: each one's `src` and `alt`.
pub fn html_images(html: &str) -> Vec<(String, String)> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = lower[from..].find("<img").map(|i| from + i) {
        let end = lower[at..].find('>').map_or(html.len(), |i| at + i);
        let tag = &html[at..end];
        if let Some(src) = attribute(tag, "src") {
            out.push((src, attribute(tag, "alt").unwrap_or_default()));
        }
        from = end;
    }
    out
}

/// The value of attribute `name` in tag `tag`, quoted or not.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find(name).map(|i| from + i) {
        from = at + name.len();
        // The whole name, not the end of another one (`data-src`).
        let before = lower[..at].chars().next_back();
        if before.is_some_and(|c| !c.is_whitespace()) {
            continue;
        }
        let rest = tag[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let value = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or_default(),
            _ => rest
                .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
                .next()
                .unwrap_or_default(),
        };
        return Some(value.to_owned());
    }
    None
}

/// The repository path an image link `src` of document `doc` names: relative to the document's
/// folder, or to the root when it starts with `/`. None for a web address or a path out of the
/// repository.
pub fn local_path(doc: &str, src: &str) -> Option<String> {
    let src = src.split(['?', '#']).next().unwrap_or_default().trim();
    if src.is_empty() || src.contains("://") || src.starts_with("//") || src.contains(':') {
        return None;
    }
    let src = src.replace("%20", " ");
    let mut parts: Vec<&str> = Vec::new();
    if !src.starts_with('/') {
        parts.extend(
            doc.rsplit_once('/')
                .map(|(dir, _)| dir.split('/'))
                .into_iter()
                .flatten(),
        );
    }
    for part in src.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Does `body` use markdown beyond plain paragraphs? Plain text needs no Rich/Raw switch.
pub fn is_rich(blocks: &[Block]) -> bool {
    blocks.iter().any(|b| match b {
        Block::Paragraph(inline) => !inline.spans.is_empty() || !mentions(&inline.text).is_empty(),
        _ => true,
    })
}

/// The `@username` mentions in `text` (#51): an `@` at the start or after a space or an opening
/// bracket, then letters, digits, `_`, `-` or `.` (not ending in `.`).
pub fn mentions(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let starts = bytes[i] == b'@'
            && (i == 0 || matches!(bytes[i - 1], b' ' | b'\n' | b'\t' | b'(' | b'['));
        if !starts {
            i += 1;
            continue;
        }
        let mut end = i + 1;
        while end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'-' | b'.'))
        {
            end += 1;
        }
        while end > i + 1 && bytes[end - 1] == b'.' {
            end -= 1;
        }
        if end > i + 1 {
            out.push(i..end);
        }
        i = end.max(i + 1);
    }
    out
}

/// The words of `source` without its markers, on one line: for the one-line previews of a comment.
pub fn plain(source: &str) -> String {
    fn walk(blocks: &[Block], out: &mut Vec<String>) {
        for b in blocks {
            match b {
                Block::Paragraph(i) | Block::Heading(_, i) => out.push(i.text.clone()),
                Block::Code { text, .. } => out.push(text.clone()),
                Block::Quote(blocks) | Block::Centered(blocks) => walk(blocks, out),
                Block::Details { summary, body } => {
                    out.push(summary.text.clone());
                    walk(body, out)
                }
                Block::List { items, .. } => {
                    for item in items {
                        walk(&item.blocks, out)
                    }
                }
                Block::Table { head, rows } => {
                    for cell in head.iter().chain(rows.iter().flatten()) {
                        out.push(cell.text.clone())
                    }
                }
                Block::Rule => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&parse(source), &mut out);
    out.join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ---- the composer's helpers ------------------------------------------------------------------

/// A button (or key) of the composer's toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Helper {
    Bold,
    Italic,
    Strike,
    Code,
    CodeBlock,
    Bullet,
    Numbered,
    Quote,
    Link,
}

impl Helper {
    /// What wraps a run of text, for the inline ones.
    fn marker(self) -> Option<&'static str> {
        match self {
            Helper::Bold => Some("**"),
            Helper::Italic => Some("_"),
            Helper::Strike => Some("~~"),
            Helper::Code => Some("`"),
            _ => None,
        }
    }

    /// What starts each line, for the line ones.
    fn prefix(self) -> Option<&'static str> {
        match self {
            Helper::Bullet => Some("- "),
            Helper::Quote => Some("> "),
            Helper::Numbered => Some("1. "),
            _ => None,
        }
    }
}

/// Applies `helper` at the caret or over the selection, the way GitHub's toolbar does: an inline
/// marker wraps the selection (or goes in empty with the caret between), and comes off when the
/// selection is already wrapped; a line marker goes at the start of every line the selection
/// touches, and comes off when they all have it.
pub fn apply(edit: &mut TextEdit, helper: Helper) {
    let text = edit.text().to_owned();
    let sel = edit.selection().unwrap_or(edit.caret()..edit.caret());
    let inner = text[sel.clone()].to_owned();
    match helper {
        Helper::Link => {
            if inner.starts_with("http://") || inner.starts_with("https://") {
                let with = format!("[]({inner})");
                let at = sel.start + 1;
                edit.replace(sel, &with, at..at);
            } else {
                let with = format!("[{inner}](url)");
                let url = sel.start + inner.len() + 3;
                edit.replace(sel, &with, url..url + 3);
            }
        }
        Helper::CodeBlock => {
            let lead = if sel.start == 0 || text[..sel.start].ends_with('\n') {
                ""
            } else {
                "\n"
            };
            let with = format!("{lead}```\n{inner}\n```");
            let at = sel.start + lead.len() + 4;
            edit.replace(sel, &with, at..at + inner.len());
        }
        Helper::Code if inner.contains('\n') => apply(edit, Helper::CodeBlock),
        _ => match helper.marker() {
            Some(m) => {
                let wrapped = sel.start >= m.len()
                    && text[..sel.start].ends_with(m)
                    && text[sel.end..].starts_with(m);
                if wrapped {
                    let outer = sel.start - m.len()..sel.end + m.len();
                    let at = outer.start;
                    edit.replace(outer, &inner, at..at + inner.len());
                } else {
                    let with = format!("{m}{inner}{m}");
                    let at = sel.start + m.len();
                    edit.replace(sel, &with, at..at + inner.len());
                }
            }
            None => prefix_lines(edit, &text, sel, helper),
        },
    }
}

fn prefix_lines(edit: &mut TextEdit, text: &str, sel: std::ops::Range<usize>, helper: Helper) {
    let start = text[..sel.start].rfind('\n').map_or(0, |i| i + 1);
    let end = text[sel.end..]
        .find('\n')
        .map_or(text.len(), |i| sel.end + i);
    let lines: Vec<&str> = text[start..end].split('\n').collect();
    let have = |line: &str| match helper {
        Helper::Numbered => numbered_prefix(line).is_some(),
        _ => helper.prefix().is_some_and(|p| line.starts_with(p)),
    };
    let all = lines.iter().all(|l| have(l));
    let block: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(n, line)| {
            if all {
                let len = match helper {
                    Helper::Numbered => numbered_prefix(line).unwrap_or(0),
                    _ => helper.prefix().map_or(0, str::len),
                };
                line[len..].to_owned()
            } else if helper == Helper::Numbered {
                format!("{}. {line}", n + 1)
            } else {
                format!("{}{line}", helper.prefix().unwrap_or_default())
            }
        })
        .collect();
    let with = block.join("\n");
    let caret = start + with.len();
    edit.replace(start..end, &with, caret..caret);
}

/// The length of `12. ` at the start of `line`, if it has one.
fn numbered_prefix(line: &str) -> Option<usize> {
    let (n, _) = line.split_once(". ")?;
    (!n.is_empty() && n.chars().all(|c| c.is_ascii_digit())).then_some(n.len() + 2)
}

/// `N. ` after a line numbered `N-1. `, else `1. `.
fn numbered_after(line: &str) -> String {
    let n = line
        .split_once(". ")
        .and_then(|(n, _)| n.trim_start().parse::<u64>().ok());
    format!("{}. ", n.map_or(1, |n| n + 1))
}

/// The list or quote marker a line starts with (`- `, `- [ ] `, `3. `, `> `), with its indent.
fn line_marker(line: &str) -> Option<String> {
    let indent = &line[..line.len() - line.trim_start().len()];
    let rest = &line[indent.len()..];
    for p in ["- [ ] ", "- [x] ", "* [ ] ", "- ", "* ", "+ ", "> "] {
        if rest.starts_with(p) {
            let p = p.replace("[x]", "[ ]");
            return Some(format!("{indent}{p}"));
        }
    }
    let (n, _) = rest.split_once(". ")?;
    let n: u64 = n.parse().ok()?;
    Some(format!("{indent}{n}. "))
}

/// Return in the composer: a list or a quote goes on to its next line; return on an empty item
/// ends the list with a blank line instead. Inside a code fence it is a plain new line.
pub fn newline(edit: &mut TextEdit) {
    if edit.selection().is_some() {
        return edit.insert("\n");
    }
    let text = edit.text().to_owned();
    let caret = edit.caret();
    let start = text[..caret].rfind('\n').map_or(0, |i| i + 1);
    let line = &text[start..caret];
    let in_code = text[..caret]
        .lines()
        .filter(|l| l.trim_start().starts_with("```"))
        .count()
        % 2
        == 1;
    match line_marker(line).filter(|_| !in_code) {
        Some(marker) if line.trim_end() == marker.trim_end() => {
            edit.replace(start..caret, "\n", start + 1..start + 1);
        }
        Some(marker) => {
            let next = match marker.trim_start().split_once(". ") {
                Some((n, _)) if n.parse::<u64>().is_ok() => {
                    let indent = &marker[..marker.len() - marker.trim_start().len()];
                    format!("{indent}{}", numbered_after(marker.trim_start()))
                }
                _ => marker,
            };
            edit.insert(&format!("\n{next}"));
        }
        None => edit.insert("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_images_are_read() {
        let html = "<p align=\"center\">\n  <img src=\"docs/logo.png\" alt='Logo' width=200>\n  <img data-src=\"x.png\" src=diagram.svg>\n</p>\n<!-- a note -->";
        assert_eq!(
            html_images(html),
            [
                ("docs/logo.png".to_owned(), "Logo".to_owned()),
                ("diagram.svg".to_owned(), String::new())
            ]
        );
        assert!(images("Text <img src=\"a.png\"> and ![b](b.png)\n").contains(&"a.png".to_owned()));
    }

    #[test]
    fn mentions_are_names_after_an_at_that_starts_a_word() {
        let text = "@juniper can you check, cc @ember.lind. mail a@b.io (@kestrel)";
        let found: Vec<&str> = mentions(text).into_iter().map(|r| &text[r]).collect();
        assert_eq!(found, ["@juniper", "@ember.lind", "@kestrel"]);
        assert!(mentions("@ alone").is_empty());
    }

    fn para(blocks: &[Block]) -> &Inline {
        match &blocks[0] {
            Block::Paragraph(i) => i,
            other => panic!("not a paragraph: {other:?}"),
        }
    }

    #[test]
    fn inline_styles_become_spans() {
        let blocks = parse("a **bold** _it_ `code` ~~gone~~ [link](https://x.y)");
        let p = para(&blocks);
        assert_eq!(p.text, "a bold it code gone link");
        let styled = |word: &str| {
            let at = p.text.find(word).unwrap();
            p.spans
                .iter()
                .find(|s| s.range.start == at)
                .unwrap_or_else(|| panic!("no span for {word}"))
                .clone()
        };
        assert!(styled("bold").style.bold);
        assert!(styled("it").style.italic);
        assert!(styled("code").style.code);
        assert!(styled("gone").style.strike);
        assert_eq!(styled("link").link.as_deref(), Some("https://x.y"));
    }

    #[test]
    fn blocks_of_a_gitlab_comment() {
        let src = "### Why\n\n- [x] done\n- [ ] todo\n\n> quoted\n\n```suggestion:-0+0\nlet x = 1;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n---";
        let blocks = parse(src);
        assert!(matches!(&blocks[0], Block::Heading(3, i) if i.text == "Why"));
        match &blocks[1] {
            Block::List { start: None, items } => {
                assert_eq!(items[0].task, Some(true));
                assert_eq!(items[1].task, Some(false));
                assert!(matches!(&items[1].blocks[0], Block::Paragraph(i) if i.text == "todo"));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(&blocks[2], Block::Quote(b) if b.len() == 1));
        assert_eq!(
            blocks[3],
            Block::Code {
                lang: "suggestion:-0+0".into(),
                text: "let x = 1;".into(),
                marks: Vec::new()
            }
        );
        assert!(
            matches!(&blocks[4], Block::Table { head, rows } if head.len() == 2 && rows.len() == 1)
        );
        assert_eq!(blocks[5], Block::Rule);
    }

    #[test]
    fn blocks_know_their_lines() {
        let src = "# Title\n\nOne\ntwo\n\n- a\n- b\n\n---\n";
        let lines: Vec<(u32, u32)> = parse_lines(src).into_iter().map(|(_, l)| l).collect();
        assert_eq!(lines, [(1, 1), (3, 4), (6, 7), (9, 9)]);
    }

    #[test]
    fn a_block_ending_in_a_wide_letter_knows_its_lines() {
        let src = "# Başlıq ə\n\nBir\niki —\n\n- ✓\n";
        let lines: Vec<(u32, u32)> = parse_lines(src).into_iter().map(|(_, l)| l).collect();
        assert_eq!(lines, [(1, 1), (3, 4), (6, 6)]);
    }

    #[test]
    fn an_image_path_is_read_from_the_document_folder() {
        assert_eq!(
            local_path("docs/a.md", "img/x.png").as_deref(),
            Some("docs/img/x.png")
        );
        assert_eq!(
            local_path("docs/a.md", "../logo.svg").as_deref(),
            Some("logo.svg")
        );
        assert_eq!(
            local_path("docs/a.md", "/assets/b.png?raw=1").as_deref(),
            Some("assets/b.png")
        );
        assert_eq!(local_path("README.md", "../up.png"), None);
        assert_eq!(local_path("README.md", "https://x.y/a.png"), None);
        assert_eq!(
            images("![logo](docs/logo.png) and ![](b.gif)"),
            ["docs/logo.png", "b.gif"]
        );
    }

    #[test]
    fn plain_text_is_not_rich() {
        assert!(!is_rich(&parse("just words\nover two lines")));
        assert!(is_rich(&parse("- a list")));
        assert!(is_rich(&parse("with `code`")));
    }

    #[test]
    fn emoji_names_show_as_emoji_but_in_code() {
        assert_eq!(
            plain("ship :white_check_mark: `:tada:`\n\n```\n:rocket:\n```"),
            "ship ✅ :tada: :rocket:"
        );
    }

    #[test]
    fn a_preview_line_has_the_words_without_the_markers() {
        assert_eq!(
            plain("### Why\n\n- **one**\n- `two`\n\n> [three](https://x)"),
            "Why one two three"
        );
    }

    fn edit(text: &str, sel: std::ops::Range<usize>) -> TextEdit {
        let mut e = TextEdit::new(text);
        e.place(sel.start, false);
        e.place(sel.end, true);
        e
    }

    #[test]
    fn an_inline_helper_wraps_the_selection_and_unwraps_it() {
        let mut e = edit("fix this now", 4..8);
        apply(&mut e, Helper::Bold);
        assert_eq!(e.text(), "fix **this** now");
        assert_eq!(e.selected_text(), Some("this"));
        apply(&mut e, Helper::Bold);
        assert_eq!(e.text(), "fix this now");
    }

    #[test]
    fn with_nothing_selected_the_caret_lands_between_the_markers() {
        let mut e = edit("a ", 2..2);
        apply(&mut e, Helper::Code);
        e.insert("x");
        assert_eq!(e.text(), "a `x`");
    }

    #[test]
    fn a_link_wraps_its_text_and_selects_the_address() {
        let mut e = edit("see docs", 4..8);
        apply(&mut e, Helper::Link);
        assert_eq!(e.text(), "see [docs](url)");
        assert_eq!(e.selected_text(), Some("url"));
    }

    #[test]
    fn line_helpers_mark_every_selected_line_and_take_it_off_again() {
        let mut e = edit("one\ntwo", 1..5);
        apply(&mut e, Helper::Bullet);
        assert_eq!(e.text(), "- one\n- two");
        let len = e.text().len();
        e.place(0, false);
        e.place(len, true);
        apply(&mut e, Helper::Bullet);
        assert_eq!(e.text(), "one\ntwo");
        let mut e = edit("a\nb", 0..3);
        apply(&mut e, Helper::Numbered);
        assert_eq!(e.text(), "1. a\n2. b");
    }

    #[test]
    fn return_continues_a_list_and_ends_it_on_an_empty_item() {
        let mut e = TextEdit::new("- a");
        newline(&mut e);
        assert_eq!(e.text(), "- a\n- ");
        newline(&mut e);
        assert_eq!(e.text(), "- a\n\n");
        assert!(matches!(
            &parse(&format!("{}after", e.text()))[1],
            Block::Paragraph(_)
        ));
        let mut e = TextEdit::new("  3. c");
        newline(&mut e);
        assert_eq!(e.text(), "  3. c\n  4. ");
        let mut e = TextEdit::new("- [x] done");
        newline(&mut e);
        assert_eq!(e.text(), "- [x] done\n- [ ] ");
        let mut e = TextEdit::new("```\n- in code");
        newline(&mut e);
        assert_eq!(e.text(), "```\n- in code\n");
    }

    // ---- HTML inside Markdown (#231) ----

    /// The words of the first paragraph with its styled runs: `**bold**`, `_italic_`, `~strike~`,
    /// `` `code` ``, `[text](link)`.
    fn show(i: &Inline) -> String {
        let mut out = String::new();
        let mut at = 0;
        for s in &i.spans {
            out.push_str(&i.text[at..s.range.start]);
            let t = &i.text[s.range.clone()];
            let t = match (s.style, &s.link) {
                (st, _) if st.image => format!("img({t}|{})", s.link.clone().unwrap_or_default()),
                (st, _) if st.bold => format!("**{t}**"),
                (st, _) if st.italic => format!("_{t}_"),
                (st, _) if st.strike => format!("~{t}~"),
                (st, _) if st.code => format!("`{t}`"),
                (_, Some(l)) => format!("[{t}]({l})"),
                _ => t.to_owned(),
            };
            out.push_str(&t);
            at = s.range.end;
        }
        out.push_str(&i.text[at..]);
        out
    }

    fn hp(src: &str) -> String {
        match parse(src).first() {
            Some(Block::Paragraph(i)) => show(i),
            other => panic!("not a paragraph: {other:?}"),
        }
    }

    #[test]
    fn html_inline_styles_and_links() {
        assert_eq!(
            hp("a <b>b</b> <strong>s</strong> <i>i</i> <em>e</em> z"),
            "a **b** **s** _i_ _e_ z"
        );
        assert_eq!(
            hp("<code>c</code> <kbd>K</kbd> <del>d</del> <s>s</s>"),
            "`c` `K` ~d~ ~s~"
        );
        assert_eq!(
            hp("x<sub>1</sub> y<sup>2</sup> <ins>n</ins> <u>u</u>"),
            "x1 y2 n u"
        );
        assert_eq!(
            hp("see <a href=\"https://x.io/a\">the page</a> now"),
            "see [the page](https://x.io/a) now"
        );
        assert_eq!(hp("<a href=\"javascript:alert(1)\">no</a>"), "no");
        assert_eq!(hp("a<br>b<br/>c<br />d"), "a\nb\nc\nd");
        assert_eq!(
            hp("<span style=\"x\">kept</span> <font color=red>text</font>"),
            "kept text"
        );
    }

    #[test]
    fn html_image_is_the_markdown_image() {
        let md = hp("![alt](a.png)");
        assert_eq!(
            hp("<img src=\"a.png\" alt=\"alt\" width=\"20\" height=20>"),
            md
        );
        let Some(Block::Paragraph(i)) = parse("<img src=\"a.png\">\n").first().cloned() else {
            panic!()
        };
        assert!(i.spans[0].style.image);
        assert!(parse("<img alt=\"none\">").is_empty());
    }

    #[test]
    fn html_blocks_become_blocks() {
        let b = parse("<p>one</p>\n\n<h2>Two <i>x</i></h2>\n\n<hr>\n\n<h6>six</h6>");
        assert!(matches!(&b[0], Block::Paragraph(i) if i.text == "one"));
        assert!(matches!(&b[1], Block::Heading(2, i) if show(i) == "Two _x_"));
        assert_eq!(b[2], Block::Rule);
        assert!(matches!(&b[3], Block::Heading(6, _)));
        let b = parse("<blockquote>quoted <b>text</b></blockquote>\n");
        let Block::Quote(inner) = &b[0] else {
            panic!("{b:?}")
        };
        assert!(matches!(&inner[0], Block::Paragraph(i) if show(i) == "quoted **text**"));
    }

    #[test]
    fn html_pre_keeps_its_lines() {
        let b = parse("<pre>\nfn a() {\n  1 &lt; 2\n}\n</pre>\n");
        assert!(matches!(&b[0], Block::Code { text, .. } if text == "fn a() {\n  1 < 2\n}"));
    }

    #[test]
    fn html_lists_nest() {
        let b = parse(
            "<ul>\n<li>one</li>\n<li>two\n<ol start=\"3\"><li>x</li><li>y</li></ol></li>\n</ul>\n",
        );
        let Block::List { start: None, items } = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(items.len(), 2);
        let Block::List {
            start: Some(3),
            items: inner,
        } = &items[1].blocks[1]
        else {
            panic!("{:?}", items[1])
        };
        assert_eq!(inner.len(), 2);
        // `<li>` left open, no `<ul>` at all.
        let b = parse("<li>a\n<li>b\n");
        assert!(matches!(&b[0], Block::List { items, .. } if items.len() == 2));
    }

    #[test]
    fn html_table_is_a_table() {
        let b = parse(
            "<table>\n<tr><th>A</th><th>B</th></tr>\n<tr><td>1</td><td><b>2</b></td></tr>\n<tr><td>3<td>4\n</table>\n",
        );
        let Block::Table { head, rows } = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(
            head.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
            ["A", "B"]
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(show(&rows[0][1]), "**2**");
        assert_eq!(rows[1].len(), 2);
    }

    #[test]
    fn html_details_has_a_summary_and_a_body() {
        let b = parse(
            "<details>\n<summary>More <b>info</b></summary>\n\nA **body**.\n\n- item\n\n</details>\n\nafter\n",
        );
        let Block::Details { summary, body } = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(show(summary), "More **info**");
        assert!(matches!(&body[0], Block::Paragraph(i) if show(i) == "A **body**."));
        assert!(matches!(&body[1], Block::List { .. }));
        assert!(matches!(&b[1], Block::Paragraph(i) if i.text == "after"));
        // One in a line, without a summary.
        let b = parse("<details><summary>S</summary>body</details>");
        let Block::Details { summary, body } = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!((summary.text.as_str(), body.len()), ("S", 1));
        let b = parse("<details>\ntext\n</details>");
        assert!(matches!(&b[0], Block::Details { summary, .. } if summary.text == "Details"));
    }

    #[test]
    fn html_centred_blocks() {
        let b = parse(
            "<p align=\"center\">\n  <img src=\"logo.png\" alt=\"Logo\">\n</p>\n\n<div align=center>hi</div>\n\n<center>c</center>\n\nplain\n",
        );
        assert!(matches!(&b[0], Block::Centered(i) if matches!(&i[0], Block::Paragraph(_))));
        assert!(matches!(&b[1], Block::Centered(_)));
        assert!(matches!(&b[2], Block::Centered(_)));
        assert!(matches!(&b[3], Block::Paragraph(_)));
    }

    #[test]
    fn html_comments_and_unknown_tags() {
        assert!(parse("<!-- a note -->\n").is_empty());
        assert_eq!(hp("a <!-- hidden --> b"), "a  b");
        assert!(parse("<!--\nmulti\nline\n-->\n\ntext").len() == 1);
        assert!(parse("<script>alert(1)</script>\n\n<style>p{}</style>\n").is_empty());
    }

    #[test]
    fn malformed_html_never_loses_text() {
        // Unclosed styles end with their paragraph.
        let b = parse("<b>bold\n\nnext");
        assert!(matches!(&b[1], Block::Paragraph(i) if show(i) == "next"));
        // Unclosed blocks close at the end.
        let b = parse("<details>\n<summary>S\n\nbody\n");
        assert!(matches!(&b[0], Block::Details { .. }), "{b:?}");
        let b = parse("<blockquote>q\n\n<ul><li>a");
        assert!(matches!(&b[0], Block::Quote(_)), "{b:?}");
        // Stray closers do nothing.
        assert_eq!(hp("a </b></a> b"), "a  b");
        assert_eq!(plain("a </p> b"), "a b");
        assert!(!parse("</details>\n\ntext").is_empty());
        // A tag is closed inside Markdown's own list item.
        let b = parse("- a <details>\n- b\n");
        assert!(
            matches!(&b[0], Block::List { items, .. } if items.len() == 2),
            "{b:?}"
        );
        // No `>`: text.
        assert_eq!(hp("<b is it"), "<b is it");
    }

    #[test]
    fn angle_brackets_in_text_stay_text() {
        assert_eq!(hp("a < b and b > c"), "a < b and b > c");
        assert_eq!(
            hp("a Vec<T> and Option<String> here"),
            "a Vec<T> and Option<String> here"
        );
        assert_eq!(hp("x <3 y"), "x <3 y");
        assert_eq!(hp("<T> alone"), "<T> alone");
        assert_eq!(hp("`<b>` in code"), "`<b>` in code");
        let b = parse("```\n<b>raw</b>\n```\n");
        assert!(matches!(&b[0], Block::Code { text, .. } if text == "<b>raw</b>"));
    }

    #[test]
    fn plain_markdown_is_as_before() {
        let b = parse("# T\n\ntext **b** and `c`\n\n> q\n\n| a | b |\n|---|---|\n| 1 | 2 |\n");
        assert_eq!(b.len(), 4);
        assert_eq!(
            plain("a <b>b</b> <details><summary>S</summary>body</details>"),
            "a b S body"
        );
    }
}
