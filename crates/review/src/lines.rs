//! The open file's lines as other features ask for them: its path, the lines selected, a line with GitLab's
//! counters and its pair on the other side, the line the `c` prompt aims at, the folded run a line is in, the row
//! that shows a line.

use crate::review::Review;
use artoo_core::rows::Row;

impl Review {
    pub fn current_path(&self) -> Option<String> {
        self.diff
            .as_ref()
            .and_then(|d| d.files.get(self.file))
            .map(|f| f.path().to_owned())
    }

    /// The lines selected in the diff, first and last, on their side; a selection that ends at the
    /// start of a line does not take that line.
    pub fn selected_lines(&self) -> Option<(bool, u32, u32)> {
        let sel = self.sel.filter(|s| !s.is_empty())?;
        let (start, end) = sel.ends();
        let last = if end.byte == 0 && end.line > start.line {
            end.line - 1
        } else {
            end.line
        };
        Some((sel.old, start.line, last))
    }

    /// A line of the open file with both of GitLab's counters: from its hunk, or, outside the
    /// hunks (all lines shown), an unchanged line.
    pub fn diff_line(&self, line: u32, old: bool) -> Option<artoo_core::git::DiffLine> {
        let file = self.diff.as_ref()?.files.get(self.file)?;
        file.position(old, line)
            .or_else(|| match self.line_pair(line, old) {
                (Some(o), Some(n)) => Some(artoo_core::git::DiffLine {
                    kind: artoo_core::git::LineKind::Context,
                    old_line: Some(o),
                    new_line: Some(n),
                }),
                _ => None,
            })
    }

    /// The old and the new line numbers GitLab wants for a line: both for an unchanged line, one
    /// for an added or removed line.
    pub fn line_pair(&self, line: u32, old: bool) -> (Option<u32>, Option<u32>) {
        use artoo_core::git::LineKind;
        let single = if old {
            (Some(line), None)
        } else {
            (None, Some(line))
        };
        let Some(at) = self.row_of_line(line, old) else {
            return single;
        };
        match self.rows[at] {
            Row::Split {
                left: Some(l),
                right: Some(r),
            } if l.kind == LineKind::Context => (Some(l.line), Some(r.line)),
            Row::Unified {
                cell,
                old_line: Some(o),
                new_line: Some(n),
                ..
            } if cell.kind == LineKind::Context => (Some(o), Some(n)),
            _ => single,
        }
    }

    /// The side and line the typed number stands for; with nothing typed, the first change.
    pub fn goline_target(&self) -> Option<(bool, u32)> {
        let text = self.goline.as_deref()?;
        if text.trim().is_empty() {
            let first = self
                .changes
                .first()
                .and_then(|&row| self.rows.get(row))
                .copied();
            return match first {
                Some(Row::Split { right: Some(c), .. }) => Some((false, c.line)),
                Some(Row::Split { left: Some(c), .. }) => Some((true, c.line)),
                Some(Row::Unified { cell, old, .. }) => Some((old, cell.line)),
                _ => None,
            };
        }
        let n = text.trim_start_matches('-').parse::<u32>().ok()?;
        Some((text.starts_with('-'), n))
    }

    /// The row of the folded run of unchanged lines that holds `line`.
    pub fn gap_of(&self, line: u32, old: bool) -> Option<usize> {
        let data = self.data.as_ref()?;
        self.rows.iter().position(|r| match r {
            Row::Gap { segment, .. } => matches!(
                data.segments.get(*segment),
                Some(artoo_core::rows::Segment::Same { old: o, new: n, len })
                    if (if old { *o } else { *n }) <= line && line < (if old { *o } else { *n }) + len
            ),
            _ => false,
        })
    }

    /// The list row that shows `line` of the old or the new side.
    pub fn row_of_line(&self, line: u32, old: bool) -> Option<usize> {
        self.rows.iter().position(|r| match r {
            Row::Split { left, right } => {
                let cell = if old { left } else { right };
                cell.is_some_and(|c| c.line == line)
            }
            Row::Unified {
                old_line, new_line, ..
            } => {
                if old {
                    *old_line == Some(line)
                } else {
                    *new_line == Some(line)
                }
            }
            _ => false,
        })
    }
}
