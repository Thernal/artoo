//! The viewer over the window (Design `design/explore/tasks/issue.html?att=view`): the page dimmed under it, a head
//! with the image's name, `2 of 2 · 214 KB · Ember Lind, 3 h ago`, `Download`, `Open in Jira ↗` and ✕; the image
//! fitted in the room left, never drawn larger than it is; round ← → beside it while the page has more than one.

use artoo_images_api::{Image, Picture, Viewer};
use artoo_shell_api::{Frame, Overlay, Parts, Snap, Toast};
use artoo_ui_kit::hover::Hover;
use artoo_ui_kit::{ROW_RADIUS, format, hover, icons, px, theme, ui_zoom};
use gpui::{
    App, AppContext, ClickEvent, Context, Entity, FontWeight, IntoElement, KeyDownEvent,
    MouseButton, Render, Rgba, SharedString, Window, div, img, prelude::*,
};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

gpui::actions!(
    artoo,
    [
        /// The image just saved, shown in Finder (the toast's button).
        ShowSavedImage
    ]
);

/// The head's height.
const HEAD: f32 = 48.;
/// Room kept around the image, and a step button's width.
const PAD: f32 = 24.;
const STEP: f32 = 36.;

pub(crate) fn parts(frame: Rc<dyn Frame>, _: &mut Window, cx: &mut App) -> Parts {
    let open = Rc::new(Cell::new(false));
    let state = cx.new(|_| State {
        frame: frame.clone(),
        open: open.clone(),
        images: Vec::new(),
        at: 0,
        saved: None,
    });
    frame
        .services()
        .set::<Rc<dyn Viewer>>(Rc::new(Handle(state.clone())));
    let (key, close, shot, reveal) = (state.clone(), state.clone(), state.clone(), state.clone());
    let sample: Snap = Rc::new(move |_, cx| {
        let images = sample();
        shot.update(cx, |s, cx| s.show(images, 1, cx));
    });
    Parts {
        overlay: Some(Overlay {
            view: state.into(),
            open,
            key: Rc::new(move |event, _, cx| key.update(cx, |s, cx| s.key(event, cx))),
            close: Rc::new(move |cx| close.update(cx, |s, cx| s.hide(cx))),
            poll: None,
        }),
        actions: vec![(
            Box::new(ShowSavedImage),
            Rc::new(move |_, cx| {
                if let Some(path) = reveal.read(cx).saved.clone() {
                    cx.reveal_path(&path);
                }
            }),
        )],
        snapshots: vec![("image-viewer", sample)],
        ..Default::default()
    }
}

/// The tab's viewer as its features ask it.
struct Handle(Entity<State>);

impl Viewer for Handle {
    fn open(&self, images: Vec<Image>, at: usize, cx: &mut App) {
        self.0.update(cx, |s, cx| s.show(images, at, cx));
    }

    fn shown(&self, cx: &App) -> Option<String> {
        let s = self.0.read(cx);
        (s.open.get())
            .then(|| s.images.get(s.at))
            .flatten()
            .map(|i| i.key.clone())
    }
}

struct State {
    frame: Rc<dyn Frame>,
    /// Whether it is up; the shell reads it.
    open: Rc<Cell<bool>>,
    images: Vec<Image>,
    /// The one shown.
    at: usize,
    /// The file last saved to Downloads, for the toast's Show in Finder.
    saved: Option<PathBuf>,
}

impl State {
    fn show(&mut self, images: Vec<Image>, at: usize, cx: &mut Context<Self>) {
        if images.is_empty() {
            return;
        }
        self.at = at.min(images.len() - 1);
        self.images = images;
        self.open.set(true);
        cx.notify();
        self.frame.redraw(cx);
    }

    fn hide(&mut self, cx: &mut Context<Self>) {
        self.open.set(false);
        cx.notify();
        self.frame.redraw(cx);
    }

    /// The next image, or the one before; round past the ends.
    fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        self.at = stepped(self.at, self.images.len(), forward);
        cx.notify();
        self.frame.redraw(cx);
    }

    fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let m = &event.keystroke.modifiers;
        match event.keystroke.key.as_str() {
            "escape" => self.hide(cx),
            "left" if !m.modified() => self.step(false, cx),
            "right" if !m.modified() => self.step(true, cx),
            "s" if m.platform && !m.alt && !m.shift => self.download(cx),
            "j" if m.platform && m.alt => self.open_source(cx),
            _ => {}
        }
    }

    /// The image shown, written to Downloads under a name not taken; the toast says where.
    fn download(&mut self, cx: &mut Context<Self>) {
        let Some(image) = self.images.get(self.at) else {
            return;
        };
        let Some((picture, _)) = picture(&image.picture, cx) else {
            self.frame
                .toast(Toast::new(format!("{} is still loading", image.name)), cx);
            return;
        };
        let Some(dir) = std::env::home_dir().map(|h| h.join("Downloads")) else {
            return;
        };
        let name = with_extension(&image.name, &picture);
        let path = free_path(&dir, &name);
        match std::fs::write(&path, picture.bytes()) {
            Ok(()) => {
                let saved = path
                    .file_name()
                    .map_or(name, |n| n.to_string_lossy().into_owned());
                self.saved = Some(path);
                self.frame.toast(
                    Toast::new(format!("Saved {saved} to Downloads"))
                        .button("Show in Finder", ShowSavedImage),
                    cx,
                );
            }
            Err(e) => self
                .frame
                .error(format!("Could not save {name}: {e}").into(), cx),
        }
    }

    fn open_source(&mut self, cx: &mut Context<Self>) {
        if let Some(source) = self.images.get(self.at).and_then(|i| i.source.as_ref()) {
            cx.open_url(&source.url);
        }
    }
}

/// `at` moved one step in `n` images, round past the ends.
fn stepped(at: usize, n: usize, forward: bool) -> usize {
    match n {
        0 | 1 => 0,
        _ if forward => (at + 1) % n,
        _ => (at + n - 1) % n,
    }
}

/// The picture of `picture`, once it is in: decoded already, or from the web through the kit's pictures.
fn picture(picture: &Picture, cx: &App) -> Option<artoo_ui_kit::pictures::Picture> {
    match picture {
        Picture::Decoded(image) => {
            let size = imagesize::blob_size(image.bytes())
                .ok()
                .map(|s| (s.width as u32, s.height as u32));
            Some((image.clone(), size))
        }
        Picture::Web { src, project } => {
            let pictures = artoo_ui_kit::pictures::get(cx)?;
            match project {
                Some(project) if src.starts_with("/uploads/") => pictures.upload(project, src),
                _ => pictures.image(src),
            }
        }
    }
}

/// `name` with the extension its bytes say, when it has none.
fn with_extension(name: &str, image: &gpui::Image) -> String {
    if Path::new(name).extension().is_some() {
        return name.to_owned();
    }
    let ext = match image.format() {
        gpui::ImageFormat::Png => "png",
        gpui::ImageFormat::Jpeg => "jpg",
        gpui::ImageFormat::Webp => "webp",
        gpui::ImageFormat::Gif => "gif",
        gpui::ImageFormat::Svg => "svg",
        gpui::ImageFormat::Bmp => "bmp",
        gpui::ImageFormat::Tiff => "tiff",
        gpui::ImageFormat::Ico => "ico",
        gpui::ImageFormat::Pnm => "pnm",
    };
    let name = if name.trim().is_empty() {
        "image"
    } else {
        name
    };
    format!("{name}.{ext}")
}

/// `dir/name`, or `name (2).ext`, `(3)`… when it is taken.
fn free_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map_or(name.into(), |s| s.to_string_lossy());
    let ext = path.extension().map(|e| e.to_string_lossy());
    (2..)
        .map(|n| {
            dir.join(match &ext {
                Some(ext) => format!("{stem} ({n}).{ext}"),
                None => format!("{stem} ({n})"),
            })
        })
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// `w` × `h` fitted in `room`, never larger than it is.
fn fitted((w, h): (f32, f32), (rw, rh): (f32, f32)) -> (f32, f32) {
    let (w, h) = (w.max(1.), h.max(1.));
    let k = (rw.max(1.) / w).min(rh.max(1.) / h).min(1.);
    (w * k, h * k)
}

/// What the head says after the name: `2 of 2 · 214 KB · Ember Lind, 3 h ago`.
fn facts(at: usize, n: usize, bytes: Option<usize>, added: Option<&(String, i64)>) -> String {
    let mut facts = Vec::new();
    if n > 1 {
        facts.push(format!("{} of {n}", at + 1));
    }
    if let Some(bytes) = bytes {
        facts.push(format::bytes(bytes));
    }
    if let Some((who, when)) = added {
        facts.push(if who.is_empty() {
            format::ago(*when)
        } else {
            format!("{who}, {}", format::ago(*when))
        });
    }
    facts.join(" · ")
}

fn scrim() -> Rgba {
    Rgba {
        a: 0.94,
        ..theme::base()
    }
}

/// A button of the head: its icon, its words, its key on a long hover.
fn head_button(
    id: &'static str,
    icon: Option<&'static str>,
    label: String,
    tip: String,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .h(px(28.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(ROW_RADIUS))
        .border_1()
        .border_color(theme::island_border())
        .cursor_pointer()
        .text_size(px(13.))
        .text_color(theme::text())
        .hover(|s| s.bg(theme::hover()))
        .children(icon.map(|i| icons::icon(i).size(px(14.)).text_color(theme::muted())))
        .child(label)
        .tip(tip)
}

/// A round step button beside the image; none while the page has one image.
fn step_button(
    id: &'static str,
    forward: bool,
    shown: bool,
    cx: &mut Context<State>,
) -> gpui::AnyElement {
    if !shown {
        return div().w(px(STEP)).flex_none().into_any_element();
    }
    div()
        .id(id)
        .occlude()
        .flex_none()
        .size(px(STEP))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(theme::hover())
        .cursor_pointer()
        .hover(|s| s.bg(theme::selected()))
        .child(
            icons::icon(if forward { "forward" } else { "back" })
                .size(px(18.))
                .text_color(theme::muted()),
        )
        .tip(if forward {
            "Next  →"
        } else {
            "Previous  ←"
        })
        .on_click(cx.listener(move |s, _: &ClickEvent, _, cx| s.step(forward, cx)))
        .into_any_element()
}

impl Render for State {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open.get() {
            return div().into_any_element();
        }
        let Some(image) = self.images.get(self.at).cloned() else {
            return div().into_any_element();
        };
        let n = self.images.len();
        let shown = picture(&image.picture, cx);
        let bytes = shown.as_ref().map(|(p, _)| p.bytes().len());
        let zoom = ui_zoom();
        let view = window.viewport_size();
        let room = (
            f32::from(view.width) / zoom - 2. * (PAD + STEP + PAD),
            f32::from(view.height) / zoom - HEAD - 2. * PAD,
        );

        let head = div()
            .id("image-viewer-head")
            .occlude()
            .flex_none()
            .h(px(HEAD))
            // Clear of the window's buttons where the title bar is the app's (macOS).
            .pl(px(artoo_core::platform::TITLEBAR_INSET.max(20.)))
            .pr(px(12.))
            .flex()
            .items_center()
            .gap(px(10.))
            .bg(theme::panel())
            .border_b_1()
            .border_color(theme::island_border())
            .text_size(px(13.))
            .text_color(theme::text())
            .child(
                icons::icon("image")
                    .size(px(16.))
                    .text_color(theme::faint()),
            )
            .child(
                hover::cut("image-viewer-name", image.name.clone())
                    .flex_shrink(1.)
                    .min_w_0()
                    .font_weight(FontWeight::SEMIBOLD),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(facts(self.at, n, bytes, image.added.as_ref())),
            )
            .child(div().flex_1())
            .child(
                head_button(
                    "image-viewer-download",
                    Some("download"),
                    "Download".to_owned(),
                    "Download  ⌘S".to_owned(),
                )
                .on_click(cx.listener(|s, _: &ClickEvent, _, cx| s.download(cx))),
            )
            .children(image.source.as_ref().map(|source| {
                head_button(
                    "image-viewer-source",
                    None,
                    format!("Open in {} ↗", source.name),
                    format!("Open in {}  ⌥⌘J", source.name),
                )
                .on_click(cx.listener(|s, _: &ClickEvent, _, cx| s.open_source(cx)))
            }))
            .child(
                div()
                    .id("image-viewer-close")
                    .flex_none()
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(ROW_RADIUS))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::hover()))
                    .child(icons::icon("x").size(px(16.)).text_color(theme::muted()))
                    .tip("Close  esc")
                    .on_click(cx.listener(|s, _: &ClickEvent, _, cx| s.hide(cx))),
            );

        let picture: gpui::AnyElement = match shown {
            Some((picture, size)) => {
                // A size its header does not say (an SVG draws at any): a box of a page's picture.
                let natural = size.map_or((640., 480.), |(w, h)| (w as f32, h as f32));
                let (w, h) = fitted(natural, room);
                div()
                    .id("image-viewer-picture")
                    .occlude()
                    .flex_none()
                    .child(
                        img(picture)
                            .w(px(w))
                            .h(px(h))
                            .object_fit(gpui::ObjectFit::Contain)
                            .rounded(px(4.)),
                    )
                    .into_any_element()
            }
            None => div()
                .id("image-viewer-picture")
                .occlude()
                .flex_none()
                .w(px(480.))
                .h(px(270.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .border_1()
                .border_color(theme::island_border())
                .text_size(px(13.))
                .text_color(theme::muted())
                .child(SharedString::from("Loading…"))
                .into_any_element(),
        };

        div()
            .id("image-viewer")
            .absolute()
            .size_full()
            .flex()
            .flex_col()
            .bg(scrim())
            .on_mouse_down(MouseButton::Left, cx.listener(|s, _, _, cx| s.hide(cx)))
            .child(head)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p(px(PAD))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(PAD))
                    .child(step_button("image-viewer-back", false, n > 1, cx))
                    .child(picture)
                    .child(step_button("image-viewer-next", true, n > 1, cx)),
            )
            .into_any_element()
    }
}

/// Two pictures a snapshot opens the viewer on, the second shown: what a page with two screenshots hands it.
fn sample() -> Vec<Image> {
    let shot = |w: u32, h: u32, accent: [u8; 3]| -> Arc<gpui::Image> {
        let picture = image::RgbaImage::from_fn(w, h, |x, y| {
            let row = y / 28;
            let bar = 24 + (row * 37 % 9) * 40;
            if y < 30 {
                image::Rgba([48, 52, 62, 255])
            } else if y % 28 < 14 && x > 24 && x < bar + 24 && row % 5 != 0 {
                let [r, g, b] = if row % 3 == 0 {
                    accent
                } else {
                    [150, 160, 176]
                };
                image::Rgba([r, g, b, 255])
            } else {
                image::Rgba([30, 33, 40, 255])
            }
        });
        let mut png = std::io::Cursor::new(Vec::new());
        picture
            .write_to(&mut png, image::ImageFormat::Png)
            .expect("a PNG in memory");
        Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            png.into_inner(),
        ))
    };
    let day = format::now() - 3 * 3600;
    vec![
        Image {
            key: "refund-replay.png".to_owned(),
            name: "refund-replay.png".to_owned(),
            picture: Picture::Decoded(shot(960, 540, [97, 175, 239])),
            added: Some(("Juniper Hale".to_owned(), day - 6 * 86400)),
            source: None,
        }
        .source("Jira", "https://example.atlassian.net/browse/PAY-412"),
        Image {
            key: "ledger-remainders.png".to_owned(),
            name: "ledger-remainders.png".to_owned(),
            picture: Picture::Decoded(shot(1280, 720, [224, 108, 117])),
            added: Some(("Ember Lind".to_owned(), day)),
            source: None,
        }
        .source("Jira", "https://example.atlassian.net/browse/PAY-412"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_go_round_the_page() {
        assert_eq!(stepped(1, 2, true), 0);
        assert_eq!(stepped(0, 2, false), 1);
        assert_eq!(stepped(0, 3, true), 1);
        assert_eq!(stepped(0, 1, true), 0);
    }

    #[test]
    fn an_image_is_never_drawn_larger_than_it_is() {
        assert_eq!(fitted((400., 300.), (1000., 800.)), (400., 300.));
        assert_eq!(fitted((2000., 1000.), (1000., 800.)), (1000., 500.));
        assert_eq!(fitted((1000., 2000.), (1000., 800.)), (400., 800.));
    }

    #[test]
    fn the_head_says_which_size_and_who() {
        let at = format::now() - 3 * 3600;
        assert_eq!(
            facts(1, 2, Some(219_136), Some(&("Ember Lind".to_owned(), at))),
            "2 of 2 · 214.0 KB · Ember Lind, 3 h ago"
        );
        assert_eq!(facts(0, 1, None, None), "");
    }

    #[test]
    fn a_saved_image_takes_a_free_name() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(free_path(dir.path(), "a.png"), dir.path().join("a.png"));
        std::fs::write(dir.path().join("a.png"), b"x").unwrap();
        std::fs::write(dir.path().join("a (2).png"), b"x").unwrap();
        assert_eq!(free_path(dir.path(), "a.png"), dir.path().join("a (3).png"));
    }
}
