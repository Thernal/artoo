//! Links: where the tab's texts have them, and what each is.
//!
//! **Found in text** (#171, D11): a feature offers a kind of them in its parts (`Parts::links`) — the requests'
//! numbers, `!398` on GitLab and `#12` on GitHub —, and every text the tab draws through the kit's markdown
//! (comments, descriptions, a commit's message) shows them as links, followed as the written ones are.
//!
//! **The registry** (#217; Design round 46): a module or a plugin says what a link of its kind is (`Kind`) — what it
//! matches (`claim`), its card on a long hover (`card`), what a click does (`open`), its menu's rows of its own
//! (`rows`) — for its parts' tab
//! (`Parts::link_kinds`) or the whole app (`add`, a plugin's `install`). Issue keys a tracker names
//! (`crate::issues`) are claimed without more; a web address nobody claims is the browser's (`Claim::web`), and what
//! brings no card has the default one (the host, the address). A card is the kind's own design, in the kit's frame
//! (`hover::card_frame`), its actions `glyphs` at its head's end and no foot (the hover rules, H1).

use crate::{MenuItem, issues};
use artoo_ui_kit::hover::card_action;
use artoo_ui_kit::markdown::FindLinks;
use gpui::{AnyView, App, Div, Global, Pixels, Point, div, prelude::*};
use std::ops::Range;
use std::rc::Rc;

/// A kind of link a feature finds in text, in its `order` among the others.
#[derive(Clone)]
pub struct Pattern {
    /// Its id, for the log and the tests: `requests.number`.
    pub id: &'static str,
    pub order: u16,
    /// Its links in a text: each one's bytes and its address, in order and apart.
    pub find: FindLinks,
}

/// Every pattern's links in `text`, in the patterns' order: one found where an earlier one found none, then sorted by
/// where they start.
pub fn find(patterns: &[Pattern], text: &str, cx: &App) -> Vec<(Range<usize>, String)> {
    let mut found: Vec<(Range<usize>, String)> = Vec::new();
    for pattern in patterns {
        for (range, url) in (pattern.find)(text, cx) {
            if !found
                .iter()
                .any(|(r, _)| r.start < range.end && range.start < r.end)
            {
                found.push((range, url));
            }
        }
    }
    found.sort_by_key(|(r, _)| r.start);
    found
}

/// A link as the one claiming it knows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Claim {
    /// What it is called in a menu and a toast: `!412`, `PAY-412`; empty for a web page nobody claims.
    pub name: String,
    /// The glyph of its `Open` row (`pull-request`), by its Lucide name.
    pub icon: &'static str,
    /// Where the browser goes for it; none when it has no address on the web.
    pub url: Option<String>,
    /// What is at that address (`GitLab`, `Jira`); none says `the browser`.
    pub site: Option<String>,
    /// Its name is worth copying alone (`!412`): the menu says `Copy !412` after Copy link.
    pub copy_name: bool,
}

impl Claim {
    /// A web address nobody claims: the browser's.
    pub fn web(address: &str) -> Option<Self> {
        (address.starts_with("http://") || address.starts_with("https://")).then(|| Self {
            name: String::new(),
            icon: "link",
            url: Some(address.to_owned()),
            site: None,
            copy_name: false,
        })
    }

    /// `Open in GitLab`, `Open in the browser`.
    pub fn open_in(&self) -> String {
        format!("Open in {}", self.site.as_deref().unwrap_or("the browser"))
    }
}

/// A kind of link a module or a plugin claims. `address` is what the link points at: a web address, or an issue
/// key's (`issues::SCHEME`). Everything it is asked while the tab is drawn or updating: it reads, it does not fetch,
/// and `open` defers what updates the tab.
pub trait Kind {
    /// Its id, for the log and the tests: `requests.link`.
    fn id(&self) -> &'static str;

    /// What the link is, when it is one of this kind's.
    fn claim(&self, address: &str, cx: &App) -> Option<Claim>;

    /// A click on it: true when this kind opened it. Otherwise the tab does — the open project's host pages in
    /// Artoo, anything else in the browser.
    fn open(&self, _address: &str, _cx: &mut App) -> bool {
        false
    }

    /// Its card on a long hover over the link; none has the default one. Its head ends with `glyphs`.
    fn card(&self, _address: &str, _cx: &mut App) -> Option<AnyView> {
        None
    }

    /// Rows of its own under the menu's (Open, Open in …, Copy link).
    fn rows(&self, _address: &str, _cx: &App) -> Vec<MenuItem> {
        Vec::new()
    }
}

#[derive(Default)]
struct Kinds(Vec<Rc<dyn Kind>>);

impl Global for Kinds {}

/// Offers a kind of link to every tab (a plugin's `install`).
pub fn add(cx: &mut App, kind: Rc<dyn Kind>) {
    cx.default_global::<Kinds>().0.push(kind);
}

/// Who claims `address`, and as what: the tab's own kinds (`of_tab`) first, then the ones offered to every tab, in
/// the order they were, then the trackers' issue keys. None: nobody does.
pub fn claimed(
    cx: &App,
    of_tab: &[Rc<dyn Kind>],
    address: &str,
) -> Option<(Claim, Option<Rc<dyn Kind>>)> {
    let offered = cx.try_global::<Kinds>().map(|k| k.0.as_slice());
    let by_kind = (of_tab.iter())
        .chain(offered.unwrap_or_default())
        .find_map(|k| Some((k.claim(address, cx)?, Some(k.clone()))));
    by_kind.or_else(|| Some((issues::claim(cx, issues::key_of(address)?)?, None)))
}

/// A right click on a link drawn where the tab is out of reach (a name's issue key, a card): the tab opens the
/// link's menu there.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = artoo_links, no_json)]
pub struct ShowLinkMenu {
    pub address: String,
    pub at: Point<Pixels>,
}

/// A link's address to be copied, from a card: the tab copies it and says so.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = artoo_links, no_json)]
pub struct CopyLinkOf {
    pub address: String,
}

/// A card's two actions, at its head's end: ⧉ copies the link at `address`, ↗ opens its claim's address in the
/// browser, its tooltip `Claim::open_in`.
pub fn glyphs(address: &str, claim: &Claim) -> Div {
    let address = address.to_owned();
    let open = claim.open_in();
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(artoo_ui_kit::px(2.))
        .child(
            card_action("link-copy", "copy", "Copy link").on_click(move |_, window, cx| {
                let copy = CopyLinkOf {
                    address: address.clone(),
                };
                window.dispatch_action(Box::new(copy), cx);
            }),
        )
        .children(claim.url.clone().map(|url| {
            card_action("link-open", "external", open).on_click(move |_, _, cx| cx.open_url(&url))
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Host(&'static str);

    impl Kind for Host {
        fn id(&self) -> &'static str {
            self.0
        }
        fn claim(&self, address: &str, _: &App) -> Option<Claim> {
            address.starts_with("https://jira.").then(|| Claim {
                name: self.0.to_owned(),
                ..Claim::default()
            })
        }
    }

    #[test]
    fn the_tabs_kinds_claim_first_and_an_unclaimed_address_is_the_browsers() {
        gpui::TestAppContext::single().update(|cx| {
            let address = "https://jira.example/browse/A-1";
            assert!(claimed(cx, &[], address).is_none());
            add(cx, Rc::new(Host("offered")));
            let own: Vec<Rc<dyn Kind>> = vec![Rc::new(Host("own"))];
            let name = |own: &[Rc<dyn Kind>]| claimed(cx, own, address).map(|c| c.0.name);
            assert_eq!(name(&own), Some("own".to_owned()));
            assert_eq!(name(&[]), Some("offered".to_owned()));
            assert!(claimed(cx, &own, "https://example.com").is_none());
            let web = Claim::web("https://example.com").unwrap();
            assert_eq!(web.open_in(), "Open in the browser");
            assert_eq!(Claim::web("artoo-issue:A-1"), None);
        });
    }
}
