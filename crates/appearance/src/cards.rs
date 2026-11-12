//! A theme drawn small (Design #12 C, `docs/specs/themes.md`): Appearance's own, so it goes where its page goes — the
//! sample diff under the Theme rows, the editor's preview of a request's header, a piece of diff and a discussion, and
//! the editor's rows of tokens. The screen keeps the theme being edited and what a click does.

use artoo_core::themes::{self, T, TOKENS, Theme};
use artoo_ui_kit::format::plural;
use artoo_ui_kit::{ROW_RADIUS, hover, icons, px, theme};
use gpui::{Div, FontWeight, HighlightStyle, Rgba, StyledText, div, prelude::*, rgba};

pub fn c(t: &Theme, tok: T) -> Rgba {
    rgba(t.get(tok))
}

/// The editor's rows: the tokens, then the parse tree's roles (`TOKENS.len()` + n, #13).
pub const ROWS: usize = TOKENS.len() + themes::ROLES;

/// Row `i`'s colour in `t`: a token's, or a role's — the token it falls back to when `t` gives it
/// none.
pub fn value(t: &Theme, i: usize) -> u32 {
    use artoo_core::highlight::role;
    let Some(r) = i.checked_sub(TOKENS.len()) else {
        return t.colors[i];
    };
    t.roles[r].unwrap_or(match role::ROLES[r].token {
        n @ 1..=8 => t.colors[T::Code1 as usize + n as usize - 1],
        role::MUTED => t.get(T::Text2),
        _ => t.get(T::Text1),
    })
}

/// Whether `t` sets row `i` itself.
pub fn own(t: &Theme, i: usize) -> bool {
    match i.checked_sub(TOKENS.len()) {
        None => t.own.contains(&i),
        Some(r) => t.own_roles.contains(&r),
    }
}

/// Whether row `i` is text, read on every surface: the text tokens and every role.
pub fn is_text(i: usize) -> bool {
    i >= TOKENS.len() || themes::TEXT.iter().any(|&x| x as usize == i)
}

/// Row `i`'s name and what it is for.
pub fn row_name(i: usize) -> (String, String) {
    use artoo_core::highlight::role;
    match i.checked_sub(TOKENS.len()) {
        None => (TOKENS[i].0.to_owned(), TOKENS[i].2.to_owned()),
        Some(r) => {
            let look = &role::ROLES[r];
            let falls = match look.token {
                n @ 1..=8 => format!("code-0{n}"),
                role::MUTED => "content-02".to_owned(),
                _ => "content-01".to_owned(),
            };
            let style = match (look.bold, look.italic) {
                (true, _) => " · semibold",
                (_, true) => " · italic",
                _ => "",
            };
            (
                format!("code.{}", look.name),
                format!("role · falls back to {falls}{style}"),
            )
        }
    }
}

/// Pieces of a sample line and the code token each is drawn in.
pub type Runs = Vec<(std::ops::Range<usize>, T)>;

/// Sample Kotlin coloured as the highlighter would: keywords, types (a capital), calls, members,
/// numbers, strings, comments and operators.
pub fn kotlin_runs(line: &str) -> Runs {
    const KEYWORDS: [&str; 7] = ["class", "private", "val", "var", "fun", "by", "return"];
    let b = line.as_bytes();
    let mut runs = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        if line[i..].starts_with("//") {
            runs.push((i..b.len(), T::Code6));
            break;
        }
        if b[i] == b'"' {
            i = line[i + 1..].find('"').map_or(b.len(), |e| i + e + 2);
            runs.push((start..i, T::Code3));
        } else if b[i].is_ascii_digit() {
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            runs.push((start..i, T::Code4));
        } else if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let word = &line[start..i];
            let token = if KEYWORDS.contains(&word) {
                Some(T::Code1)
            } else if word.starts_with(|c: char| c.is_ascii_uppercase()) {
                Some(T::Code5)
            } else if b.get(i) == Some(&b'(') {
                Some(T::Code2)
            } else if start > 0 && b[start - 1] == b'.' {
                Some(T::Code7)
            } else {
                None
            };
            runs.extend(token.map(|t| (start..i, t)));
        } else {
            if matches!(b[i], b'=' | b'<' | b'>') {
                runs.push((i..i + 1, T::Code8));
            }
            i += 1;
        }
    }
    runs
}

/// The preview's piece of diff (Design #12 C and D's sample): (kind -1 / 0 / 1, old and new
/// numbers, the line, the word marked as changed).
pub const SAMPLE: [(i8, u32, u32, &str, &str); 9] = [
    (0, 12, 12, "class LoginViewModel(", ""),
    (0, 13, 13, "    private val auth: AuthRepository,", ""),
    (0, 14, 14, ") : ViewModel() {", ""),
    (
        -1,
        15,
        0,
        "    var state by mutableStateOf(LoginState())",
        "mutableStateOf",
    ),
    (
        1,
        0,
        15,
        "    private val _state = MutableStateFlow(LoginState())",
        "MutableStateFlow",
    ),
    (
        1,
        0,
        16,
        "    val state: StateFlow<LoginState> = _state",
        "",
    ),
    (0, 16, 17, "    // One source of truth for the screen", ""),
    (0, 17, 18, "    fun submit(email: String) {", ""),
    (
        0,
        18,
        19,
        "        if (email.isBlank()) return show(\"Email is empty\", 3)",
        "",
    ),
];

/// The sample drawn as the diff draws it: the surface and the plain text the window's, the tints
/// and the syntax colours the code theme's (D16).
pub fn sample_diff(window: &Theme, code: &Theme) -> Div {
    let number = |n: u32, tint: Option<Rgba>| {
        div()
            .w(px(34.))
            .h_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .pr(px(8.))
            .text_color(c(window, T::Text3))
            .when_some(tint, |s, t| s.bg(t))
            .child(if n == 0 { String::new() } else { n.to_string() })
    };
    div()
        .py(px(4.))
        .bg(c(window, T::Bg3))
        .font_family(theme::code_font())
        .text_size(px(12.))
        .text_color(c(window, T::Text1))
        .children(SAMPLE.iter().map(|&(kind, old, new, text, word)| {
            let (line, gutter, mark) = match kind {
                -1 => (Some(T::Removed1), Some(T::Removed2), T::Removed3),
                1 => (Some(T::Added1), Some(T::Added2), T::Added3),
                _ => (None, None, T::Added3),
            };
            let gutter = gutter.map(|g| c(code, g));
            // Each byte's colour and whether it is the changed word, run together.
            let mut look: Vec<(Option<T>, bool)> = vec![(None, false); text.len()];
            for (r, t) in kotlin_runs(text) {
                for l in &mut look[r] {
                    l.0 = Some(t);
                }
            }
            if let Some(at) = (!word.is_empty()).then(|| text.find(word)).flatten() {
                for l in &mut look[at..at + word.len()] {
                    l.1 = true;
                }
            }
            let mut runs = Vec::new();
            let mut from = 0;
            for i in 1..=look.len() {
                if i == look.len() || look[i] != look[from] {
                    let (color, marked) = look[from];
                    if color.is_some() || marked {
                        runs.push((
                            from..i,
                            HighlightStyle {
                                color: color.map(|t| c(code, t).into()),
                                background_color: marked.then(|| c(code, mark).into()),
                                ..Default::default()
                            },
                        ));
                    }
                    from = i;
                }
            }
            div()
                .h(px(19.))
                .flex()
                .items_center()
                .when_some(line, |s, l| s.bg(c(code, l)))
                .child(number(old, gutter))
                .child(number(new, gutter))
                .child(
                    div()
                        .pl(px(10.))
                        .whitespace_nowrap()
                        .child(StyledText::new(text).with_highlights(runs)),
                )
        }))
}

/// The preview of the editor (Design #12 C): the app in theme `t` — a request's header, the
/// sample diff and a selected discussion.
pub fn theme_preview(t: &Theme) -> Div {
    let header = div()
        .px(px(10.))
        .h(px(40.))
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(13.))
        .text_color(c(t, T::Text2))
        .child(
            icons::icon("pull-request")
                .size(px(16.))
                .text_color(c(t, T::Text2)),
        )
        .child("!398")
        .child(
            hover::cut("sample-title", "Refactor the login screen state")
                .flex_1()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(c(t, T::Text1)),
        )
        .child(
            div()
                .px(px(10.))
                .h(px(24.))
                .flex()
                .items_center()
                .rounded(px(ROW_RADIUS))
                .bg(c(t, T::Accent))
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(c(t, T::Text5))
                .child("Approve"),
        );
    let discussion = div()
        .m(px(10.))
        .p(px(10.))
        .rounded(px(8.))
        .border_2()
        .border_color(c(t, T::Focus))
        .bg(c(t, T::Bg2))
        .flex()
        .flex_col()
        .gap(px(8.))
        .text_size(px(12.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .size(px(22.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(c(t, T::Code5))
                        .text_size(px(10.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(c(t, T::Bg1))
                        .child("EL"),
                )
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(c(t, T::Text1))
                        .child("Ember Lind"),
                )
                .child(div().text_color(c(t, T::Text3)).child("2 h ago"))
                .child(div().flex_1())
                .child(
                    div()
                        .px(px(6.))
                        .rounded(px(4.))
                        .bg(c(t, T::AttentionBg))
                        .text_size(px(11.))
                        .text_color(c(t, T::Attention))
                        .child("open"),
                ),
        )
        .child(
            div()
                .text_color(c(t, T::Text1))
                .child("Does the old state survive a rotation? StateFlow keeps it, but the screen reads it once."),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .flex_1()
                        .h(px(28.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .rounded(px(ROW_RADIUS))
                        .border_1()
                        .border_color(c(t, T::Border1))
                        .bg(c(t, T::Bg1))
                        .text_color(c(t, T::Text3))
                        .child("Reply…"),
                )
                .child(
                    div()
                        .px(px(10.))
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .rounded(px(ROW_RADIUS))
                        .bg(c(t, T::Bg4))
                        .text_color(c(t, T::Text1))
                        .child("Resolve"),
                ),
        );
    div()
        .rounded(px(10.))
        .overflow_hidden()
        .border_1()
        .border_color(c(t, T::Border1))
        .bg(c(t, T::Bg1))
        .child(header)
        .child(
            div()
                .mx(px(8.))
                .rounded(px(6.))
                .overflow_hidden()
                .child(sample_diff(t, t)),
        )
        .child(discussion)
}

pub fn small_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .px(px(10.))
        .h(px(26.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(ROW_RADIUS))
        .border_1()
        .border_color(theme::island_border())
        .bg(theme::panel())
        .text_size(px(12.))
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .child(label.into())
}

/// A tag for a theme: default, light, yours, its problems (true: drawn as one).
pub fn tag_of(t: &Theme) -> Option<(String, bool)> {
    if !t.problems.is_empty() {
        return Some((plural(t.problems.len(), "problem"), true));
    }
    let word = if t.id == "one-dark" {
        "default"
    } else if !t.built_in && t.plugin.is_none() {
        "JSON"
    } else if t.light() {
        "light"
    } else {
        return None;
    };
    Some((word.into(), false))
}
