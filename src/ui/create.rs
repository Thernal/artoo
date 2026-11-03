//! New merge request (the requests keep the form and draw it): the tab opens it on a
//! branch, from an agent's draft for it (#152) when there is one, gives it the keyboard, shows the
//! diff of the pair it would merge and puts the quick way over the branch view (the requests draw the
//! form in the diff's place).

use super::Workspace;
use super::typing::Input;
use artoo_core::mr::source_of;
use artoo_requests_api::{CreateKey, Drafted, INPUT, Pair};
use gpui::{AnyElement, App, Context};

impl Workspace {
    /// Opens the form for branch `ix` (the open one when `None`).
    pub(super) fn open_create(&mut self, ix: Option<usize>, cx: &mut Context<Self>) {
        if self.root.is_none() || self.web.is_none() {
            return;
        }
        if !self.requests_available(cx) {
            self.error = Some(
                super::words::say("Needs a GitLab token: Settings → Merge requests.")
                    .into_owned()
                    .into(),
            );
            return cx.notify();
        }
        let Some(branch) = ix
            .or(self.branch_ix(cx))
            .and_then(|i| self.branch_list(cx).get(i).cloned())
        else {
            return;
        };
        let source = source_of(&branch);
        // An agent's draft for this branch (#152) is what the form starts from.
        self.drafts_unseen.remove(&source);
        let drafted = self
            .agent_requests
            .iter()
            .find(|(_, d)| d.source == source)
            .map(|(agent, d)| Drafted {
                agent: agent.clone(),
                target: d.target.clone(),
                title: d.title.clone(),
                description: d.description.clone(),
                labels: d.labels.clone(),
                reviewers: d.reviewers.clone(),
            });
        self.field = None;
        if let Some(branches) = self.branches() {
            branches.drop_find(cx);
        }
        self.palette = None;
        self.ctx_menu.update(cx, |m, cx| m.shut(cx));
        self.bfilter.clear();
        let pair = self
            .requests_api()
            .and_then(|r| r.open_create(&branch, drafted, cx));
        self.take_keyboard(Input::Part(INPUT), cx);
        self.compare_pair(pair, cx);
        cx.notify();
    }

    /// The diff the request would hold: under Changes, and the files on the left.
    fn compare_pair(&mut self, pair: Option<Pair>, cx: &mut Context<Self>) {
        if let Some((target, source)) = pair {
            self.run_compare(target, source, true, None, cx);
        }
    }

    /// Templates ▾ chose `ix`. Chosen from a list over the form: the form has the keyboard again.
    pub(super) fn use_template(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.take_keyboard(Input::Part(INPUT), cx);
        if let Some(requests) = self.requests_api() {
            requests.use_template(ix, cx);
        }
        cx.notify();
    }

    /// A snapshot's way in: `create-templates`, `create-labels`, `create-milestone`.
    #[cfg(feature = "snapshot")]
    pub(super) fn open_create_menu(&mut self, name: &str, cx: &mut Context<Self>) {
        if let Some(requests) = self.requests_api() {
            requests.open_create_menu(name, cx);
        }
    }

    /// ⌘K → Create merge request: the quick way, starting at Into.
    pub(super) fn open_create_quick(&mut self, cx: &mut Context<Self>) {
        self.open_create(None, cx);
        if let Some(requests) = self.requests_api() {
            requests.create_quick(cx);
        }
        cx.notify();
    }

    /// The form is open.
    pub(super) fn creating(&self, cx: &App) -> bool {
        self.requests_api().is_some_and(|r| r.creating(cx))
    }

    /// From's or Into's list picked. Chosen from a list over the form: the form has the keyboard
    /// again.
    pub(super) fn pick_branch(&mut self, name: String, source: bool, cx: &mut Context<Self>) {
        self.take_keyboard(Input::Part(INPUT), cx);
        let pair = self
            .requests_api()
            .and_then(|r| r.pick_branch(name, source, cx));
        self.compare_pair(pair, cx);
        cx.notify();
    }

    /// ↑ ↓ on From's or Into's list (`Requests::step_branch_list`).
    pub(super) fn step_branch_list(&mut self, down: bool, cx: &mut Context<Self>) -> bool {
        let took = (self.requests_api()).is_some_and(|r| r.step_branch_list(down, cx));
        if took {
            cx.notify();
        }
        took
    }

    /// What a key on the form asked (`Event::Create`): the request created, or the pair's diff.
    pub(super) fn create_asked(&mut self, asked: CreateKey, cx: &mut Context<Self>) {
        match asked {
            CreateKey::Create => self.submit_create(cx),
            CreateKey::Compare(pair) => self.compare_pair(Some(pair), cx),
        }
    }

    /// Creates the request (`Requests::submit_create`), or opens the one there is for the pair.
    pub(super) fn submit_create(&mut self, cx: &mut Context<Self>) {
        if let Some(iid) = self.requests_api().and_then(|r| r.submit_create(cx)) {
            self.open_request(iid, cx);
        }
    }

    /// The host created request `iid` from `source`: an agent's draft for the branch is done
    /// with (#152), and the request opens.
    pub(super) fn created(&mut self, iid: u64, source: &str, cx: &mut Context<Self>) {
        self.agent_requests.retain(|(_, d)| d.source != source);
        self.layout.show_sidebar = true;
        self.open_request(iid, cx);
        cx.notify();
    }

    /// The quick way over the branch view (`Requests::render_create_quick`).
    pub(super) fn render_create_quick(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let typing = self.has_keyboard(Input::Part(INPUT), cx);
        let selecting = self.selecting(cx.entity().downgrade());
        self.requests_api()?.render_create_quick(typing, selecting, cx)
    }
}
