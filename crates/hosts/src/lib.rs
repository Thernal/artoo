//! GitLab and GitHub (#165): merge requests and pull requests, the ways to connect a host, merging, people's
//! and comments' pictures. One crate while they reach into one another; stage 3's `Host` trait splits it.

pub mod avatars;
pub mod connect;
pub mod github;
pub mod ide;
pub mod merge;
pub mod mr;
pub mod pictures;

use artoo_core::{emoji, git, links, platform, search, storage};
