//! A window's repository tabs, the tab's side: the tabs are the shell's (`artoo_shell::tabs`); a tab is a
//! workspace, and the window says to agents that it is there and what it has open (#153).

use super::px;
use super::{ROW_RADIUS, Workspace, theme};
use artoo_shell::tabs::{Tab, Tabs};
pub(super) use artoo_shell::tabs::{TabInfo, TabModel};
use gpui::{
    App, AppContext, ClickEvent, Context, FocusHandle, MouseButton, WeakEntity, Window, div,
    prelude::*,
};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

/// A window: its tabs.
pub type Shell = Tabs<Workspace>;

impl Tab for Workspace {
    fn make(
        path: Option<PathBuf>,
        first: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Workspace::new(path, first, window, cx)
    }

    fn join(&mut self, tabs: WeakEntity<Shell>, model: TabModel) -> Rc<Cell<bool>> {
        self.shell = Some(tabs);
        self.tabs = model;
        self.on_screen.clone()
    }

    fn root(&self) -> Option<PathBuf> {
        self.root.clone()
    }

    fn project(&self) -> Option<PathBuf> {
        self.project.clone()
    }

    fn info(&self, cx: &App) -> (String, bool, usize) {
        (self.tab_title(), self.has_news(cx), self.inbox_unread(cx))
    }

    fn focus(&self) -> FocusHandle {
        self.focus.clone()
    }
}

/// A window's tabs (`primary`: the first window, which keeps the session and opens what agents ask for).
pub(super) fn new(
    path: Option<PathBuf>,
    primary: bool,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> Shell {
    let shell = Tabs::new(path, primary, window, cx);
    // Agents read whether this window is there and what it has open; the first window opens
    // what they ask for (#153).
    let beat = Rc::new(RefCell::new(Beat::default()));
    cx.spawn_in(window, async move |this, cx| {
        loop {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            let beat = beat.clone();
            if this
                .update_in(cx, |this, window, cx| {
                    beat.borrow_mut().beat(this, window, cx)
                })
                .is_err()
            {
                return;
            }
        }
    })
    .detach();
    let key = cx.entity_id().as_u64();
    cx.on_release(move |_, _| {
        if let Some(bus) = crate::agents::Bus::new() {
            bus.absent(std::process::id(), key);
        }
        super::system_notes::set_front(key, false);
    })
    .detach();
    // While a window is in front, news is not sent to the system's notifications (#26).
    super::system_notes::set_front(key, window.is_window_active());
    cx.observe_window_activation(window, move |_, window, _| {
        super::system_notes::set_front(key, window.is_window_active());
    })
    .detach();
    // The first start: Set up over the welcome (#143); a release that adds a part, that part.
    let parts = if primary {
        super::setup::due()
    } else {
        Vec::new()
    };
    if !parts.is_empty() {
        shell.active().update(cx, |w, cx| w.open_setup(parts, cx));
    }
    shell
}

/// What a window last said to agents.
#[derive(Default)]
struct Beat {
    /// The projects it said it has open, and when.
    said: Vec<PathBuf>,
    said_at: i64,
    /// Projects opened for an agent whose tabs are still loading (a tab has its root once loaded),
    /// with when: not opened twice meanwhile.
    opening: Vec<(PathBuf, i64)>,
}

impl Beat {
    /// Every 2 s: the window says it is there and what it has open (#153); the first window also
    /// opens a project an agent asked for, or acted on, that no window has, as a tab behind the one
    /// on screen — its workspace then takes the agent's event.
    fn beat(&mut self, shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
        let Some(bus) = crate::agents::Bus::new() else {
            return;
        };
        let mut projects = shell.projects(cx);
        let now = crate::agents::now();
        // Said again when it changes, and every 10 s so it stays fresh.
        if projects != self.said || now - self.said_at >= 10 {
            let presence = crate::agents::Presence {
                pid: std::process::id(),
                projects: projects.clone(),
                beat: now,
            };
            if bus.present(cx.entity_id().as_u64(), &presence).is_ok() {
                self.said = projects.clone();
                self.said_at = now;
            }
        }
        if !shell.primary() {
            return;
        }
        let pid = std::process::id();
        self.opening
            .retain(|(path, at)| !projects.contains(path) && now - at < 30);
        let elsewhere: Vec<PathBuf> = bus
            .running()
            .into_iter()
            .filter(|p| p.pid == pid)
            .flat_map(|p| p.projects)
            .chain(self.opening.iter().map(|(path, _)| path.clone()))
            .collect();
        for event in bus.events() {
            let project = event.project.clone();
            if !projects.contains(&project) && !elsewhere.contains(&project) {
                if !project.is_dir() {
                    bus.take_event(&event.uid);
                    continue;
                }
                shell.open_behind(project.clone(), window, cx);
                projects.push(project.clone());
                self.opening.push((project.clone(), now));
                let said = format!(
                    "{} opened {} in a tab",
                    event.agent,
                    crate::labels::project_name(&project)
                );
                shell
                    .active()
                    .update(cx, |w, cx| w.toast_info(said.into(), cx));
            }
            if event.what == crate::agents::What::Open {
                bus.take_event(&event.uid);
            }
        }
    }
}

impl Workspace {
    pub(super) fn tab_title(&self) -> String {
        // Its own name when it has one (the developer, 2026-10-23), else its folder's.
        self.root
            .as_deref()
            .map_or_else(|| "New tab".to_owned(), crate::labels::project_name)
    }

    pub(super) fn has_news(&self, cx: &gpui::App) -> bool {
        self.history.read(cx).has_news()
    }

    /// Runs `f` on the shell this workspace is a tab of.
    pub(super) fn with_shell(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) + 'static,
    ) {
        // After the event: the shell reads its tabs' workspaces (closing one reads its path), and
        // this workspace is still being updated now — reading it then panics.
        if let Some(shell) = self.shell.as_ref().and_then(WeakEntity::upgrade) {
            window.defer(cx, move |window, cx| {
                shell.update(cx, |s, cx| f(s, window, cx));
            });
        }
    }

    /// The tabs in the title bar (Design `shell.js`, D12, X10): each project's mark, name and unread
    /// count, a dot when it has news, ✕ on the active one (on the others under the pointer), + to
    /// open another. None unless Settings → Appearance chose Tabs.
    pub(super) fn render_title_tabs(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if super::projects() != crate::storage::Projects::Tabs {
            return None;
        }
        let tabs = self.tabs.borrow().clone();
        Some(
            div()
                .flex_none()
                .min_w_0()
                .max_w(px(760.))
                .overflow_hidden()
                .flex()
                .items_center()
                .gap(px(4.))
                .children(tabs.into_iter().enumerate().map(|(ix, tab)| {
                    let mark = super::rail::initials(&tab.title);
                    let hue = super::rail::project_hue(tab.root.as_deref(), &tab.title);
                    let group: gpui::SharedString = format!("tab-{ix}").into();
                    let (menu_tab, n) = (tab.clone(), self.tabs.borrow().len());
                    div()
                        .id(("tab", ix))
                        .group(group.clone())
                        // The rail's right click: name and colour, move, show, a window, close.
                        .on_mouse_down(
                            gpui::MouseButton::Right,
                            cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                                this.project_menu(ix, n, &menu_tab, e.position, cx)
                            }),
                        )
                        .flex_shrink(1.)
                        .min_w(px(60.))
                        .max_w(px(220.))
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .pl(px(6.))
                        .pr(px(4.))
                        .rounded(px(ROW_RADIUS))
                        .cursor_pointer()
                        .when(tab.active, |s| {
                            s.bg(theme::panel())
                                .border_1()
                                .border_color(theme::island_border())
                        })
                        .when(!tab.active, |s| {
                            s.text_color(theme::muted())
                                .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                        })
                        .child(super::rail::project_mark(&mark, hue, 18.))
                        .child(super::hover::cut(("tab-title", ix), tab.title.clone()).flex_1())
                        .children((!tab.active && tab.unread > 0).then(|| {
                            div()
                                .flex_none()
                                .min_w(px(16.))
                                .h(px(16.))
                                .px(px(4.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(theme::accent())
                                .text_size(px(10.))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(theme::on_accent())
                                .child(tab.unread.to_string())
                        }))
                        .children(tab.news.then(|| {
                            div()
                                .flex_none()
                                .size(px(6.))
                                .rounded_full()
                                .bg(theme::accent())
                        }))
                        .child(
                            div()
                                .id(("tab-close", ix))
                                .flex_none()
                                .w(px(18.))
                                .h(px(18.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.))
                                .text_size(px(12.))
                                .text_color(theme::faint())
                                .when(!tab.active, |s| {
                                    s.invisible().group_hover(group, |s| s.visible())
                                })
                                .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                                .child("×")
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                    this.with_shell(window, cx, move |s, window, cx| {
                                        s.close(ix, window, cx)
                                    })
                                })),
                        )
                        // Dragged onto another tab, it takes that place, as on the rail.
                        .drag_over::<super::rail::DragProject>(|s, _, _, _| s.bg(theme::selected()))
                        .on_drag(super::rail::DragProject(ix), {
                            let title = tab.title.clone();
                            move |_, _, _, cx| {
                                cx.new(|_| artoo_ui_kit::controls::DragLabel(title.clone()))
                            }
                        })
                        .on_drop(cx.listener(
                            move |this, dragged: &super::rail::DragProject, window, cx| {
                                let from = dragged.0;
                                this.with_shell(window, cx, move |s, _, cx| {
                                    s.move_tab(from, ix, cx)
                                });
                            },
                        ))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.with_shell(window, cx, move |s, window, cx| {
                                s.select(ix, window, cx)
                            })
                        }))
                }))
                .child(
                    div()
                        .id("tab-new")
                        .flex_none()
                        .w(px(26.))
                        .h(px(26.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(ROW_RADIUS))
                        .text_color(theme::muted())
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                        .tooltip_show_delay(super::hover::LONG)
                        .tooltip(|_, cx| cx.new(|_| super::Tip("Open a repository  ⌘T")).into())
                        .child("+")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.open_palette(super::palette::Kind::Projects, cx)
                        })),
                )
                .into_any_element(),
        )
    }
}
