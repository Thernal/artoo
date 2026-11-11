//! The image viewer's contract (#211): an image opened whole is the app's, never a page's own. A page hands the
//! viewer its images, in the order it shows them, and the one to show; the viewer draws them over the window, steps
//! through them, saves one and opens it where it lives. A merge request's description and comments, a diff's images
//! and a plugin's pages open theirs here.
//!
//! The viewer is a service of the tab (`Rc<dyn Viewer>` in its services); a plugin reaches it the same way, through
//! the `Frame` its parts are made with.

use gpui::App;
use std::sync::Arc;

/// Where an image's picture comes from.
#[derive(Clone)]
pub enum Picture {
    /// Decoded already: a diff's side, a plugin's bytes.
    Decoded(Arc<gpui::Image>),
    /// From the web, through the kit's pictures (`artoo_ui_kit::pictures`): `src` as the text writes it, and the
    /// project's address when it is an upload GitLab writes as `/uploads/…`, read with the host's token.
    Web {
        src: String,
        project: Option<String>,
    },
}

/// Where an image lives, for `Open in <name> ↗`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    /// The place's name: `GitLab`, `GitHub`, `Jira`.
    pub name: String,
    pub url: String,
}

/// One of a page's images.
#[derive(Clone)]
pub struct Image {
    /// What the page marks its tile by while it is open (`Viewer::shown`): its `src`, a file's path.
    pub key: String,
    /// Its file's name: `ledger-remainders.png`.
    pub name: String,
    pub picture: Picture,
    /// Who added it and when (Unix seconds), when the page knows.
    pub added: Option<(String, i64)>,
    pub source: Option<Source>,
}

impl Image {
    /// An image named by the last part of its address (`/uploads/…/shot%201.png` → `shot 1.png`), else by `alt`.
    pub fn web(src: &str, alt: &str, project: Option<&str>) -> Self {
        Self {
            key: src.to_owned(),
            name: name_of(src).unwrap_or_else(|| alt.trim().to_owned()),
            picture: Picture::Web {
                src: src.to_owned(),
                project: project.map(str::to_owned),
            },
            added: None,
            source: None,
        }
    }

    pub fn added(mut self, who: impl Into<String>, at: i64) -> Self {
        self.added = Some((who.into(), at));
        self
    }

    pub fn source(mut self, name: impl Into<String>, url: impl Into<String>) -> Self {
        self.source = Some(Source {
            name: name.into(),
            url: url.into(),
        });
        self
    }
}

/// The file's name at the end of an address, its `%xx` read; `None` when the address ends in no name.
pub fn name_of(src: &str) -> Option<String> {
    let path = src.split(['?', '#']).next()?;
    let last = path.trim_end_matches('/').rsplit('/').next()?;
    let name = unescape(last);
    (!name.is_empty() && name.contains('.')).then_some(name)
}

/// `%xx` read as the bytes they stand for; a broken one kept as written.
fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = (bytes[i] == b'%')
            .then(|| text.get(i + 1..i + 3))
            .flatten()
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match hex {
            Some(b) => {
                out.push(b);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The tab's viewer, a service its features offer (`Rc<dyn Viewer>`).
pub trait Viewer {
    /// `images` opened whole over the window, `at` shown first.
    fn open(&self, images: Vec<Image>, at: usize, cx: &mut App);

    /// The key of the image shown while the viewer is open: the page marks its tile.
    fn shown(&self, cx: &App) -> Option<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_is_named_by_its_address() {
        assert_eq!(
            name_of("/uploads/5b2c/refund%20replay.png").as_deref(),
            Some("refund replay.png")
        );
        assert_eq!(
            name_of("https://x.test/a/ledger.png?width=400#x").as_deref(),
            Some("ledger.png")
        );
        assert_eq!(name_of("https://x.test/assets/12345"), None);
        assert_eq!(
            name_of("https://x.test/a/100%.png").as_deref(),
            Some("100%.png")
        );
    }

    #[test]
    fn an_address_with_no_name_takes_the_alt() {
        let image = Image::web("https://user-images.test/12345", " the ledger ", None);
        assert_eq!(image.name, "the ledger");
        assert_eq!(image.key, "https://user-images.test/12345");
    }
}
