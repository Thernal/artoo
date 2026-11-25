//! A request's description as a place (#241): its paragraphs as Overview counts them, and the kind of place a
//! note for an agent on one is on (`requests.description`, #212) — `!398 · description · ¶1`, its digest the
//! paragraph, its page the request's. A note at Overview's end is on the request itself (`!398`).

use crate::requests::Requests;
use artoo_agents_api::places::{self, Kind};
use artoo_core::markdown;
use artoo_core::route::Place as _;
use artoo_hosts_api::Mr;
use artoo_requests_api::{DESCRIPTION_PLACE, RequestRoute};
use artoo_ui_kit::words;
use gpui::App;
use serde::{Deserialize, Serialize};
use std::rc::Rc;

/// A note's place, as its data keeps it.
#[derive(Serialize, Deserialize)]
struct On {
    number: u64,
    /// The paragraph, from 1; 0, the request as a whole.
    paragraph: u32,
    /// What the paragraph said when the note was written.
    text: String,
}

impl On {
    fn read(data: &str) -> Option<On> {
        serde_json::from_str(data).ok()
    }

    fn reads(&self) -> String {
        let number = words::number(self.number);
        match self.paragraph {
            0 => number,
            k => format!("{number} · description · ¶{k}"),
        }
    }
}

/// Paragraph `k` of `text` as it is written, counted from 1 as its top-level blocks.
fn paragraph(text: &str, k: u32) -> Option<String> {
    let (_, (first, last)) = markdown::parse_lines(text)
        .into_iter()
        .nth((k as usize).checked_sub(1)?)?;
    let lines: Vec<&str> = text.lines().collect();
    let last = (last as usize).min(lines.len());
    let own = lines.get(first as usize - 1..last)?;
    Some(own.join("\n").trim_end().to_owned())
}

/// Declares the kind, once for the app.
pub(crate) fn declare(cx: &mut App) {
    let kind = Kind {
        kind: DESCRIPTION_PLACE,
        reads: Rc::new(|data, _| On::read(data).map(|on| on.reads()).unwrap_or_default()),
        digest: Rc::new(|data, _| On::read(data).map(|on| on.text).unwrap_or_default()),
        tool: Some("get_request"),
        route: Rc::new(|data| {
            let number = On::read(data)?.number;
            Some(RequestRoute { number }.route())
        }),
    };
    // Declared already by an earlier `install`.
    let _ = places::add(cx, kind);
}

impl Requests {
    /// What the description of `m` says: its details', the list's own copy until they are in.
    pub(crate) fn description_of<'a>(&'a self, m: &'a Mr) -> &'a str {
        match self.details.as_ref() {
            Some(d) if !d.description.trim().is_empty() => &d.description,
            _ => &m.description,
        }
    }

    /// The data of a note's place on request `number`: paragraph `k` of its description, 0 the request.
    pub(crate) fn description_place(&self, number: u64, k: u32) -> Option<String> {
        let m = self.find(number)?;
        let text = match k {
            0 => m.title.clone(),
            k => paragraph(self.description_of(m), k)?,
        };
        serde_json::to_string(&On {
            number,
            paragraph: k,
            text,
        })
        .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paragraph_is_a_block_as_it_is_written() {
        let text = "Moves the screen\nto one state.\n\n### States\n\n- a\n- b\n";
        assert_eq!(
            paragraph(text, 1).as_deref(),
            Some("Moves the screen\nto one state.")
        );
        assert_eq!(paragraph(text, 2).as_deref(), Some("### States"));
        assert_eq!(paragraph(text, 3).as_deref(), Some("- a\n- b"));
        assert_eq!(paragraph(text, 4), None);
        assert_eq!(paragraph(text, 0), None);
        // As many as Overview draws.
        assert_eq!(markdown::parse(text).len(), 3);
    }

    #[test]
    fn a_note_reads_by_its_paragraph() {
        let on = |paragraph| On {
            number: 398,
            paragraph,
            text: String::new(),
        };
        assert_eq!(
            on(1).reads(),
            format!("{} · description · ¶1", words::number(398))
        );
        assert_eq!(on(0).reads(), words::number(398));
    }
}
