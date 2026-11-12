//! A request's images opened whole in the tab's viewer (#211): a picture clicked in its description or in a
//! discussion opens there with the request's other images, in the order its page shows them — the description's,
//! then each discussion's notes' —, each with who wrote it, when, and its address on the host.

use crate::requests::Requests;
use artoo_core::markdown;
use artoo_images_api::{Image, Viewer};
use artoo_shell_api::Services;
use artoo_ui_kit::markdown::{Selecting, resolve};
use gpui::{App, Entity};
use std::rc::Rc;

impl Requests {
    /// The open request's images, as its page shows them.
    pub(crate) fn images(&self) -> Vec<Image> {
        let Some(m) = self.current.and_then(|iid| self.find(iid)) else {
            return Vec::new();
        };
        let description = match &self.details {
            Some(d) if !d.description.trim().is_empty() => &d.description,
            _ => &m.description,
        };
        let web = self.web.as_ref();
        let base = web.map(|w| w.base.as_str());
        let host = if web.is_some_and(|w| w.github) {
            "GitHub"
        } else {
            "GitLab"
        };
        let notes = (self.threads.iter().flat_map(|t| &t.notes))
            .map(|n| (n.body.as_str(), n.author.as_str(), n.created));
        std::iter::once((description.as_str(), m.author.as_str(), m.created))
            .chain(notes)
            .flat_map(|(body, who, at)| {
                written(body).into_iter().map(move |(src, alt)| {
                    let image = Image::web(&src, &alt, base).added(who, at);
                    let url = match base {
                        Some(base) => Some(resolve(&src, Some(base))),
                        None => src.contains("://").then(|| src.clone()),
                    };
                    match url {
                        Some(url) => image.source(host, url),
                        None => image,
                    }
                })
            })
            .collect()
    }
}

/// The pictures a text draws, in order, as the kit's markdown draws them in a comment: a paragraph's images, in a
/// quote or a list too (a heading's, a table's and HTML's are drawn as written); each one's address and its words.
fn written(body: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for block in markdown::parse(body) {
        collect(&block, &mut found);
    }
    found
}

fn collect(block: &markdown::Block, found: &mut Vec<(String, String)>) {
    use markdown::Block;
    match block {
        Block::Paragraph(i) => found.extend((i.spans.iter()).filter(|s| s.style.image).map(|s| {
            (
                s.link.clone().unwrap_or_default(),
                i.text[s.range.clone()].trim().to_owned(),
            )
        })),
        Block::Quote(blocks) => blocks.iter().for_each(|b| collect(b, found)),
        Block::List { items, .. } => {
            (items.iter().flat_map(|i| &i.blocks)).for_each(|b| collect(b, found))
        }
        _ => {}
    }
}

/// `sel` with a picture's click opening the request's images in the tab's viewer and the one open marked; as it
/// was where the tab has no viewer.
pub(crate) fn viewing(requests: &Entity<Requests>, sel: Selecting, cx: &App) -> Selecting {
    let Some(viewer) = viewer(requests, cx) else {
        return sel;
    };
    let me = requests.downgrade();
    let shown = viewer.shown(cx);
    Selecting {
        image: Some(Rc::new(move |src, _, cx| {
            let Some(me) = me.upgrade() else {
                return;
            };
            let rq = me.read(cx);
            let mut images = rq.images();
            let at = match images.iter().position(|i| i.key == src) {
                Some(at) => at,
                // Not the request's own (a comment being written): alone.
                None => {
                    let base = rq.web.as_ref().map(|w| w.base.clone());
                    images = vec![Image::web(src, "", base.as_deref())];
                    0
                }
            };
            viewer.open(images, at, cx);
        })),
        shown,
        ..sel
    }
}

/// The tab's viewer, through the services the requests were made in.
fn viewer(requests: &Entity<Requests>, cx: &App) -> Option<Rc<dyn Viewer>> {
    let services: Rc<Services> = requests.read(cx).services.upgrade()?;
    services.get::<Rc<dyn Viewer>>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_core::git::WebRemote;
    use artoo_core::mr::{Mr, Note, Thread};

    fn note(author: &str, body: &str, created: i64) -> Note {
        Note {
            id: created as u64,
            author: author.to_owned(),
            login: String::new(),
            body: body.to_owned(),
            created,
            edited: false,
        }
    }

    #[test]
    fn a_requests_images_are_its_descriptions_then_its_notes() {
        let mut rq = Requests {
            current: Some(12),
            web: Some(WebRemote {
                base: "https://gitlab.test/pay/service".to_owned(),
                github: false,
            }),
            ..Default::default()
        };
        rq.list.push(Mr {
            iid: 12,
            author: "Juniper Hale".to_owned(),
            created: 100,
            description: "Before:\n\n![before](/uploads/a1/before.png)\n\n- ![after](https://cdn.test/after.png)\n\n<img src=\"/uploads/c3/raw.png\">"
                .to_owned(),
            ..Default::default()
        });
        rq.threads.push(Thread {
            id: "d1".to_owned(),
            notes: vec![
                note(
                    "Ember Lind",
                    "Looks off:\n\n> ![ledger](/uploads/b2/ledger%20x.png)",
                    200,
                ),
                note("Juniper Hale", "No pictures here.", 300),
            ],
            path: None,
            new_line: None,
            old_line: None,
            resolved: false,
            resolvable: false,
            resolved_by: None,
            resolved_at: 0,
            commit: None,
            outdated: false,
        });

        let images = rq.images();
        let names: Vec<_> = images.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["before.png", "after.png", "ledger x.png"]);
        assert_eq!(images[0].added, Some(("Juniper Hale".to_owned(), 100)));
        assert_eq!(images[2].added, Some(("Ember Lind".to_owned(), 200)));
        let source = images[0].source.as_ref().unwrap();
        assert_eq!(source.name, "GitLab");
        assert_eq!(
            source.url,
            "https://gitlab.test/pay/service/uploads/a1/before.png"
        );
        assert_eq!(
            images[1].source.as_ref().unwrap().url,
            "https://cdn.test/after.png"
        );
        assert_eq!(images[0].key, "/uploads/a1/before.png");
    }
}
