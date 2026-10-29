//! The hosts' contract implemented (#165 stage 3): GitLab for any remote but GitHub's. GitLab answers from
//! `gitlab.rs`, GitHub from `github.rs`; neither asks which host it is.

use crate::git::WebRemote;
use crate::{github, gitlab, tokens};
use anyhow::Result;
use anyhow::bail;
use artoo_hosts_api::*;
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) struct HostsImpl;

/// The host `remote` is on, for hosts' own modules (connect's checks).
pub(crate) fn of(remote: &WebRemote) -> Arc<dyn Host> {
    HostsImpl.of(remote)
}

impl Hosts for HostsImpl {
    fn of(&self, remote: &WebRemote) -> Arc<dyn Host> {
        let remote = remote.clone();
        if remote.github {
            Arc::new(GitHub { remote })
        } else {
            Arc::new(GitLab { remote })
        }
    }
}

struct GitLab {
    remote: WebRemote,
}

struct GitHub {
    remote: WebRemote,
}

impl Host for GitLab {
    fn remote(&self) -> &WebRemote {
        &self.remote
    }
}

impl Host for GitHub {
    fn remote(&self) -> &WebRemote {
        &self.remote
    }
}

impl Account for GitLab {
    fn host(&self) -> String {
        gitlab::host(&self.remote)
    }

    fn api_host(&self) -> String {
        gitlab::api_host(&self.remote)
    }

    fn token_source(&self) -> Option<(String, &'static str)> {
        gitlab::token_source(&self.remote)
    }

    fn token(&self) -> Option<String> {
        gitlab::token(&self.remote)
    }

    fn token_now(&self) -> Option<String> {
        gitlab::token_now(&self.remote)
    }

    fn save_token(&self, token: &str) -> Result<()> {
        gitlab::save_token(&self.remote, token)
    }

    fn forget_token(&self) {
        gitlab::forget_token(&self.remote)
    }

    fn token_page(&self) -> String {
        gitlab::token_page(&self.remote)
    }

    fn me(&self) -> Result<String> {
        gitlab::me(&self.remote)
    }

    fn whois_with(&self, token: &str) -> Result<(String, String)> {
        gitlab::whois_with(&self.remote, token)
    }

    fn token_info(&self) -> Option<String> {
        gitlab::token_info(&self.remote)
    }

    fn token_check_with(&self, token: &str) -> Option<(Vec<String>, Option<String>)> {
        gitlab::token_check_with(&self.remote, token)
    }

    fn revoke_token(&self) -> Result<()> {
        gitlab::revoke_token(&self.remote)
    }

    fn available(&self) -> bool {
        gitlab::available(&self.remote)
    }

    fn username(&self) -> Result<String> {
        gitlab::username(&self.remote)
    }
}

impl Requests for GitLab {
    fn list(&self) -> Result<Vec<Mr>> {
        gitlab::list(&self.remote)
    }

    fn list_past(&self, state: MrState, whose: &Whose, at: Option<&str>) -> Result<PastPage> {
        gitlab::list_past(&self.remote, state, whose, at)
    }

    fn one(&self, iid: u64) -> Result<Mr> {
        gitlab::one(&self.remote, iid)
    }

    fn details(&self, iid: u64) -> Result<Details> {
        gitlab::details(&self.remote, iid)
    }

    fn labels(&self) -> Result<Vec<Label>> {
        gitlab::labels(&self.remote)
    }

    fn milestones(&self) -> Result<Vec<Milestone>> {
        gitlab::milestones(&self.remote)
    }

    fn members(&self) -> Result<Vec<(String, String)>> {
        gitlab::members(&self.remote)
    }

    fn project_template(&self) -> Result<Option<String>> {
        gitlab::project_template(&self.remote)
    }

    fn unreached(&self) -> Result<Vec<String>> {
        gitlab::unreached(&self.remote)
    }

    fn create(
        &self,
        source: &str,
        target: &str,
        title: &str,
        description: &str,
        options: &NewOptions,
    ) -> Result<Mr> {
        gitlab::create(&self.remote, source, target, title, description, options)
    }

    fn pipeline(&self, iid: u64) -> Result<Option<Pipeline>> {
        gitlab::pipeline(&self.remote, iid)
    }

    fn versions(&self, iid: u64) -> Result<Vec<Push>> {
        gitlab::versions(&self.remote, iid)
    }

    fn todos(&self) -> Result<Vec<Todo>> {
        gitlab::todos(&self.remote)
    }

    fn mark_todo_done(&self, id: u64) -> Result<()> {
        gitlab::mark_todo_done(&self.remote, id)
    }
}

impl Discussions for GitLab {
    fn threads(&self, iid: u64) -> Result<Vec<Thread>> {
        gitlab::threads(&self.remote, iid)
    }

    fn awards(&self, iid: u64, notes: &[(u64, bool)]) -> Result<HashMap<u64, Vec<Award>>> {
        gitlab::awards(&self.remote, iid, notes)
    }

    fn award(&self, iid: u64, note: u64, _issue: bool, name: &str) -> Result<u64> {
        gitlab::award(&self.remote, iid, note, name)
    }

    fn unaward(&self, iid: u64, note: u64, _issue: bool, name: &str, award: u64) -> Result<()> {
        gitlab::unaward(&self.remote, iid, note, name, award)
    }

    fn post_discussion(
        &self,
        iid: u64,
        head: &str,
        place: &Place<'_>,
        body: &str,
        draft: bool,
    ) -> Result<Option<u64>> {
        gitlab::post_discussion(&self.remote, iid, head, place, body, draft)
    }

    fn resolve(&self, iid: u64, discussion: &str, resolved: bool) -> Result<()> {
        gitlab::resolve(&self.remote, iid, discussion, resolved)
    }

    fn reply(&self, iid: u64, discussion: &str, body: &str) -> Result<()> {
        gitlab::reply(&self.remote, iid, discussion, body)
    }

    fn edit_note(&self, iid: u64, discussion: &str, note: u64, body: &str) -> Result<()> {
        gitlab::edit_note(&self.remote, iid, discussion, note, body)
    }

    fn delete_note(&self, iid: u64, discussion: &str, note: u64) -> Result<()> {
        gitlab::delete_note(&self.remote, iid, discussion, note)
    }

    fn drafts(&self, iid: u64) -> Result<Vec<Draft>> {
        gitlab::drafts(&self.remote, iid)
    }

    fn publish_draft(&self, iid: u64, id: u64) -> Result<()> {
        gitlab::publish_draft(&self.remote, iid, id)
    }

    fn delete_draft(&self, iid: u64, id: u64) -> Result<()> {
        gitlab::delete_draft(&self.remote, iid, id)
    }
}

impl Reviews for GitLab {
    fn approvals(&self, iid: u64) -> Result<Approvals> {
        gitlab::approvals(&self.remote, iid)
    }

    fn approve(&self, iid: u64, head: &str) -> Result<()> {
        gitlab::approve(&self.remote, iid, head)
    }

    fn unapprove(&self, iid: u64) -> Result<()> {
        gitlab::unapprove(&self.remote, iid)
    }

    fn comment(&self, iid: u64, body: &str) -> Result<()> {
        gitlab::comment(&self.remote, iid, body)
    }

    fn submit(
        &self,
        iid: u64,
        head: &str,
        drafts: bool,
        summary: &str,
        verdict: Verdict,
        approved: bool,
    ) -> Result<()> {
        gitlab::submit(&self.remote, iid, head, drafts, summary, verdict, approved)
    }
}

impl Merging for GitLab {
    fn merge_state(&self, iid: u64) -> Result<MergeState> {
        gitlab::merge_state(&self.remote, iid)
    }

    fn merge(&self, iid: u64, m: &Merge) -> Result<()> {
        gitlab::merge(&self.remote, iid, m)
    }

    fn cancel_auto_merge(&self, iid: u64, _node: &str) -> Result<()> {
        gitlab::cancel_auto_merge(&self.remote, iid)
    }

    fn rebase(&self, iid: u64) -> Result<()> {
        gitlab::rebase(&self.remote, iid)
    }
}

impl Account for GitHub {
    fn host(&self) -> String {
        github::host(&self.remote)
    }

    fn api_host(&self) -> String {
        format!("https://{}", github::host(&self.remote))
    }

    fn token_source(&self) -> Option<(String, &'static str)> {
        github::token_source(&self.remote, false)
    }

    fn token(&self) -> Option<String> {
        github::token_source(&self.remote, true).map(|(t, _)| t)
    }

    fn token_now(&self) -> Option<String> {
        github::token_source(&self.remote, false).map(|(t, _)| t)
    }

    fn save_token(&self, token: &str) -> Result<()> {
        tokens::save(&self.api_host(), token)
    }

    fn forget_token(&self) {
        tokens::forget(&self.api_host())
    }

    fn token_page(&self) -> String {
        github::token_page(&self.remote)
    }

    fn me(&self) -> Result<String> {
        github::me(&self.remote)
    }

    fn whois_with(&self, token: &str) -> Result<(String, String)> {
        github::whois_with(&self.remote, token)
    }

    fn token_info(&self) -> Option<String> {
        None
    }

    fn token_check_with(&self, _token: &str) -> Option<(Vec<String>, Option<String>)> {
        None
    }

    fn revoke_token(&self) -> Result<()> {
        Ok(())
    }

    fn available(&self) -> bool {
        crate::fixture::fixture().is_some() || self.token_now().is_some()
    }

    fn username(&self) -> Result<String> {
        github::username(&self.remote)
    }
}

impl Requests for GitHub {
    fn list(&self) -> Result<Vec<Mr>> {
        github::list(&self.remote)
    }

    fn list_past(&self, state: MrState, whose: &Whose, at: Option<&str>) -> Result<PastPage> {
        github::list_past(&self.remote, state, whose, at)
    }

    fn one(&self, iid: u64) -> Result<Mr> {
        github::one(&self.remote, iid)
    }

    fn details(&self, iid: u64) -> Result<Details> {
        github::details(&self.remote, iid)
    }

    fn labels(&self) -> Result<Vec<Label>> {
        github::labels(&self.remote)
    }

    fn milestones(&self) -> Result<Vec<Milestone>> {
        github::milestones(&self.remote)
    }

    fn members(&self) -> Result<Vec<(String, String)>> {
        github::members(&self.remote)
    }

    fn project_template(&self) -> Result<Option<String>> {
        Ok(None)
    }

    fn unreached(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    fn create(
        &self,
        source: &str,
        target: &str,
        title: &str,
        description: &str,
        options: &NewOptions,
    ) -> Result<Mr> {
        github::create(&self.remote, source, target, title, description, options)
    }

    fn pipeline(&self, iid: u64) -> Result<Option<Pipeline>> {
        github::pipeline(&self.remote, iid)
    }

    fn versions(&self, iid: u64) -> Result<Vec<Push>> {
        github::versions(&self.remote, iid)
    }

    fn todos(&self) -> Result<Vec<Todo>> {
        github::todos(&self.remote)
    }

    fn mark_todo_done(&self, id: u64) -> Result<()> {
        github::mark_todo_done(&self.remote, id)
    }
}

impl Discussions for GitHub {
    fn threads(&self, iid: u64) -> Result<Vec<Thread>> {
        github::threads(&self.remote, iid)
    }

    fn awards(&self, iid: u64, notes: &[(u64, bool)]) -> Result<HashMap<u64, Vec<Award>>> {
        github::awards(&self.remote, iid, notes)
    }

    fn award(&self, iid: u64, note: u64, issue: bool, name: &str) -> Result<u64> {
        github::react(&self.remote, iid, note, issue, name)
    }

    fn unaward(&self, iid: u64, note: u64, issue: bool, name: &str, award: u64) -> Result<()> {
        github::unreact(&self.remote, iid, note, issue, name, award)
    }

    fn post_discussion(
        &self,
        iid: u64,
        head: &str,
        place: &Place<'_>,
        body: &str,
        draft: bool,
    ) -> Result<Option<u64>> {
        github::post_discussion(&self.remote, iid, head, place, body, draft)
    }

    fn resolve(&self, iid: u64, discussion: &str, resolved: bool) -> Result<()> {
        github::resolve(&self.remote, iid, discussion, resolved)
    }

    fn reply(&self, iid: u64, discussion: &str, body: &str) -> Result<()> {
        github::reply(&self.remote, iid, discussion, body)
    }

    fn edit_note(&self, iid: u64, discussion: &str, note: u64, body: &str) -> Result<()> {
        github::edit_note(&self.remote, iid, discussion, note, body)
    }

    fn delete_note(&self, iid: u64, discussion: &str, note: u64) -> Result<()> {
        github::delete_note(&self.remote, iid, discussion, note)
    }

    fn drafts(&self, iid: u64) -> Result<Vec<Draft>> {
        github::drafts(&self.remote, iid)
    }

    fn publish_draft(&self, _iid: u64, _id: u64) -> Result<()> {
        bail!("GitHub sends a pending comment only with its review")
    }

    fn delete_draft(&self, iid: u64, id: u64) -> Result<()> {
        github::delete_draft(&self.remote, iid, id)
    }
}

impl Reviews for GitHub {
    fn approvals(&self, iid: u64) -> Result<Approvals> {
        github::approvals(&self.remote, iid)
    }

    fn approve(&self, iid: u64, head: &str) -> Result<()> {
        github::submit(&self.remote, iid, head, "", Verdict::Approve)
    }

    fn unapprove(&self, _iid: u64) -> Result<()> {
        bail!(
            "GitHub cannot take an approval back — a later review (comment or changes) replaces it"
        )
    }

    fn comment(&self, iid: u64, body: &str) -> Result<()> {
        github::comment(&self.remote, iid, body)
    }

    fn submit(
        &self,
        iid: u64,
        head: &str,
        _drafts: bool,
        summary: &str,
        verdict: Verdict,
        _approved: bool,
    ) -> Result<()> {
        github::submit(&self.remote, iid, head, summary, verdict)
    }
}

impl Merging for GitHub {
    fn merge_state(&self, iid: u64) -> Result<MergeState> {
        github::merge_state(&self.remote, iid)
    }

    fn merge(&self, iid: u64, m: &Merge) -> Result<()> {
        github::merge(&self.remote, iid, m)
    }

    fn cancel_auto_merge(&self, iid: u64, node: &str) -> Result<()> {
        github::cancel_auto_merge(&self.remote, iid, node)
    }

    fn rebase(&self, _iid: u64) -> Result<()> {
        bail!("GitHub cannot rebase a pull request from here: update the branch on GitHub")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remote_finds_its_host_and_reads_through_the_contract() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("mrs.json"),
            r#"[{"iid": 412, "title": "Fix the login", "author": {"name": "Ann"}}]"#,
        )
        .unwrap();
        std::fs::create_dir(dir.path().join("github")).unwrap();
        std::fs::write(
            dir.path().join("github/pulls.json"),
            r#"[{"number": 7, "title": "Tidy the docs", "user": {"login": "bo"},
                "head": {"ref": "docs", "sha": "abc"}, "base": {"ref": "main"}}]"#,
        )
        .unwrap();
        crate::fixture::TEST_FIXTURE.with(|f| *f.borrow_mut() = Some(dir.path().to_owned()));
        let gitlab = WebRemote {
            base: "https://git.example.com/g/p".into(),
            github: false,
        };
        let github = WebRemote {
            base: "https://github.com/o/r".into(),
            github: true,
        };
        let hosts = HostsImpl;
        assert_eq!(hosts.of(&github).remote(), &github);
        // Each host answers from its own side: GitHub's pull requests, GitLab's merge requests.
        let pulls = hosts.of(&github).list().unwrap();
        assert_eq!(pulls.iter().map(|m| m.iid).collect::<Vec<_>>(), [7]);
        let host = hosts.of(&gitlab);
        assert_eq!(host.remote(), &gitlab);
        let list = host.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(
            (list[0].iid, list[0].title.as_str()),
            (412, "Fix the login")
        );
        // One side of a host alone, as a function that needs only it takes it.
        let discussions: &dyn Discussions = &*host;
        assert!(discussions.publish_draft(412, 9001).is_ok());
        assert!(hosts.of(&github).publish_draft(412, 9001).is_err());
        crate::fixture::TEST_FIXTURE.with(|f| *f.borrow_mut() = None);
    }
}
