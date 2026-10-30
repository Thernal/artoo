//! The agents' contract implemented (#165 stage 4): the reports chats drafted, as the agents' folder keeps them.

use crate::agents::Bus;
use artoo_agents_api::{ReportDraft, ReportDrafts};

pub(crate) struct ReportDraftsImpl;

impl ReportDrafts for ReportDraftsImpl {
    fn take(&self) -> Vec<ReportDraft> {
        let Some(bus) = Bus::new() else {
            return Vec::new();
        };
        bus.take_reports()
            .into_iter()
            .map(|d| ReportDraft {
                title: d.title,
                description: d.description,
                by: d.by,
            })
            .collect()
    }
}
