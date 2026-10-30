//! People as the kit draws them — an avatar's picture and the username its colour comes from — as a port: the
//! kit names what it needs, hosts provides it (its `install`), as they learn people from every answer. Process-wide
//! like what they learn, so an avatar is drawn without the app at hand. Without one provided, an avatar is the
//! name's initial on the name's colour.

use std::sync::{Arc, OnceLock};

/// People named by the hosts.
pub trait People: Send + Sync {
    /// The picture of `who` (a username or a name), once it is in.
    fn picture(&self, who: &str) -> Option<Arc<gpui::Image>>;
    /// The username of `who` (a name or a username), lower case, once a host named them.
    fn login(&self, who: &str) -> Option<String>;
}

static PEOPLE: OnceLock<Arc<dyn People>> = OnceLock::new();

/// Sets who draws people (hosts' `install`); the first one stays.
pub fn provide(people: Arc<dyn People>) {
    let _ = PEOPLE.set(people);
}

/// The picture of `who`, once it is in.
pub fn picture(who: &str) -> Option<Arc<gpui::Image>> {
    PEOPLE.get()?.picture(who)
}

/// The username of `who`, lower case; `who` itself, lower case, while no host named them.
pub fn login_of(who: &str) -> String {
    PEOPLE
        .get()
        .and_then(|p| p.login(who))
        .unwrap_or_else(|| who.to_lowercase())
}
