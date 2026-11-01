//! The window's frame (#165 stage 4): what surrounds the pages features draw — the keys and their card, toasts, the
//! context menu, Settings → Appearance's drawing, the repository tabs and
//! the rail, the title bar, the stripe
//! and its tool windows, narrow windows' rules,
//! the page bar, so far.
//! The tab (the app's) composes: it makes the features' entities and their hosts, and hands the frame what it draws
//! and what a click on it does.

pub mod appearance;
pub mod bar;
pub mod fonts;
pub mod keymap;
pub mod menu;
pub mod narrow;
pub mod rail;
pub mod shortcuts;
pub mod tabs;
pub mod title;
pub mod toast;
pub mod tools;
