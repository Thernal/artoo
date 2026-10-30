//! What became of the line an outdated discussion was written on, version by version (#46), and the comparison
//! opened from one to step through them.

/// One later version that changed the line.
#[derive(Clone, Debug)]
pub struct Step {
    pub version: usize,
    pub who: String,
    pub summary: String,
    pub sha: String,
    /// What the line says in that version; None when it was removed.
    pub now: Option<String>,
    /// Where the line is in that version.
    pub line: Option<u32>,
}

/// What became of the line a discussion was written on.
#[derive(Clone, Debug)]
pub struct LineHistory {
    /// The version it was written on.
    pub written: usize,
    /// The line as it read then.
    pub then: String,
    pub steps: Vec<Step>,
    /// Where the line is in the latest version; None when it is gone.
    pub line_now: Option<u32>,
}

impl LineHistory {
    pub fn changed(&self) -> bool {
        !self.steps.is_empty()
    }

    /// The versions to step through: the one it was written on (at `line` then), each that
    /// changed the line, and the latest (`latest`) when the line did not change in it.
    pub fn stops(&self, line: Option<u32>, latest: usize) -> Vec<(usize, Option<u32>)> {
        let mut stops = vec![(self.written, line)];
        stops.extend(self.steps.iter().map(|s| (s.version, s.line)));
        if stops.last().is_some_and(|(v, _)| *v < latest) {
            stops.push((latest, self.line_now));
        }
        stops
    }
}

/// A comparison opened from an outdated discussion: the versions that changed its line, to step
/// through (#46 C, its stepper v3 → v5 → v7).
#[derive(Clone, Debug)]
pub struct Since {
    pub iid: u64,
    /// The discussion it was opened from.
    pub thread: String,
    pub path: String,
    /// Version numbers, oldest first, with where the line is in each (`None`: gone).
    pub stops: Vec<(usize, Option<u32>)>,
}
