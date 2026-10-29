//! The hosts' contract implemented (#165 stage 3): GitLab for any remote but GitHub's. Each forwards to `mr.rs`
//! for now, which still hands a GitHub remote to `github.rs`; the next step gives each its own.

use crate::git::WebRemote;
use crate::mr;
use anyhow::Result;
use artoo_hosts_api::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

pub(crate) struct HostsImpl;

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
        mr::host(&self.remote)
    }

    fn api_host(&self) -> String {
        mr::api_host(&self.remote)
    }

    fn token_source(&self) -> Option<(String, &'static str)> {
        mr::token_source(&self.remote)
    }

    fn token(&self) -> Option<String> {
        mr::token(&self.remote)
    }

    fn token_now(&self) -> Option<String> {
        mr::token_now(&self.remote)
    }

    fn save_token(&self, token: &str) -> Result<()> {
        mr::save_token(&self.remote, token)
    }

    fn forget_token(&self) {
        mr::forget_token(&self.remote)
    }

    fn token_page(&self) -> String {
        mr::token_page(&self.remote)
    }

    fn me(&self) -> Result<String> {
        mr::me(&self.remote)
    }

    fn whois_with(&self, token: &str) -> Result<(String, String)> {
        mr::whois_with(&self.remote, token)
    }

    fn token_info(&self) -> Option<String> {
        mr::token_info(&self.remote)
    }

    fn token_check_with(&self, token: &str) -> Option<(Vec<String>, Option<String>)> {
        mr::token_check_with(&self.remote, token)
    }

    fn revoke_token(&self) -> Result<()> {
        mr::revoke_token(&self.remote)
    }

    fn available(&self) -> bool {
        mr::available(&self.remote)
    }

    fn username(&self) -> Result<String> {
        mr::username(&self.remote)
    }
}

impl Requests for GitLab {
    fn list(&self) -> Result<Vec<Mr>> {
        mr::list(&self.remote)
    }

    fn list_past(&self, state: MrState, whose: &Whose, at: Option<&str>) -> Result<PastPage> {
        mr::list_past(&self.remote, state, whose, at)
    }

    fn one(&self, iid: u64) -> Result<Mr> {
        mr::one(&self.remote, iid)
    }

    fn details(&self, iid: u64) -> Result<Details> {
        mr::details(&self.remote, iid)
    }

    fn labels(&self) -> Result<Vec<Label>> {
        mr::labels(&self.remote)
    }

    fn milestones(&self) -> Result<Vec<Milestone>> {
        mr::milestones(&self.remote)
    }

    fn members(&self) -> Result<Vec<(String, String)>> {
        mr::members(&self.remote)
    }

    fn project_template(&self) -> Result<Option<String>> {
        mr::project_template(&self.remote)
    }

    fn unreached(&self) -> Result<Vec<String>> {
        mr::unreached(&self.remote)
    }

    fn create(
        &self,
        source: &str,
        target: &str,
        title: &str,
        description: &str,
        options: &NewOptions,
    ) -> Result<Mr> {
        mr::create(&self.remote, source, target, title, description, options)
    }

    fn pipeline(&self, iid: u64) -> Result<Option<Pipeline>> {
        mr::pipeline(&self.remote, iid)
    }

    fn versions(&self, iid: u64) -> Result<Vec<Push>> {
        mr::versions(&self.remote, iid)
    }

    fn todos(&self) -> Result<Vec<Todo>> {
        mr::todos(&self.remote)
    }

    fn mark_todo_done(&self, id: u64) -> Result<()> {
        mr::mark_todo_done(&self.remote, id)
    }
}

impl Discussions for GitLab {
    fn threads(&self, iid: u64) -> Result<Vec<Thread>> {
        mr::threads(&self.remote, iid)
    }

    fn awards(&self, iid: u64, notes: &[(u64, bool)]) -> Result<HashMap<u64, Vec<Award>>> {
        mr::awards(&self.remote, iid, notes)
    }

    fn award(&self, iid: u64, note: u64, issue: bool, name: &str) -> Result<u64> {
        mr::award(&self.remote, iid, note, issue, name)
    }

    fn unaward(&self, iid: u64, note: u64, issue: bool, name: &str, award: u64) -> Result<()> {
        mr::unaward(&self.remote, iid, note, issue, name, award)
    }

    fn post_discussion(
        &self,
        iid: u64,
        head: &str,
        place: &Place<'_>,
        body: &str,
        draft: bool,
    ) -> Result<Option<u64>> {
        mr::post_discussion(&self.remote, iid, head, place, body, draft)
    }

    fn resolve(&self, iid: u64, discussion: &str, resolved: bool) -> Result<()> {
        mr::resolve(&self.remote, iid, discussion, resolved)
    }

    fn reply(&self, iid: u64, discussion: &str, body: &str) -> Result<()> {
        mr::reply(&self.remote, iid, discussion, body)
    }

    fn edit_note(&self, iid: u64, discussion: &str, note: u64, body: &str) -> Result<()> {
        mr::edit_note(&self.remote, iid, discussion, note, body)
    }

    fn delete_note(&self, iid: u64, discussion: &str, note: u64) -> Result<()> {
        mr::delete_note(&self.remote, iid, discussion, note)
    }

    fn drafts(&self, iid: u64) -> Result<Vec<Draft>> {
        mr::drafts(&self.remote, iid)
    }

    fn publish_drafts(&self, iid: u64) -> Result<()> {
        mr::publish_drafts(&self.remote, iid)
    }

    fn publish_draft(&self, iid: u64, id: u64) -> Result<()> {
        mr::publish_draft(&self.remote, iid, id)
    }

    fn delete_draft(&self, iid: u64, id: u64) -> Result<()> {
        mr::delete_draft(&self.remote, iid, id)
    }

    fn upload(&self, path: &str, out: &Path) -> bool {
        mr::upload(&self.remote, path, out)
    }
}

impl Reviews for GitLab {
    fn approvals(&self, iid: u64) -> Result<Approvals> {
        mr::approvals(&self.remote, iid)
    }

    fn approve(&self, iid: u64, head: &str) -> Result<()> {
        mr::approve(&self.remote, iid, head)
    }

    fn unapprove(&self, iid: u64) -> Result<()> {
        mr::unapprove(&self.remote, iid)
    }

    fn comment(&self, iid: u64, body: &str) -> Result<()> {
        mr::comment(&self.remote, iid, body)
    }

    fn request_changes(&self, iid: u64) -> Result<()> {
        mr::request_changes(&self.remote, iid)
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
        mr::submit(&self.remote, iid, head, drafts, summary, verdict, approved)
    }
}

impl Merging for GitLab {
    fn merge_state(&self, iid: u64) -> Result<MergeState> {
        mr::merge_state(&self.remote, iid)
    }

    fn merge(&self, iid: u64, m: &Merge) -> Result<()> {
        mr::merge(&self.remote, iid, m)
    }

    fn cancel_auto_merge(&self, iid: u64, node: &str) -> Result<()> {
        mr::cancel_auto_merge(&self.remote, iid, node)
    }

    fn rebase(&self, iid: u64) -> Result<()> {
        mr::rebase(&self.remote, iid)
    }
}

impl Account for GitHub {
    fn host(&self) -> String {
        mr::host(&self.remote)
    }

    fn api_host(&self) -> String {
        mr::api_host(&self.remote)
    }

    fn token_source(&self) -> Option<(String, &'static str)> {
        mr::token_source(&self.remote)
    }

    fn token(&self) -> Option<String> {
        mr::token(&self.remote)
    }

    fn token_now(&self) -> Option<String> {
        mr::token_now(&self.remote)
    }

    fn save_token(&self, token: &str) -> Result<()> {
        mr::save_token(&self.remote, token)
    }

    fn forget_token(&self) {
        mr::forget_token(&self.remote)
    }

    fn token_page(&self) -> String {
        mr::token_page(&self.remote)
    }

    fn me(&self) -> Result<String> {
        mr::me(&self.remote)
    }

    fn whois_with(&self, token: &str) -> Result<(String, String)> {
        mr::whois_with(&self.remote, token)
    }

    fn token_info(&self) -> Option<String> {
        mr::token_info(&self.remote)
    }

    fn token_check_with(&self, token: &str) -> Option<(Vec<String>, Option<String>)> {
        mr::token_check_with(&self.remote, token)
    }

    fn revoke_token(&self) -> Result<()> {
        mr::revoke_token(&self.remote)
    }

    fn available(&self) -> bool {
        mr::available(&self.remote)
    }

    fn username(&self) -> Result<String> {
        mr::username(&self.remote)
    }
}

impl Requests for GitHub {
    fn list(&self) -> Result<Vec<Mr>> {
        mr::list(&self.remote)
    }

    fn list_past(&self, state: MrState, whose: &Whose, at: Option<&str>) -> Result<PastPage> {
        mr::list_past(&self.remote, state, whose, at)
    }

    fn one(&self, iid: u64) -> Result<Mr> {
        mr::one(&self.remote, iid)
    }

    fn details(&self, iid: u64) -> Result<Details> {
        mr::details(&self.remote, iid)
    }

    fn labels(&self) -> Result<Vec<Label>> {
        mr::labels(&self.remote)
    }

    fn milestones(&self) -> Result<Vec<Milestone>> {
        mr::milestones(&self.remote)
    }

    fn members(&self) -> Result<Vec<(String, String)>> {
        mr::members(&self.remote)
    }

    fn project_template(&self) -> Result<Option<String>> {
        mr::project_template(&self.remote)
    }

    fn unreached(&self) -> Result<Vec<String>> {
        mr::unreached(&self.remote)
    }

    fn create(
        &self,
        source: &str,
        target: &str,
        title: &str,
        description: &str,
        options: &NewOptions,
    ) -> Result<Mr> {
        mr::create(&self.remote, source, target, title, description, options)
    }

    fn pipeline(&self, iid: u64) -> Result<Option<Pipeline>> {
        mr::pipeline(&self.remote, iid)
    }

    fn versions(&self, iid: u64) -> Result<Vec<Push>> {
        mr::versions(&self.remote, iid)
    }

    fn todos(&self) -> Result<Vec<Todo>> {
        mr::todos(&self.remote)
    }

    fn mark_todo_done(&self, id: u64) -> Result<()> {
        mr::mark_todo_done(&self.remote, id)
    }
}

impl Discussions for GitHub {
    fn threads(&self, iid: u64) -> Result<Vec<Thread>> {
        mr::threads(&self.remote, iid)
    }

    fn awards(&self, iid: u64, notes: &[(u64, bool)]) -> Result<HashMap<u64, Vec<Award>>> {
        mr::awards(&self.remote, iid, notes)
    }

    fn award(&self, iid: u64, note: u64, issue: bool, name: &str) -> Result<u64> {
        mr::award(&self.remote, iid, note, issue, name)
    }

    fn unaward(&self, iid: u64, note: u64, issue: bool, name: &str, award: u64) -> Result<()> {
        mr::unaward(&self.remote, iid, note, issue, name, award)
    }

    fn post_discussion(
        &self,
        iid: u64,
        head: &str,
        place: &Place<'_>,
        body: &str,
        draft: bool,
    ) -> Result<Option<u64>> {
        mr::post_discussion(&self.remote, iid, head, place, body, draft)
    }

    fn resolve(&self, iid: u64, discussion: &str, resolved: bool) -> Result<()> {
        mr::resolve(&self.remote, iid, discussion, resolved)
    }

    fn reply(&self, iid: u64, discussion: &str, body: &str) -> Result<()> {
        mr::reply(&self.remote, iid, discussion, body)
    }

    fn edit_note(&self, iid: u64, discussion: &str, note: u64, body: &str) -> Result<()> {
        mr::edit_note(&self.remote, iid, discussion, note, body)
    }

    fn delete_note(&self, iid: u64, discussion: &str, note: u64) -> Result<()> {
        mr::delete_note(&self.remote, iid, discussion, note)
    }

    fn drafts(&self, iid: u64) -> Result<Vec<Draft>> {
        mr::drafts(&self.remote, iid)
    }

    fn publish_drafts(&self, iid: u64) -> Result<()> {
        mr::publish_drafts(&self.remote, iid)
    }

    fn publish_draft(&self, iid: u64, id: u64) -> Result<()> {
        mr::publish_draft(&self.remote, iid, id)
    }

    fn delete_draft(&self, iid: u64, id: u64) -> Result<()> {
        mr::delete_draft(&self.remote, iid, id)
    }

    fn upload(&self, path: &str, out: &Path) -> bool {
        mr::upload(&self.remote, path, out)
    }
}

impl Reviews for GitHub {
    fn approvals(&self, iid: u64) -> Result<Approvals> {
        mr::approvals(&self.remote, iid)
    }

    fn approve(&self, iid: u64, head: &str) -> Result<()> {
        mr::approve(&self.remote, iid, head)
    }

    fn unapprove(&self, iid: u64) -> Result<()> {
        mr::unapprove(&self.remote, iid)
    }

    fn comment(&self, iid: u64, body: &str) -> Result<()> {
        mr::comment(&self.remote, iid, body)
    }

    fn request_changes(&self, iid: u64) -> Result<()> {
        mr::request_changes(&self.remote, iid)
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
        mr::submit(&self.remote, iid, head, drafts, summary, verdict, approved)
    }
}

impl Merging for GitHub {
    fn merge_state(&self, iid: u64) -> Result<MergeState> {
        mr::merge_state(&self.remote, iid)
    }

    fn merge(&self, iid: u64, m: &Merge) -> Result<()> {
        mr::merge(&self.remote, iid, m)
    }

    fn cancel_auto_merge(&self, iid: u64, node: &str) -> Result<()> {
        mr::cancel_auto_merge(&self.remote, iid, node)
    }

    fn rebase(&self, iid: u64) -> Result<()> {
        mr::rebase(&self.remote, iid)
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
        mr::TEST_FIXTURE.with(|f| *f.borrow_mut() = Some(dir.path().to_owned()));
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
        mr::TEST_FIXTURE.with(|f| *f.borrow_mut() = None);
    }
}
