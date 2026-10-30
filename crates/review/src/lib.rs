//! The review (#165 stage 4): a diff read file by file or in one scroll. Moving here from the app's views bottom up
//! (the task's plan, R3): the row drawing and the types it keeps first (the selection, the marks, blame, the stack, the pins, the diff, images, pages, the file view); the
//! entity, its events and its host follow.

pub mod annotate;
pub mod changes;
pub mod diff;
pub mod diff_view;
pub mod file_view;
pub mod image_view;
pub mod md_page;
pub mod pins;
pub mod reviewed;
pub mod select;
pub mod stack;
