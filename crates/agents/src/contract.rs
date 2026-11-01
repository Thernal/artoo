//! The agents' contract implemented (#165 stage 4): the reports chats drafted, and the folder the windows read, as
//! this Mac keeps them.

use crate::agents::Bus;
use artoo_agents_api::bus::{Event, Folder, Mark, Presence};
use artoo_agents_api::{ReportDraft, ReportDrafts};
use std::path::Path;
use std::time::SystemTime;

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

/// The folder as the windows and tabs ask it: this Mac's, read anew on each ask.
pub(crate) struct FolderImpl;

impl Folder for FolderImpl {
    fn present(&self, window: u64, presence: &Presence) -> bool {
        Bus::new().is_some_and(|bus| bus.present(window, presence).is_ok())
    }

    fn absent(&self, pid: u32, window: u64) {
        if let Some(bus) = Bus::new() {
            bus.absent(pid, window);
        }
    }

    fn running(&self) -> Vec<Presence> {
        Bus::new().map(|bus| bus.running()).unwrap_or_default()
    }

    fn events(&self) -> Vec<Event> {
        Bus::new().map(|bus| bus.events()).unwrap_or_default()
    }

    fn take_event(&self, uid: &str) -> bool {
        Bus::new().is_some_and(|bus| bus.take_event(uid))
    }

    fn marks(&self, project: &Path) -> Vec<Mark> {
        Bus::new().map(|bus| bus.marks(project)).unwrap_or_default()
    }

    fn marks_stamp(&self, project: &Path) -> Option<SystemTime> {
        Bus::new()?.marks_stamp(project)
    }

    fn codex_beat(&self, project: &Path) {
        if let Some(bus) = Bus::new() {
            crate::codex::beat(&bus, project);
        }
    }
}
