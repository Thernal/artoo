//! A link and what is done with it (#217; Design round 46): who claims it (`artoo_shell_api::links`, then the open
//! project's own host pages, then the browser), its menu wherever it is right-clicked, its click, its address copied.

use super::Workspace;
use super::menu::{Act, Entry};
use artoo_shell_api::links::{self, Claim, Kind};
use artoo_ui_kit::menu::Lead;
use artoo_ui_kit::toast::{Tone, Words};
use artoo_ui_kit::{marked, px, theme};
use gpui::{
    AnyElement, App, ClipboardItem, Context, FontWeight, Pixels, Point, Window, div, point,
    prelude::*,
};
use std::rc::Rc;

/// A link as the tab knows it.
pub(super) struct Linked {
    /// What it points at, as written: a web address, or an issue key's (`artoo_shell_api::issues`).
    pub address: String,
    pub claim: Claim,
    /// The kind that claims it, when a feature or a plugin registered it.
    kind: Option<Rc<dyn Kind>>,
}

impl Workspace {
    /// The kinds of link the tab's features claim.
    fn link_kinds(&self) -> Vec<Rc<dyn Kind>> {
        (self.parts.iter())
            .flat_map(|p| p.link_kinds.clone())
            .collect()
    }

    /// What `address` is: a registered kind's, the open project's host page (a commit, a branch), or the web's;
    /// none for anything else, a relative address or a key no tracker knows.
    pub(super) fn linked(&self, address: &str, cx: &App) -> Option<Linked> {
        // A request's number a feature drew: the request's page on the host.
        if let Some(number) = address.strip_prefix(marked::REQUEST_SCHEME) {
            let url = self.request_url(number.parse().ok()?, cx)?;
            return self.linked(&url, cx);
        }
        let (claim, kind) = links::claimed(cx, &self.link_kinds(), address)
            .or_else(|| Some((self.host_page(address, cx)?, None)))
            .or_else(|| Some((Claim::web(address)?, None)))?;
        Some(Linked {
            address: address.to_owned(),
            claim,
            kind,
        })
    }

    /// A link to a page of the open project on its host, which opens in Artoo.
    fn host_page(&self, address: &str, cx: &App) -> Option<Claim> {
        let web = self.web.as_ref()?;
        let link = artoo_hosts_api::get(cx).link(address)?;
        (link.base == web.base).then(|| Claim {
            name: link.describe(),
            icon: match link.target {
                artoo_core::links::Target::Commit(_) => "commit",
                artoo_core::links::Target::Branch(_) => "branch",
                artoo_core::links::Target::Request { .. } => "pull-request",
            },
            url: Some(address.to_owned()),
            site: Some(web.name().to_owned()),
            copy_name: false,
        })
    }

    /// A link's right click at `at`: its menu; false when `address` is no link the tab knows.
    pub(super) fn link_menu(
        &mut self,
        address: &str,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(linked) = self.linked(address, cx) else {
            return false;
        };
        let groups = self.link_groups(&linked, cx);
        self.open_menu(at, groups, cx);
        // The menu's keys act on this link while it is open.
        self.menu_link = Some(linked.address);
        true
    }

    /// A link's menu: Open, the browser, Copy link (and its name), then its kind's own rows.
    pub(super) fn link_groups(&self, linked: &Linked, cx: &App) -> Vec<Vec<Entry>> {
        let Linked {
            address,
            claim,
            kind,
        } = linked;
        let glyph = |name| Lead::Icon(name, theme::muted());
        let mut open = Vec::new();
        // A web page nobody claims has the browser alone to open in.
        if !claim.name.is_empty() {
            let to = address.clone();
            open.push(
                Entry::new(
                    format!("Open {}", claim.name),
                    Act::new(move |this, _, cx| this.open_link_here(&to, cx)),
                )
                .lead(glyph(claim.icon))
                .key("\u{21b5}"),
            );
        }
        let browser = match &claim.url {
            Some(url) => Entry::new(claim.open_in(), Act::open_url(url.clone()))
                .key("\u{2318}-click, \u{2325}\u{21b5}"),
            None => Entry::off(format!("{}: no address", claim.open_in())),
        };
        open.push(browser.lead(glyph("external")));
        let mut copy = Vec::new();
        let (url, name) = (claim.url.clone(), claim.name.clone());
        copy.push(
            match url {
                Some(url) => Entry::new(
                    "Copy link",
                    Act::new(move |this, _, cx| this.link_copied(url.clone(), &name, cx)),
                )
                .key("\u{21e7}\u{2318}C"),
                None => Entry::off("Copy link: no address"),
            }
            .lead(glyph("link")),
        );
        if claim.copy_name {
            let name = claim.name.clone();
            copy.push(
                Entry::new(format!("Copy {name}"), Act::copy_said(name.clone(), name))
                    .lead(glyph("copy")),
            );
        }
        let own: Vec<Entry> = (kind.iter())
            .flat_map(|k| k.rows(address, cx))
            .map(|item| {
                let entry = match item.run.clone() {
                    Some(run) => Entry::new(item.label.clone(), Act::part(run)),
                    None => Entry::off(item.label.clone()),
                };
                entry.key(item.key).lead(glyph(item.icon.unwrap_or("dot")))
            })
            .collect();
        [open, copy, own]
            .into_iter()
            .filter(|g| !g.is_empty())
            .collect()
    }

    /// The link whose menu is open: what the menu's keys act on.
    fn menu_focus(&self, cx: &App) -> Option<Linked> {
        let address = self.menu_link.as_ref()?;
        if !self.ctx_menu.read(cx).is_open() {
            return None;
        }
        self.linked(address, cx)
    }

    /// The request row picked in the list that has the keyboard: the focused link where no menu is open.
    fn picked_link(&self, cx: &App) -> Option<Linked> {
        let listing = self.nav.space == super::spaces::REQUESTS
            && self.at_root_list()
            && self.zone == super::zones::Zone::List
            && !self.inbox
            && !self.settings_open
            && !self.overlay_open()
            && !self.ctx_menu.read(cx).is_open();
        let picked = (self.requests_api())
            .filter(|_| listing)
            .and_then(|r| r.picked_row(cx))?;
        (self.listed_requests(cx).iter()).find(|m| m.iid == picked)?;
        self.linked(&marked::request(picked), cx)
    }

    /// A click on a link, and its menu's Open: its kind's own way, else in Artoo when it is a page of the open
    /// project, else the browser.
    pub(super) fn open_link_here(&mut self, address: &str, cx: &mut Context<Self>) {
        if !self.open_url_here(address, cx)
            && let Some(url) = self.linked(address, cx).and_then(|l| l.claim.url)
        {
            cx.open_url(&url);
        }
    }

    /// `address`'s kind opens it its own way.
    pub(super) fn open_by_kind(&self, address: &str, cx: &mut App) -> bool {
        links::claimed(cx, &self.link_kinds(), address)
            .and_then(|(_, kind)| kind)
            .is_some_and(|kind| kind.open(address, cx))
    }

    /// The focused link's keys, ↵, ⌥↵ and ⇧⌘C: true when a link had them. The link of an open menu, which then
    /// closes; ⌥↵ and ⇧⌘C also the picked request row's, ⌥↵ after the newest toast's button.
    pub(super) fn link_key(&mut self, key: LinkKey, cx: &mut Context<Self>) -> bool {
        let linked = match self.menu_focus(cx) {
            Some(linked) => {
                self.close_menu(cx);
                linked
            }
            None => {
                let row = match key {
                    // ↵ on a row is the list's own.
                    LinkKey::Open => None,
                    LinkKey::Browser if self.press_toast(cx) => return true,
                    LinkKey::Browser | LinkKey::Copy => self.picked_link(cx),
                };
                let Some(linked) = row else {
                    return false;
                };
                linked
            }
        };
        match (key, linked.claim.url) {
            (LinkKey::Open, url) => match url.filter(|_| linked.claim.name.is_empty()) {
                Some(url) => cx.open_url(&url),
                None => self.open_link_here(&linked.address, cx),
            },
            (LinkKey::Browser, Some(url)) => cx.open_url(&url),
            (LinkKey::Copy, Some(url)) => self.link_copied(url, &linked.claim.name, cx),
            (LinkKey::Browser | LinkKey::Copy, None) => {}
        }
        cx.notify();
        true
    }

    /// ⇧F10 and the menu key: the picked request row's menu, at the row.
    pub(super) fn open_focused_menu(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        let Some(linked) = self.picked_link(cx) else {
            return false;
        };
        let at = match artoo_ui_kit::hover::picked_row(window, cx) {
            Some((_, row)) => point(row.left() + px(24.), row.bottom()),
            None => window.mouse_position(),
        };
        self.link_menu(&linked.address, at, cx)
    }

    /// The line at the window's foot while ⌘ marks a link (Design round 46): where a ⌘-click goes.
    pub(super) fn render_link_foot(&self, window: &Window, cx: &App) -> Option<AnyElement> {
        let address = marked::address(window)?;
        let claim = self.linked(&address, cx)?.claim;
        let url = claim.url.clone()?;
        Some(
            div()
                .absolute()
                .left(px(8.))
                .bottom(px(10.))
                .max_w(px(640.))
                .h(px(26.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(6.))
                .border_1()
                .border_color(theme::island_border())
                .bg(theme::panel())
                .shadow_lg()
                .text_size(px(11.))
                .text_color(theme::muted())
                .child(
                    artoo_ui_kit::icons::icon("external")
                        .size(px(13.))
                        .flex_none()
                        .text_color(theme::muted()),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(theme::text())
                        .font_weight(FontWeight::MEDIUM)
                        .child(claim.open_in()),
                )
                .child(
                    artoo_ui_kit::hover::cut("link-foot", bare(&url).to_owned())
                        .min_w_0()
                        .font_family(theme::code_font())
                        .text_color(theme::faint()),
                )
                .into_any_element(),
        )
    }

    /// A link's address copied: the toast says whose, with the address, and a screen reader hears `Link copied`.
    pub(super) fn link_copied(&mut self, url: String, name: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
        let words = copied_words(&url, name);
        self.toasts.update(cx, |t, cx| t.show(words, true, cx));
    }
}

/// A key on the focused link.
#[derive(Clone, Copy)]
pub(super) enum LinkKey {
    Open,
    Browser,
    Copy,
}

/// An address as it is shown: without its scheme.
fn bare(url: &str) -> &str {
    url.split_once("://").map_or(url, |(_, rest)| rest)
}

/// Copy link's toast: `Link to !412 copied` over the address without its scheme.
fn copied_words(url: &str, name: &str) -> Words {
    let said = match name {
        "" => "Link copied".to_owned(),
        name => format!("Link to {name} copied"),
    };
    Words::new(said)
        .about(bare(url).to_owned())
        .tone(Tone::Success)
        .announce("Link copied")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copied_links_toast_names_it_and_shows_the_address() {
        let words = copied_words(
            "https://gitlab.example.com/a/b/-/merge_requests/412",
            "!412",
        );
        assert_eq!(words.text.as_ref(), "Link to !412 copied");
        assert_eq!(
            words.about.as_deref(),
            Some("gitlab.example.com/a/b/-/merge_requests/412")
        );
        assert_eq!(words.announce.as_deref(), Some("Link copied"));
        assert_eq!(
            copied_words("https://example.com", "").text.as_ref(),
            "Link copied"
        );
    }
}
