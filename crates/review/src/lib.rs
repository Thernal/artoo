//! The review (#165 stage 4): a diff read file by file or in one scroll. The entity (`review::Review`) with its
//! events and its host, what it keeps (the selection, the marks, blame, the stack, the pins, the diff, images,
//! pages, the file view, the Changes list's rows) and how it draws its rows, the files' headers and the file read
//! in Code. The find bar, the Changes list and the file's head line are still drawn by the app's tab.

pub mod annotate;
pub mod changes;
pub mod composer;
pub mod diff;
pub mod diff_view;
pub mod file_view;
pub mod find;
pub mod goline;
pub mod image_view;
pub mod lines;
pub mod lists;
pub mod md_page;
pub mod pins;
pub mod review;
pub mod reviewed;
pub mod select;
pub mod stack;
