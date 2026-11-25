//! HTML inside Markdown (#231): a body's raw HTML read as tags and text, for `markdown` to put
//! into the blocks and styles it already has. Pure reading; nothing is fetched or run here.
//!
//! Only the names of HTML's own elements are tags. Anything else that looks like one (`<T>`, a
//! generic in a sentence about `Vec<T>`) is text, as is a `<` that opens no tag (`a < b`).

/// What the reader finds.
#[derive(Debug, PartialEq)]
pub(crate) enum Tok {
    Text(String),
    Open {
        name: String,
        attrs: Vec<(String, String)>,
    },
    Close(String),
}

impl Tok {
    pub(crate) fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
        attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// HTML's elements: the ones the renderer maps, and the ones it drops keeping their text.
const ELEMENTS: &[&str] = &[
    "a",
    "abbr",
    "address",
    "article",
    "aside",
    "b",
    "bdi",
    "bdo",
    "big",
    "blockquote",
    "body",
    "br",
    "caption",
    "center",
    "cite",
    "code",
    "col",
    "colgroup",
    "data",
    "dd",
    "del",
    "details",
    "dfn",
    "div",
    "dl",
    "dt",
    "em",
    "figcaption",
    "figure",
    "font",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hr",
    "html",
    "i",
    "img",
    "ins",
    "kbd",
    "label",
    "li",
    "main",
    "mark",
    "nav",
    "ol",
    "p",
    "picture",
    "pre",
    "q",
    "s",
    "samp",
    "script",
    "section",
    "small",
    "source",
    "span",
    "strike",
    "strong",
    "style",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "time",
    "title",
    "tr",
    "tt",
    "u",
    "ul",
    "var",
    "wbr",
];

/// `src` cut into text and tags; comments are left out.
pub(crate) fn tokens(src: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut rest = src;
    let flush = |text: &mut String, out: &mut Vec<Tok>| {
        if !text.is_empty() {
            out.push(Tok::Text(entities(text)));
            text.clear();
        }
    };
    while let Some(at) = rest.find('<') {
        text.push_str(&rest[..at]);
        rest = &rest[at..];
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = r.split_once("-->").map_or("", |(_, x)| x);
        } else if rest.starts_with("<!") || rest.starts_with("<?") {
            rest = rest.split_once('>').map_or("", |(_, x)| x);
        } else if let Some((tok, len)) = tag(rest) {
            flush(&mut text, &mut out);
            out.push(tok);
            rest = &rest[len..];
        } else {
            text.push('<');
            rest = &rest[1..];
        }
    }
    text.push_str(rest);
    flush(&mut text, &mut out);
    out
}

/// The tag at the start of `s` (which starts with `<`) and its length; None when it is not one.
fn tag(s: &str) -> Option<(Tok, usize)> {
    let body = &s[1..];
    let (closing, body) = match body.strip_prefix('/') {
        Some(b) => (true, b),
        None => (false, body),
    };
    let name_len = body
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .unwrap_or(body.len());
    let name = body[..name_len].to_ascii_lowercase();
    if !body.starts_with(|c: char| c.is_ascii_alphabetic()) || !ELEMENTS.contains(&name.as_str()) {
        return None;
    }
    // To the `>` that is not inside a quoted value.
    let mut quote = None;
    let mut end = None;
    for (i, c) in body[name_len..].char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => {
                end = Some(name_len + i);
                break;
            }
            _ => {}
        }
    }
    let end = end?;
    let len = s.len() - body.len() + end + 1;
    if closing {
        return Some((Tok::Close(name), len));
    }
    Some((
        Tok::Open {
            attrs: attributes(&body[name_len..end]),
            name,
        },
        len,
    ))
}

/// `name="value" other='v' bare=1 flag`, names lower-cased, values with their entities read.
fn attributes(mut s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    loop {
        s = s.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        if s.is_empty() {
            return out;
        }
        let name_end = s
            .find(|c: char| c.is_whitespace() || c == '=' || c == '/')
            .unwrap_or(s.len());
        let name = s[..name_end].to_ascii_lowercase();
        s = s[name_end..].trim_start();
        let mut value = String::new();
        if let Some(r) = s.strip_prefix('=') {
            let r = r.trim_start();
            match r.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    let inner = &r[1..];
                    let close = inner.find(q).unwrap_or(inner.len());
                    value = entities(&inner[..close]);
                    s = inner.get(close + 1..).unwrap_or_default();
                }
                _ => {
                    let close = r.find(char::is_whitespace).unwrap_or(r.len());
                    value = entities(r[..close].trim_end_matches('/'));
                    s = &r[close..];
                }
            }
        }
        if !name.is_empty() {
            out.push((name, value));
        }
    }
}

/// The common character references read: `&amp;`, `&lt;`, `&#39;`, `&#x1F600;`…
fn entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let read = rest.find(';').filter(|&i| i <= 10).and_then(|i| {
            let name = &rest[1..i];
            let c = match name {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                "nbsp" => ' ',
                "mdash" => '—',
                "ndash" => '–',
                "hellip" => '…',
                _ => {
                    let n = name.strip_prefix('#')?;
                    let code = match n.strip_prefix(['x', 'X']) {
                        Some(h) => u32::from_str_radix(h, 16).ok()?,
                        None => n.parse().ok()?,
                    };
                    char::from_u32(code)?
                }
            };
            Some((c, i + 1))
        });
        match read {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(src: &str) -> Vec<String> {
        tokens(src)
            .into_iter()
            .map(|t| match t {
                Tok::Text(t) => format!("'{t}'"),
                Tok::Open { name, .. } => format!("<{name}>"),
                Tok::Close(name) => format!("</{name}>"),
            })
            .collect()
    }

    #[test]
    fn tags_text_and_comments() {
        assert_eq!(
            names("<B>a &amp; b</B><!-- hid <b> -->c"),
            ["<b>", "'a & b'", "</b>", "'c'"]
        );
    }

    #[test]
    fn what_is_no_tag_stays_text() {
        assert_eq!(
            names("a < b and Vec<T> or <3"),
            ["'a < b and Vec<T> or <3'"]
        );
        assert_eq!(names("<b>x"), ["<b>", "'x'"]);
        assert_eq!(names("<b"), ["'<b'"]);
        assert_eq!(names("<img src=\"a>b"), ["'<img src=\"a>b'"]);
    }

    #[test]
    fn attributes_quoted_bare_and_flags() {
        let Tok::Open { attrs, .. } =
            &tokens("<img SRC='a b.png' width=20 hidden alt=\"x>y\"/>")[0]
        else {
            panic!()
        };
        assert_eq!(Tok::attr(attrs, "src"), Some("a b.png"));
        assert_eq!(Tok::attr(attrs, "width"), Some("20"));
        assert_eq!(Tok::attr(attrs, "alt"), Some("x>y"));
        assert_eq!(Tok::attr(attrs, "hidden"), Some(""));
    }
}
