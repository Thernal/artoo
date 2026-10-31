//! New merge request (`artoo_requests::create` keeps the form and draws it): the tab opens it on a
//! branch, from an agent's draft for it (#152) when there is one, gives it the keyboard, shows the
//! diff of the pair it would merge and puts the quick way over the branch view (the requests draw the
//! form in the diff's place).

use super::Workspace;
use super::typing::Input;
use crate::git::BranchRef;
use artoo_requests::create::{
    CreateKey, CreateLook, Drafted, Here, INPUT, Pair, Totals, source_of,
};
use gpui::{AnyElement, App, Context};

impl Workspace {
    /// What the form reads of the repository: its folder, its `branches` (the history's), the host.
    fn create_here<'a>(&'a self, branches: &'a [BranchRef]) -> Option<Here<'a>> {
        Some(Here {
            root: self.root.as_deref()?,
            branches,
            github: self.on_github(),
        })
    }

    /// Opens the form for branch `ix` (the open one when `None`).
    pub(super) fn open_create(&mut self, ix: Option<usize>, cx: &mut Context<Self>) {
        let (Some(_), Some(web)) = (self.root.as_ref(), self.web.clone()) else {
            return;
        };
        if !self.requests.read(cx).available {
            self.error = Some(
                super::words::say("Needs a GitLab token: Settings → Merge requests.")
                    .into_owned()
                    .into(),
            );
            return cx.notify();
        }
        let Some(branch) = ix
            .or(self.history.read(cx).branch)
            .and_then(|i| self.history.read(cx).branches.get(i))
            .cloned()
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
        self.history.update(cx, |h, _| h.find.text = None);
        self.palette = None;
        self.ctx_menu = None;
        self.bfilter.clear();
        let host = super::host_of(&web, cx);
        let branches = self.history.read(cx).branches.clone();
        let Some(here) = self.create_here(&branches) else {
            return;
        };
        let pair = self.requests.update(cx, |rq, cx| {
            rq.open_create(&here, &branch, drafted, host, cx)
        });
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
        self.requests.update(cx, |rq, _| rq.use_template(ix));
        cx.notify();
    }

    /// A snapshot's way in: `create-templates`, `create-labels`, `create-milestone`.
    #[cfg(feature = "snapshot")]
    pub(super) fn open_create_menu(&mut self, name: &str, cx: &mut Context<Self>) {
        self.requests
            .update(cx, |rq, cx| rq.open_create_menu(name, cx));
    }

    /// ⌘K → Create merge request: the quick way, starting at Into.
    pub(super) fn open_create_quick(&mut self, cx: &mut Context<Self>) {
        self.open_create(None, cx);
        self.requests.update(cx, |rq, _| rq.create_quick());
        cx.notify();
    }

    /// The form is open.
    pub(super) fn creating(&self, cx: &App) -> bool {
        self.requests.read(cx).creating.is_some()
    }

    /// From's or Into's list picked. Chosen from a list over the form: the form has the keyboard
    /// again.
    pub(super) fn pick_branch(&mut self, name: String, source: bool, cx: &mut Context<Self>) {
        self.take_keyboard(Input::Part(INPUT), cx);
        let branches = self.history.read(cx).branches.clone();
        let Some(here) = self.create_here(&branches) else {
            return;
        };
        let pair = self.requests.update(cx, |rq, _| {
            if source {
                rq.pick_source(&here, name)
            } else {
                rq.pick_target(&here, name)
            }
        });
        self.compare_pair(pair, cx);
        cx.notify();
    }

    /// ↑ ↓ on From's or Into's list (`Requests::step_branch_list`).
    pub(super) fn step_branch_list(&mut self, down: bool, cx: &mut Context<Self>) -> bool {
        let branches = self.history.read(cx).branches.clone();
        let took = self
            .requests
            .update(cx, |rq, _| rq.step_branch_list(&branches, down));
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
        let Some(remote) = self.web.clone() else {
            return;
        };
        let host = super::host_of(&remote, cx);
        if let Some(iid) = self
            .requests
            .update(cx, |rq, cx| rq.submit_create(host, cx))
        {
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

    /// "3 commits · 8 files · +46 −12": the diff of the pair.
    fn create_totals(&self, cx: &App) -> Option<Totals> {
        Totals::of(self.review.read(cx).diff.as_ref()?)
    }

    /// What the form is drawn with that the tab knows, the history's `branches` among it.
    fn create_look<'a>(&self, branches: &'a [BranchRef], cx: &Context<Self>) -> CreateLook<'a> {
        CreateLook {
            typing: self.has_keyboard(Input::Part(INPUT), cx),
            github: self.on_github(),
            base: self.web.as_ref().map(|w| w.base.clone()),
            selecting: Some(self.selecting(cx.entity().downgrade())),
            totals: self.create_totals(cx),
            branches,
        }
    }

    /// The quick way over the branch view (`Requests::render_create_quick`).
    pub(super) fn render_create_quick(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let branches = self.history.read(cx).branches.clone();
        let look = self.create_look(&branches, cx);
        let acts = self.request_acts(cx);
        self.requests
            .update(cx, |rq, cx| rq.render_create_quick(&look, acts, cx))
    }
}
