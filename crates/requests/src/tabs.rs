//! A merge request's Overview and Commits (#73), a view the tab puts under the request's bar while one of them is
//! its section on screen: Overview the requests' (`page`) with its facts from the branches' and the review's
//! contracts, Commits the request's line over the branch's commits, each with the version it came with
//! (`artoo_branches_api::Branches::commits_page`). Skeleton rows until the request is the one loaded.

use crate::page::{Facts, OverviewLook};
use crate::requests::{Acts, Requests};
use artoo_branches_api::Branches;
use artoo_hosts_api::Mr;
use artoo_review_api::Review;
use artoo_shell_api::Frame;
use artoo_ui_kit::controls::skeleton;
use artoo_ui_kit::format::plural;
use artoo_ui_kit::markdown;
use artoo_ui_kit::{gap, island, px, theme};
use gpui::{AnyElement, App, Context, Entity, Render, Window, div, prelude::*};
use std::rc::{Rc, Weak};

/// The request's Overview or Commits in one tab.
pub struct Tabs {
    requests: Entity<Requests>,
    frame: Weak<dyn Frame>,
    /// The request on screen.
    pub number: Option<u64>,
    /// Its Overview is the section on screen, else its Commits.
    pub overview: bool,
}

/// The tab's view of them, made with its parts, among its services.
pub fn view(requests: Entity<Requests>, frame: &Rc<dyn Frame>, cx: &mut App) -> Entity<Tabs> {
    let frame = Rc::downgrade(frame);
    cx.new(|_| Tabs {
        requests,
        frame,
        number: None,
        overview: false,
    })
}

impl Render for Tabs {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(frame) = self.frame.upgrade() else {
            return div().flex_1().into_any_element();
        };
        let m = self.number.and_then(|number| {
            let rq = self.requests.read(cx);
            rq.find(number)
                .filter(|_| rq.current == Some(number))
                .cloned()
        });
        let Some(m) = m else {
            return island()
                .flex_1()
                .bg(theme::editor())
                .px(px(28.))
                .py(px(26.))
                .child(skeleton("request-loading", 4))
                .into_any_element();
        };
        if self.overview {
            self.overview(&m, &frame, cx)
        } else {
            self.commits(&m, &frame, cx)
        }
    }
}

impl Tabs {
    /// Overview (`Requests::render_overview`): how far your review is, the versions since the one you marked at.
    fn overview(&self, m: &Mr, frame: &Rc<dyn Frame>, cx: &mut Context<Self>) -> AnyElement {
        let services = frame.services();
        let (review, branches) = (
            services.get::<Rc<dyn Review>>(),
            services.get::<Rc<dyn Branches>>(),
        );
        let head = git2::Oid::from_str(&m.head).ok();
        let reviewed = review
            .as_ref()
            .and_then(|rv| {
                rv.progress(cx)
                    .or_else(|| head.and_then(|h| rv.head_part(h, cx)))
            })
            .map_or_else(
                || "—".to_owned(),
                |(done, total)| format!("{done} of {}", plural(total as usize, "file")),
            );
        let tips = branches.as_ref().map_or_else(Vec::new, |b| b.versions(cx));
        let latest = tips.last().map(|(number, _)| *number);
        let heads = review
            .as_ref()
            .map_or_else(Vec::new, |rv| rv.request_heads(m.iid, cx));
        let marked = tips
            .iter()
            .filter(|(_, tip)| heads.contains(tip))
            .map(|(number, _)| *number)
            .max();
        let versions = match (marked, latest) {
            (Some(at), Some(latest)) if at < latest => format!("v{at} → v{latest}"),
            (_, Some(latest)) => format!("v{latest}"),
            _ => "—".to_owned(),
        };
        let selecting = frame.selecting(cx);
        let look = OverviewLook {
            title: markdown::body(
                &m.title,
                false,
                format!("title-{}", m.iid),
                None,
                selecting.clone(),
                cx,
            ),
            base: frame.places(cx).web.map(|w| w.base),
            selecting,
            facts: Facts {
                commits: branches.as_ref().map_or(0, |b| b.commit_count(cx)),
                reviewed,
                versions,
            },
        };
        self.requests.read(cx).render_overview(m, look, cx)
    }

    /// Commits: one line of identity, then the branch's commits, each with the version it came with.
    fn commits(&self, m: &Mr, frame: &Rc<dyn Frame>, cx: &mut Context<Self>) -> AnyElement {
        let services = frame.services();
        let title = markdown::body(
            &m.title,
            false,
            format!("line-title-{}", m.iid),
            None,
            frame.selecting(cx),
            cx,
        );
        let acts = services.get::<Acts>().unwrap_or_else(|| Rc::new(|_, _| {}));
        let line = self.requests.read(cx).request_line(m, title, acts);
        let commits = services
            .get::<Rc<dyn Branches>>()
            .map(|b| b.commits_page(true, cx));
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(gap()))
            .child(line)
            .children(commits)
            .into_any_element()
    }
}
