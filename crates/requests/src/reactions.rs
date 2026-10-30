//! The reactions on a request's notes, by note id (#9).

use artoo_hosts_api::Award;
use std::collections::HashMap;

/// Keeps the map type named once.
pub type Awards = HashMap<u64, Vec<Award>>;
