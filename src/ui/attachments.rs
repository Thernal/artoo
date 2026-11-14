//! A file attached to the comment being written (#214): a screenshot pasted with ⌘V, a file dropped on the composer,
//! one picked with its clip (⇧⌘A). The kit's composer draws its tile and keeps the Markdown; here it goes where the
//! comment does. A merge request's host uploads it (GitLab's project uploads); a note for the agent keeps it on this
//! Mac, under Artoo's data folder, and gives the agent its path. A GitHub request offers no clip.

use super::Workspace;
use super::composer::{Change, Compose, Target};
use gpui::{Context, PathPromptOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

/// How often a running upload's progress is read back into its tile.
const PROGRESS_EVERY: Duration = Duration::from_millis(150);

/// Where its comment keeps an attachment: on this Mac, beside the agent's notes, or with the host.
fn local(target: Target) -> bool {
    matches!(
        target,
        Target::Agent | Target::ThreadNote(_) | Target::Answer(_) | Target::Edit(Change::Agent(_))
    )
}

/// The folder pasted screenshots and a note's attachments are kept in.
pub(super) fn folder() -> PathBuf {
    artoo_core::platform::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("attachments")
}

/// The Markdown that shows `path` named `name`: an image's `![…](…)`, any other file a link.
fn markdown(name: &str, path: &str, image: bool) -> String {
    let name = name.replace(['[', ']'], "");
    let path = if path.contains([' ', '(', ')']) {
        format!("<{path}>")
    } else {
        path.to_owned()
    };
    if image {
        format!("![{name}]({path})")
    } else {
        format!("[{name}]({path})")
    }
}

/// A note's file kept on this Mac: copied under `folder` unless it is there already (a pasted screenshot).
fn keep(path: &Path, folder: &Path) -> std::io::Result<PathBuf> {
    if path.starts_with(folder) {
        return Ok(path.to_owned());
    }
    std::fs::create_dir_all(folder)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_owned());
    let kept = folder.join(format!("{now}-{name}"));
    std::fs::copy(path, &kept)?;
    Ok(kept)
}

impl Workspace {
    /// Whether `compose` takes files: a note for the agent always; on a request, when its host uploads them.
    pub(super) fn compose_clip(&self, compose: &Compose) -> bool {
        match compose.target {
            Target::AnswerValue(_) => false,
            target if local(target) => true,
            _ => self.host.as_ref().is_some_and(|h| h.uploads()),
        }
    }

    /// The comment being written waits for an upload: nothing posts it until it is done (#214).
    pub(super) fn compose_waits(&self) -> bool {
        self.compose.as_ref().is_some_and(|c| c.waiting())
    }

    /// ⌘V with a screenshot or copied files on the clipboard, in a composer that takes them: attached instead of
    /// pasted. Whether it was.
    pub(super) fn paste_attachment(
        &mut self,
        key: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.compose.as_ref().is_some_and(|c| self.compose_clip(c)) {
            return false;
        }
        let files = artoo_ui_kit::composer::pasted_files(key, &folder(), cx);
        if files.is_empty() {
            return false;
        }
        self.attach_files(files, cx);
        true
    }

    /// ⇧⌘A or the clip: the system's picker, what it picks attached.
    pub(super) fn pick_attachment(&mut self, cx: &mut Context<Self>) {
        if !self.compose.as_ref().is_some_and(|c| self.compose_clip(c)) {
            return;
        }
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            this.update(cx, |this, cx| this.attach_files(paths, cx))
                .ok();
        })
        .detach();
    }

    /// `paths` attached to the comment being written, each uploaded where it goes.
    pub(super) fn attach_files(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if !self.compose.as_ref().is_some_and(|c| self.compose_clip(c)) {
            return;
        }
        for path in paths.into_iter().filter(|p| p.is_file()) {
            let Some(compose) = self.compose.as_mut() else {
                return;
            };
            let id = compose.attach(path.clone());
            self.upload_attachment(id, path, cx);
        }
        self.diff_list(cx).remeasure();
        cx.notify();
    }

    /// `Try again` on a failed tile.
    pub(super) fn retry_attachment(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(path) = self.compose.as_mut().and_then(|c| c.retry(id)) {
            self.upload_attachment(id, path, cx);
            cx.notify();
        }
    }

    /// ✕ on a tile: it goes, its Markdown out of the text.
    pub(super) fn unattach(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(compose) = self.compose.as_mut() {
            compose.unattach(id);
            self.diff_list(cx).remeasure();
            cx.notify();
        }
    }

    /// Attachment `id` (file `path`) taken where its comment goes: kept for a note, uploaded to the host for a
    /// request, its progress read back into its tile while it goes.
    fn upload_attachment(&mut self, id: u64, path: PathBuf, cx: &mut Context<Self>) {
        let Some(compose) = self.compose.as_ref() else {
            return;
        };
        let name = (path.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let image = (compose.attachments.iter())
            .find(|a| a.id == id)
            .is_some_and(|a| a.image());
        if local(compose.target) {
            let kept = keep(&path, &folder())
                .map(|kept| markdown(&name, &kept.to_string_lossy(), image))
                .map_err(|e| e.to_string());
            return self.attachment_done(id, &path, kept, cx);
        }
        let Some(host) = self.host.clone() else {
            return self.attachment_done(id, &path, Err("No host".to_owned()), cx);
        };
        let progress = Arc::new(AtomicU32::new(0));
        let done = Arc::new(AtomicBool::new(false));
        let upload = cx.background_executor().spawn({
            let (path, progress) = (path.clone(), progress.clone());
            async move {
                host.upload(&path, &move |p: f32| {
                    progress.store(p.to_bits(), Ordering::Relaxed)
                })
            }
        });
        // The tile's bar, while it goes.
        cx.spawn({
            let (path, progress, done) = (path.clone(), progress.clone(), done.clone());
            async move |this, cx| {
                loop {
                    cx.background_executor().timer(PROGRESS_EVERY).await;
                    if done.load(Ordering::Relaxed) {
                        return;
                    }
                    let p = f32::from_bits(progress.load(Ordering::Relaxed));
                    let alive = this.update(cx, |this, cx| {
                        if let Some(c) = (this.compose.as_mut())
                            .filter(|c| c.attachments.iter().any(|a| a.id == id && a.path == path))
                        {
                            c.upload_progress(id, p);
                            cx.notify();
                        }
                    });
                    if alive.is_err() {
                        return;
                    }
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let result = upload.await.map_err(|e| format!("{e:#}"));
            done.store(true, Ordering::Relaxed);
            this.update(cx, |this, cx| this.attachment_done(id, &path, result, cx))
                .ok();
        })
        .detach();
    }

    /// Attachment `id` uploaded (its Markdown put in the text) or failed, if it is still the comment's.
    fn attachment_done(
        &mut self,
        id: u64,
        path: &Path,
        result: Result<String, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(compose) = (self.compose.as_mut())
            .filter(|c| c.attachments.iter().any(|a| a.id == id && a.path == path))
        else {
            return;
        };
        match result {
            Ok(markdown) => compose.uploaded(id, markdown),
            Err(why) => compose.upload_failed(id, short(&why)),
        }
        self.diff_list(cx).remeasure();
        cx.notify();
    }
}

/// A failure as its tile says it: its first line, cut short.
fn short(why: &str) -> String {
    let line = why.lines().next().unwrap_or("Upload failed").trim();
    match line.char_indices().nth(60) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_attachment_is_written_as_the_markdown_that_shows_it() {
        assert_eq!(markdown("a.png", "/x/a.png", true), "![a.png](/x/a.png)");
        assert_eq!(
            markdown("log.txt", "/x/log.txt", false),
            "[log.txt](/x/log.txt)"
        );
        assert_eq!(
            markdown("a [1].png", "/x y/a.png", true),
            "![a 1.png](</x y/a.png>)"
        );
    }

    #[test]
    fn a_note_keeps_its_file_under_the_folder_once() {
        let dir = std::env::temp_dir().join(format!("artoo-attach-{}", std::process::id()));
        let folder = dir.join("attachments");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("shot.png");
        std::fs::write(&file, b"png").unwrap();
        let kept = keep(&file, &folder).unwrap();
        assert!(kept.starts_with(&folder) && kept.to_string_lossy().ends_with("-shot.png"));
        assert_eq!(keep(&kept, &folder).unwrap(), kept);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_failure_reads_short() {
        assert_eq!(short("HTTP 413\nbody"), "HTTP 413");
        assert_eq!(short(&"x".repeat(80)).chars().count(), 61);
    }
}
