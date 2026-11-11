//! The window's frame, the tab's side: the root view — its actions and keys, what it draws between the title bar and
//! the overlays —, zen's line and the splitters' widths. The body's row, zen's strips and the handles are drawn by the
//! shell (`artoo_shell_api::frame`).

use super::*;
use artoo_shell_api::frame;
use typing::Input;

impl Workspace {
    /// Zen's line at the foot of the diff (`artoo_shell_api::frame::zen_keys`).
    pub(super) fn render_zen_keys(&self, cx: &App) -> Option<impl IntoElement + use<>> {
        (self.zen.is_some() || self.motion.recent("zen")).then_some(())?;
        Some(self.zen_strip(
            "zen-keys",
            self.zen.is_some(),
            40.,
            frame::zen_keys(self.zen_one, cx),
            cx,
        ))
    }

    /// A strip zen takes away or brings (`artoo_shell_api::frame::zen_strip`).
    pub(super) fn zen_strip(
        &self,
        id: &'static str,
        shown: bool,
        room: f32,
        strip: impl IntoElement,
        cx: &App,
    ) -> gpui::AnyElement {
        frame::zen_strip(
            id,
            shown,
            self.motion.recent("zen"),
            room,
            strip.into_any_element(),
            cx,
        )
    }

    pub(super) fn size(&mut self, split: Split) -> &mut f32 {
        match split {
            Split::Menu => &mut self.layout.menu,
            Split::Sidebar => &mut self.layout.sidebar,
            Split::Tool => &mut self.layout.tool_width,
            Split::List => &mut self.layout.list,
        }
    }

    pub(super) fn start_drag(
        &mut self,
        split: Split,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if event.click_count == 2 {
            *self.size(split) = match split {
                Split::Menu => Layout::default().menu,
                Split::Sidebar => Layout::default().sidebar,
                Split::Tool => Layout::default().tool_width,
                Split::List => Layout::default().list,
            };
            self.save_layout();
            cx.notify();
            return;
        }
        let origin = split.axis(event.position);
        let size = *self.size(split);
        self.drag = Some(Drag {
            split,
            origin,
            size,
        });
    }

    pub(super) fn drag_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(drag) = self.drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            // The button was released outside the window.
            self.end_drag(cx);
            return;
        }
        *self.size(drag.split) = drag.size_at(event.position, super::ui_zoom());
        cx.notify();
    }

    pub(super) fn end_drag(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() {
            self.save_layout();
            cx.notify();
        }
    }

    /// The gap after a pane, which resizes it (`artoo_shell_api::frame::handle`).
    pub(super) fn handle(&self, split: Split, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let active = self.drag.is_some_and(|d| d.split == split);
        let tab = cx.weak_entity();
        frame::handle(
            split,
            active,
            Rc::new(move |event, _, cx| {
                tab.update(cx, |this, cx| this.start_drag(split, event, cx))
                    .ok();
            }),
            cx,
        )
    }

    // ---- rendering -----------------------------------------------------------------------

    /// The title bar is the window's, never the page's (#70); drawn by the shell
    /// (`artoo_shell::title`).
    pub(super) fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // In Tabs mode the tabs take the project's place (D21).
        let tabs = self.render_title_tabs(cx);
        let project = tabs.is_none().then(|| {
            self.root
                .as_deref()
                .map_or_else(|| "Artoo".to_owned(), crate::labels::project_name)
        });
        let mut items: Vec<gpui::AnyElement> = self
            .title_items(cx)
            .into_iter()
            .map(IntoElement::into_any_element)
            .collect();
        items.extend(self.render_fetch(cx).map(IntoElement::into_any_element));
        items.extend(self.render_status(cx).map(IntoElement::into_any_element));
        title::title_bar(
            title::TitleBar {
                open: self.root.is_some(),
                project,
                tabs,
                folded: self.menu_folded(),
                back: self.nav.can_back(),
                forward: self.nav.can_forward(),
                compact: self.compact(),
                items,
            },
            Rc::new(TitleGlue(cx.weak_entity())),
            cx,
        )
    }
}

/// What a click on the title bar does, answered by the tab.
struct TitleGlue(gpui::WeakEntity<Workspace>);

impl TitleGlue {
    fn tab(&self, cx: &mut App, f: impl FnOnce(&mut Workspace, &mut Context<Workspace>)) {
        self.0.update(cx, f).ok();
    }
}

impl title::TitleHost for TitleGlue {
    fn fold(&self, _: &mut Window, cx: &mut App) {
        self.tab(cx, |this, cx| this.toggle_menu(cx));
    }

    fn back(&self, _: &mut Window, cx: &mut App) {
        self.tab(cx, |this, cx| this.go_back(cx));
    }

    fn forward(&self, _: &mut Window, cx: &mut App) {
        self.tab(cx, |this, cx| this.go_forward(cx));
    }

    fn project_menu(&self, at: Point<gpui::Pixels>, _: &mut Window, cx: &mut App) {
        self.tab(cx, |this, cx| {
            let tabs = this.tabs.borrow().clone();
            if let Some((ix, tab)) = tabs.iter().enumerate().find(|(_, t)| t.active) {
                this.project_menu(ix, tabs.len(), tab, at, cx)
            }
        });
    }

    fn projects(&self, _: &mut Window, cx: &mut App) {
        self.tab(cx, |this, cx| this.search(palette_lists::PROJECTS, cx));
    }

    fn search(&self, _: &mut Window, cx: &mut App) {
        self.tab(cx, |this, cx| this.search(artoo_shell_api::search::ALL, cx));
    }

    fn settings(&self, _: &mut Window, cx: &mut App) {
        self.tab(cx, |this, cx| this.open_settings(cx));
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        debug_assert_eq!(
            self.rv().app_settings(cx),
            self.settings,
            "a change of the settings did not reach the review's copy (save_settings)"
        );
        debug_assert_eq!(
            self.review_root(cx),
            self.root,
            "the review's root is not the tab's"
        );
        // The host's words (pull requests on GitHub) for everything drawn from here.
        words::set(self.web.as_ref().is_some_and(|w| w.github));
        markdown::set_me(self.request_login(cx).as_deref());
        // What the Discussions window's icon and title ask as they are drawn (`DiscussionsHost::showing`).
        self.discussions_showing.set(self.showing_request(cx));
        self.take_error(cx);
        self.poll_parts(cx);
        // Settings closed (esc, Done, a place): what a part's rows began ends (a theme edited: its preview).
        if !self.settings_open {
            self.settings_closed(cx);
        }
        // Before anything is drawn: which input has the keyboard now (#168).
        self.sync_keyboard(cx);
        let title = match &self.root {
            Some(root) => format!(
                "Artoo — {} · {}",
                crate::labels::project_name(root),
                root.display()
            ),
            None => "Artoo".to_owned(),
        };
        if title != self.title {
            window.set_window_title(&title);
            self.title = title;
        }
        // The widths the layout rules read: the window's, and last frame's width of the diff.
        if !self.zoom_applied {
            self.zoom_applied = true;
            apply_ui_zoom(self.settings.ui_zoom, window);
        }
        let win: f32 = window.viewport_size().width.into();
        if (win - self.win_width).abs() > 0.5 {
            self.win_width = win;
            self.follow_width();
        }
        // A folded section has no keyboard: it goes to the diff.
        let folded_zone = match self.zone {
            zones::Zone::Commits => {
                self.branches().is_some_and(|b| b.commits_folded(cx)) || !self.side_open()
            }
            zones::Zone::Files => !self.files_zone(cx),
            _ => false,
        };
        if folded_zone && self.shown_diff(cx).is_some() {
            self.zone = zones::Zone::Diff;
        }
        let body = self.rv().body_width(cx);
        // The diff's width alone decides Split, whatever the window (#186).
        let narrow = body > 0. && body < f32::from(px(artoo_review_api::SPLIT_WIDTH));
        // At the interface size chosen: 600 px of the design is more at 150 %.
        let tight = body > 0. && body < f32::from(px(600.));
        if narrow != self.diff_narrow(cx) || tight != self.tight(cx) {
            self.rv().set_room(narrow, tight, cx);
            self.relayout_all(cx);
        }
        self.follow_scroll(cx);
        if self.char_width(cx) == 0. || self.char_face != theme::code_font() {
            self.char_face = theme::code_font();
            let text = window.text_system();
            let id = text.resolve_font(&font(self.char_face.clone()));
            let width = text
                .advance(id, px(self.rv().code_size(cx)), 'm')
                .map_or(7.2, |size| f32::from(size.width));
            self.rv().set_char_width(width, cx);
        }

        let mut root = div().id("workspace");
        // The commands features offer, as this tab runs them (`artoo_shell_api::Parts::actions`).
        for (act, run) in self.part_actions() {
            root = root.on_boxed_action(act.as_ref(), move |_, window, cx| run(window, cx));
        }
        root.key_context(
            // Recording a key in Settings → Keyboard: no binding may take it first.
            if self.recording.is_some() {
                "Recording"
            } else {
                match self.typing(cx) {
                    None => "Workspace",
                    Some(Input::Palette | Input::FindFiles | Input::Beside) => {
                        "Workspace Typing Palette"
                    }
                    // ↑ ↓ belong to the field: they move its caret or its line. The new
                    // version's card scrolls with them (#135).
                    Some(Input::Compose | Input::Submit | Input::Tool | Input::Overlay) => {
                        "Workspace Typing Caret"
                    }
                    Some(Input::Part(id)) if self.part_input(id).is_some_and(|i| (i.caret)(cx)) => {
                        "Workspace Typing Caret"
                    }
                    Some(_) => "Workspace Typing",
                }
            },
        )
        .track_focus(&self.focus)
        .on_action(cx.listener(Self::open))
        .on_action(cx.listener(Self::refresh))
        .on_action(cx.listener(Self::previous_commit))
        .on_action(cx.listener(Self::next_commit))
        .on_action(cx.listener(Self::previous_file))
        .on_action(cx.listener(Self::next_file))
        .on_action(cx.listener(Self::previous_step))
        .on_action(cx.listener(Self::next_step))
        .on_action(cx.listener(Self::toggle_unified))
        .on_action(cx.listener(Self::cycle_mode))
        .on_action(cx.listener(|this, _: &ToggleRendered, _, cx| this.toggle_page_source(cx)))
        .on_action(cx.listener(Self::set_lines))
        .on_action(cx.listener(Self::set_words))
        .on_action(cx.listener(Self::set_structural))
        .on_action(cx.listener(Self::open_settings_action))
        .on_action(cx.listener(Self::find_action))
        .on_action(cx.listener(Self::find_commits_action))
        .on_action(cx.listener(Self::filter_files_action))
        .on_action(cx.listener(Self::go_back_action))
        .on_action(cx.listener(Self::go_forward_action))
        .on_action(cx.listener(Self::toggle_sidebar))
        .on_action(cx.listener(Self::toggle_files))
        .on_action(cx.listener(Self::toggle_requests))
        .on_action(cx.listener(Self::focus_diff))
        .on_action(cx.listener(Self::zen_one_file))
        .on_action(cx.listener(Self::set_up))
        .on_action(cx.listener(Self::merge_request))
        .on_action(cx.listener(Self::copy_selection))
        .on_action(cx.listener(Self::select_all))
        .on_action(cx.listener(Self::next_change))
        .on_action(cx.listener(Self::previous_change))
        .on_action(cx.listener(Self::next_match))
        .on_action(cx.listener(Self::previous_match))
        .on_action(cx.listener(|this, _: &CommentLine, _, cx| this.open_goline(cx)))
        .on_action(cx.listener(|this, _: &SubmitReview, _, cx| this.open_submit(cx)))
        .on_action(cx.listener(|this, _: &ApproveRequest, _, cx| this.toggle_approval(cx)))
        .on_action(cx.listener(|this, _: &ToggleSince, _, cx| this.toggle_since(cx)))
        .on_action(cx.listener(|this, _: &SwitchTo, _, cx| {
            if this.root.is_some() {
                this.search(palette_lists::SWITCH, cx)
            }
        }))
        .on_action(cx.listener(|this, _: &NextDraft, _, cx| this.step_draft(true, cx)))
        .on_action(cx.listener(|this, _: &PreviousDraft, _, cx| this.step_draft(false, cx)))
        .on_action(cx.listener(|this, _: &CreateRequest, _, cx| this.open_create(None, cx)))
        // Every draft to its route; with no chat listening, the card asks (#77).
        .on_action(cx.listener(|this, _: &SendToAgent, _, cx| {
            let ids = this.agent_drafts(cx);
            if ids.is_empty() {
                return this.open_agent_pick(cx);
            }
            this.send_agent(ids, cx)
        }))
        .on_action(cx.listener(|this, _: &OpenIn, _, cx| this.open_in(cx)))
        .on_action(cx.listener(|this, _: &RevealInFinder, _, cx| this.reveal_target(cx)))
        .on_action(cx.listener(|this, _: &ShowShortcuts, _, cx| this.toggle_shortcuts(cx)))
        .on_action(cx.listener(|this, _: &NameProject, _, cx| {
            if let Some(root) = this.root.clone() {
                this.open_label(labeling::Target::Project(root), cx)
            }
        }))
        .on_action(cx.listener(|this, _: &NextThread, _, cx| {
            // ⇧N: a note on the agent's form in view, else the next discussion (#10 C).
            match this.acting_agent_form(cx) {
                Some(id) => this.start_answer(id, cx),
                None => this.step_thread(true, cx),
            }
        }))
        .on_action(cx.listener(|this, _: &PreviousThread, _, cx| this.step_thread(false, cx)))
        .on_action(cx.listener(|this, _: &ReplyThread, _, cx| this.reply_focused(cx)))
        .on_action(cx.listener(|this, _: &ResolveThread, _, cx| this.resolve_focused(cx)))
        .on_action(cx.listener(|this, _: &EditComment, _, cx| {
            // In the Inbox, e is done with the selected notice (an agent's: later, or resolved).
            if this.inbox && this.zone != zones::Zone::Tool {
                this.inbox_done(cx)
            } else {
                this.edit_focused(cx)
            }
        }))
        .on_action(cx.listener(|this, _: &ToggleThread, _, cx| this.toggle_focused(cx)))
        .on_action(cx.listener(|this, _: &ReactNote, _, cx| this.react_focused(cx)))
        .on_action(cx.listener(|this, _: &MarkReviewed, _, cx| this.mark_and_advance(cx)))
        .on_action(cx.listener(|this, _: &ToggleBookmark, _, cx| this.toggle_bookmark(cx)))
        .on_action(cx.listener(|this, _: &ShowBookmarks, _, cx| {
            if this.palette_open(cx) {
                this.close_palette(cx);
            } else if this.shown_diff(cx).is_some() {
                this.search(palette_lists::BOOKMARKS, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &ZoomIn, window, cx| this.ui_zoom_by(1, window, cx)))
        .on_action(cx.listener(|this, _: &ZoomOut, window, cx| this.ui_zoom_by(-1, window, cx)))
        .on_action(cx.listener(|this, _: &ZoomReset, window, cx| this.set_ui_zoom(100, window, cx)))
        .on_action(cx.listener(|this, _: &ShowWhole, window, cx| this.show_whole(window, cx)))
        .on_action(cx.listener(|this, _: &ShowStructure, _, cx| {
            if this.palette_open(cx) {
                this.close_palette(cx);
            } else if this.shown_diff(cx).is_some() {
                this.search(palette_lists::STRUCTURE, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &OpenRecent, _, cx| {
            if this.palette_open(cx) {
                this.close_palette(cx);
            } else {
                this.search(palette_lists::RECENT, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &ToggleAnnotate, _, cx| this.toggle_annotate(cx)))
        .on_action(cx.listener(|this, _: &FindInFiles, _, cx| this.open_find_files(cx)))
        .on_action(cx.listener(|this, _: &ToggleReviewed, _, cx| this.toggle_reviewed(cx)))
        .on_action(cx.listener(|this, _: &ShowOverview, _, cx| {
            this.show_section(nav::Section::Overview, cx)
        }))
        .on_action(cx.listener(|this, _: &NextSection, _, cx| this.step_section(true, cx)))
        .on_action(cx.listener(|this, _: &PreviousSection, _, cx| this.step_section(false, cx)))
        .on_action(cx.listener(|this, _: &NextOpenItem, _, cx| this.step_open_item(true, cx)))
        .on_action(cx.listener(|this, _: &PreviousOpenItem, _, cx| this.step_open_item(false, cx)))
        .on_action(cx.listener(|this, _: &FoldMenu, _, cx| this.toggle_menu(cx)))
        .on_action(cx.listener(|this, _: &NextZone, _, cx| this.step_zone(true, cx)))
        .on_action(cx.listener(|this, _: &PreviousZone, _, cx| this.step_zone(false, cx)))
        .on_action(cx.listener(|this, _: &Activate, _, cx| this.activate(cx)))
        .on_action(cx.listener(|this, _: &PageDown, _, cx| this.page(true, cx)))
        .on_action(cx.listener(|this, _: &PageUp, _, cx| this.page(false, cx)))
        .on_action(cx.listener(|this, _: &DiffStart, _, cx| this.diff_edge(false, cx)))
        .on_action(cx.listener(|this, _: &DiffEnd, _, cx| this.diff_edge(true, cx)))
        // ⌃1, the branch quick switch: search everywhere on its Branches (#72).
        .on_action(cx.listener(|this, _: &SwitchBranch, _, cx| {
            if this.root.is_some() {
                this.search(artoo_branches_api::SEARCH, cx)
            }
        }))
        // ⌃2 ⌃3: a section of the side panel; narrow, its drawer opens there (#80).
        .on_action(
            cx.listener(|this, _: &FocusCommits, _, cx| this.focus_panel(zones::Zone::Commits, cx)),
        )
        .on_action(
            cx.listener(|this, _: &FocusFiles, _, cx| this.focus_panel(zones::Zone::Files, cx)),
        )
        .on_action(
            cx.listener(|this, _: &FocusDiffZone, _, cx| this.set_zone(zones::Zone::Diff, cx)),
        )
        .on_action(cx.listener(|this, _: &NewTab, _, cx| this.search(palette_lists::PROJECTS, cx)))
        .on_action(cx.listener(|this, _: &CloseItem, _, cx| this.close_shown(cx)))
        .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
            this.with_shell(window, cx, |s, window, cx| {
                let active = s.active_ix();
                s.close(active, window, cx)
            })
        }))
        .on_action(cx.listener(|this, _: &ReopenTab, window, cx| {
            this.with_shell(window, cx, |s, window, cx| s.reopen(window, cx))
        }))
        .on_action(cx.listener(|this, _: &NextTab, window, cx| {
            this.with_shell(window, cx, |s, window, cx| s.step(true, window, cx))
        }))
        .on_action(cx.listener(|this, _: &PreviousTab, window, cx| {
            this.with_shell(window, cx, |s, window, cx| s.step(false, window, cx))
        }))
        .on_action(cx.listener(|this, _: &Tab1, window, cx| {
            this.with_shell(window, cx, |s, window, cx| s.select(0, window, cx))
        }))
        .on_action(cx.listener(|this, _: &Tab2, window, cx| {
            this.with_shell(window, cx, |s, window, cx| s.select(1, window, cx))
        }))
        .on_action(cx.listener(|this, _: &Tab3, window, cx| {
            this.with_shell(window, cx, |s, window, cx| s.select(2, window, cx))
        }))
        .on_action(cx.listener(|this, _: &Tab4, window, cx| {
            this.with_shell(window, cx, |s, window, cx| s.select(3, window, cx))
        }))
        .on_action(cx.listener(|this, _: &Tab5, window, cx| {
            this.with_shell(window, cx, |s, window, cx| s.select(4, window, cx))
        }))
        .on_action(cx.listener(|this, _: &Tab6, window, cx| {
            this.with_shell(window, cx, |s, window, cx| s.select(5, window, cx))
        }))
        .on_action(cx.listener(|this, _: &Tab7, window, cx| {
            this.with_shell(window, cx, |s, window, cx| s.select(6, window, cx))
        }))
        .on_action(cx.listener(|this, _: &Tab8, window, cx| {
            this.with_shell(window, cx, |s, window, cx| s.select(7, window, cx))
        }))
        .on_action(cx.listener(|this, _: &Tab9, window, cx| {
            this.with_shell(window, cx, |s, window, cx| s.select(8, window, cx))
        }))
        .on_action(cx.listener(Self::open_palette_action))
        .on_action(
            cx.listener(|this, _: &SearchFiles, _, cx| this.search(artoo_code_api::SEARCH, cx)),
        )
        .on_action(cx.listener(|this, _: &SearchCommits, _, cx| {
            this.search(artoo_branches_api::SEARCH_COMMITS, cx)
        }))
        .on_action(cx.listener(|this, _: &SearchBranches, _, cx| {
            this.search(artoo_branches_api::SEARCH, cx)
        }))
        .on_action(cx.listener(|this, _: &SearchRequests, _, cx| {
            this.search(artoo_requests_api::SEARCH, cx)
        }))
        .on_action(cx.listener(|this, _: &SearchActions, _, cx| {
            this.search(artoo_shell_api::search::ACTIONS, cx)
        }))
        .on_action(cx.listener(Self::compare_action))
        .on_action(cx.listener(Self::copy_link_action))
        .on_action(cx.listener(Self::open_toast_action))
        .on_action(cx.listener(Self::pin_tab_action))
        .on_action(cx.listener(Self::palette_up))
        .on_action(cx.listener(Self::palette_down))
        .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
            if this.recording.is_some() {
                return this.record_key(event, cx);
            }
            this.find_key(event, window, cx)
        }))
        .on_action(cx.listener(Self::toggle_wrap))
        .on_action(cx.listener(Self::toggle_full_context))
        .on_action(cx.listener(Self::toggle_whitespace))
        .on_action(cx.listener(Self::toggle_one_file))
        .on_action(cx.listener(Self::toggle_expand_all_action))
        .on_action(cx.listener(Self::toggle_rich_comments_action))
        .on_action(cx.listener(|this, _: &LeaveRequest, _, cx| this.leave_request(cx)))
        .on_action(cx.listener(|this, _: &ToggleAllReviewed, _, cx| this.toggle_all_reviewed(cx)))
        .on_action(cx.listener(|this, _: &NextRequest, _, cx| this.step_request(true, cx)))
        .on_action(cx.listener(|this, _: &PreviousRequest, _, cx| this.step_request(false, cx)))
        .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, _, cx| {
            if let Some(path) = paths.paths().first() {
                this.request_open(path.clone(), None, cx);
            }
        }))
        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
            this.mention_pointer(event.position, cx);
            this.drag_move(event, cx)
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.rv().set_selecting(false, cx);
                this.end_drag(cx)
            }),
        )
        .when_some(self.drag, |s, drag| s.cursor(drag.cursor()))
        .size_full()
        .relative()
        .flex()
        .flex_col()
        .bg(theme::base())
        .text_color(theme::text())
        .font_family(theme::ui_font())
        .text_size(px(13.))
        .child(self.render_title_bar(cx))
        .children(self.render_toasts(cx))
        .child(match self.root {
            _ if self.setup_open(cx) => self.render_setup(cx),
            _ if self.settings_open => self.render_settings(cx).into_any_element(),
            None => self.render_welcome(cx).into_any_element(),
            Some(_) => frame::body(
                frame::Body {
                    rail: self
                        .render_project_rail(cx)
                        .map(IntoElement::into_any_element),
                    spaces: self.render_spaces(cx).map(IntoElement::into_any_element),
                    page: self.render_space(cx).into_any_element(),
                    stripe_gap: self.stripe_gap().map(IntoElement::into_any_element),
                    stripe: self.render_stripe(cx).map(IntoElement::into_any_element),
                    zen: self.zen.is_some(),
                    zen_moving: self.motion.recent("zen"),
                },
                cx,
            ),
        })
        // Over everything, the toasts too: a toast's `⋯` opens it (#198).
        .children(
            self.ctx_menu
                .read(cx)
                .shows()
                .then(|| gpui::deferred(self.ctx_menu.clone()).with_priority(3)),
        )
        .children(self.render_mention_card(cx))
        .children(self.render_whole(cx))
        .children(self.rv().render_blame_tip(cx))
        .children(self.render_find_files(cx))
        .children(self.render_open_ask(cx))
        .children(self.render_beside_backdrop(cx))
        .children(self.render_label_card(cx))
        .children(self.overlay_view())
        .children(self.render_merge_card(cx))
        .children(self.render_shortcuts(cx))
        .children(self.render_palette(cx))
    }
}
