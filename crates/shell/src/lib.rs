//! The window's frame (#165 stage 4): what surrounds the pages features draw — the keys and their card, toasts, the
//! context menu, Settings → Appearance's drawing, the repository tabs, so far.
//! The tab (the app's) composes: it makes the features' entities and their hosts, and hands the frame what it draws
//! and what a click on it does.

pub mod appearance;
pub mod keymap;
pub mod menu;
pub mod shortcuts;
pub mod tabs;
pub mod toast;
