//! Settings → Appearance → Fonts and size's helpers (#24 A): the faces a face's menu offers, a face's name and
//! family, and the line saying where the narrow layout folds at an interface size (`narrow`: the steps grow with it).

use crate::narrow::{FLOAT, MENU, SIDE};
use artoo_ui_kit::theme;

/// Faces a face's menu offers when they are installed; Other… lists every one.
pub const UI_FACES: [&str; 5] = [
    "Helvetica Neue",
    "Avenir Next",
    "Inter",
    "Georgia",
    "Verdana",
];
pub const CODE_FACES: [&str; 5] = [
    "SF Mono",
    "Monaco",
    "JetBrains Mono",
    "Fira Code",
    "Courier New",
];

/// Where the menu folds, the side panel becomes a drawer and a tool window floats, in window
/// pixels, at interface size `ui_zoom` (%).
pub fn fold_widths(ui_zoom: u16) -> [u32; 3] {
    [MENU, SIDE, FLOAT].map(|w| (w * f32::from(ui_zoom) / 100.).round() as u32)
}

/// `1472` as `1 472`.
pub fn grouped(n: u32) -> String {
    match n {
        0..1000 => n.to_string(),
        _ => format!("{} {:03}", n / 1000, n % 1000),
    }
}

/// The line under the rows: where the layout folds at this size.
pub fn fold_line(ui_zoom: u16) -> String {
    let [menu, side, float] = fold_widths(ui_zoom).map(grouped);
    match ui_zoom {
        100 => format!(
            "At 100 % the menu folds under {menu} px, the side panel under {side}, a tool window floats under {float}; at a larger size they come sooner."
        ),
        _ => format!(
            "At {ui_zoom} % the narrow layout comes {}, as the text needs {} room: the menu folds under {menu} px (1 280 at 100 %), the side panel under {side}, a tool window floats under {float}.",
            if ui_zoom > 100 { "sooner" } else { "later" },
            if ui_zoom > 100 { "more" } else { "less" },
        ),
    }
}

/// A face's menu: its default (None), those of `common` installed, and the chosen one.
pub fn short_list(
    common: &[&str],
    installed: &[String],
    chosen: Option<&str>,
) -> Vec<Option<String>> {
    let mut faces = vec![None];
    faces.extend(
        common
            .iter()
            .filter(|f| installed.iter().any(|i| i == *f))
            .map(|f| Some((*f).to_owned())),
    );
    if let Some(chosen) = chosen
        && !faces.iter().flatten().any(|f| f == chosen)
    {
        faces.push(Some(chosen.to_owned()));
    }
    faces
}

/// Every face installed, by family name; the system's hidden ones (`.SF NS`) left out.
pub fn installed(cx: &gpui::App) -> Vec<String> {
    cx.text_system()
        .all_font_names()
        .into_iter()
        .filter(|f| !f.starts_with('.'))
        .collect()
}

/// A face's name as a list says it: the interface's default is the system's.
pub fn face_label(code: bool, face: Option<&str>) -> String {
    match (face, code) {
        (Some(face), _) => face.to_owned(),
        (None, false) => "System".to_owned(),
        (None, true) => theme::CODE_FONT.to_owned(),
    }
}

/// The family `face` is drawn in.
pub fn face_family(code: bool, face: Option<&str>) -> gpui::SharedString {
    match (face, code) {
        (Some(face), _) => face.to_owned().into(),
        (None, false) => theme::UI_FONT.into(),
        (None, true) => theme::CODE_FONT.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_folds_sooner_at_a_larger_interface_size() {
        assert_eq!(fold_widths(100), [1280, 1100, 1000]);
        assert_eq!(fold_widths(115), [1472, 1265, 1150]);
        assert!(fold_line(115).contains("under 1 472 px (1 280 at 100 %)"));
        assert!(fold_line(115).contains("the side panel under 1 265"));
        assert!(fold_line(90).contains("comes later"));
    }

    #[test]
    fn a_faces_menu_has_its_default_the_installed_ones_and_the_chosen() {
        let installed = ["Georgia".to_owned(), "Optima".to_owned()];
        assert_eq!(
            short_list(&UI_FACES, &installed, None),
            [None, Some("Georgia".to_owned())]
        );
        assert_eq!(
            short_list(&UI_FACES, &installed, Some("Optima")),
            [None, Some("Georgia".to_owned()), Some("Optima".to_owned())]
        );
        assert_eq!(short_list(&UI_FACES, &installed, Some("Georgia")).len(), 2);
    }
}
