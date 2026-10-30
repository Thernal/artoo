//! A place in the window as data (#165): who draws it — its kind, a stable id such as `requests.request` — and
//! what it is, its fields as JSON. It is what `nav.txt` keeps and history compares, so neither needs the type of
//! the feature that draws it, and a kind nobody draws any more is dropped where it is read, not a crash. A kind is
//! renamed only with a migration, like an action's id.

use serde::{Serialize, de::DeserializeOwned};

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Route {
    pub kind: String,
    /// Its fields, as a JSON object.
    pub args: String,
}

impl Route {
    /// As a line keeps it: the kind, then its fields (`requests.request{"number":398}`).
    pub fn text(&self) -> String {
        format!("{}{}", self.kind, self.args)
    }

    /// `text` read back; `None` for what is not one (a kind is lowercase letters, digits, `.` and `-`).
    pub fn parse(text: &str) -> Option<Route> {
        let (kind, args) = text.split_at(text.find('{')?);
        let named = !kind.is_empty()
            && kind
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-'));
        (named && args.ends_with('}')).then(|| Route {
            kind: kind.to_owned(),
            args: args.to_owned(),
        })
    }
}

/// A route a crate knows the type of: its kind, its fields through serde.
pub trait Place: Serialize + DeserializeOwned {
    const KIND: &'static str;

    fn route(&self) -> Route {
        Route {
            kind: Self::KIND.to_owned(),
            args: serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned()),
        }
    }

    /// `route` as this type: `None` when it is another kind, or its fields do not fit.
    fn from_route(route: &Route) -> Option<Self> {
        (route.kind == Self::KIND)
            .then(|| serde_json::from_str(&route.args).ok())
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct File {
        path: String,
    }

    impl Place for File {
        const KIND: &'static str = "code.file";
    }

    #[test]
    fn a_place_survives_its_route_and_its_text() {
        let file = File {
            path: "src/a {b}.rs".to_owned(),
        };
        let text = file.route().text();
        assert_eq!(text, r#"code.file{"path":"src/a {b}.rs"}"#);
        let route = Route::parse(&text).unwrap();
        assert_eq!(File::from_route(&route), Some(file));
    }

    #[test]
    fn what_is_not_a_route_reads_as_none() {
        // nav.txt's keys before routes: a path may hold a brace.
        assert_eq!(Route::parse("file:src/{a}.rs"), None);
        assert_eq!(Route::parse("request:398"), None);
        assert_eq!(Route::parse(r#"{"path":"a"}"#), None);
        let other = Route::parse(r#"code.folder{"path":"a"}"#).unwrap();
        assert_eq!(File::from_route(&other), None);
        let broken = Route::parse(r#"code.file{"name":"a"}"#).unwrap();
        assert_eq!(File::from_route(&broken), None);
    }
}
