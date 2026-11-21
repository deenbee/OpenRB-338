//! Pattern arrangement editor: a block timeline for the per-bar pattern selections stored in
//! standard RBS device tracks. Pattern numbers and colors are derived from the pattern index and
//! never saved; edits still write the original controller-1 events, one selected pattern per bar.
//! The read-only overview shows the four device rows together.

use std::collections::HashMap;

use eframe::egui::{self, Color32, Id, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2, Window};
use orb_engine::seq::Event as TrackEvent;

use crate::app::App;
use crate::link::ROW_DEVICES;
use crate::model::{BAR, Event};

/// The open pattern automation editor window, if any.
#[derive(Clone, Debug)]
pub struct PatternAutomationEditor {
    /// The pattern section (row) 0..=3.
    pub row: usize,
    /// Human-readable title.
    pub title: String,
    /// Serial to distinguish multiple open windows.
    pub serial: u32,
    /// The first bar of the timeline's visible range.
    pub view_start: f32,
    /// The last transport bar seen, used to reveal a bar selected with the transport arrows.
    pub last_song_bar: i32,
    /// Number of bars visible across the timeline.
    pub zoom_bars: i32,
    /// Whether the pencil tool is active for adding pattern blocks.
    pub pencil: bool,
    /// Whether dragging a block resizes it and shifts later blocks.
    pub resize_mode: bool,
    resize_drag: Option<PatternResizeDrag>,
    /// The bar last selected.
    pub selected_bar: Option<i32>,
    /// The pattern (1..=32) painted when the user drags on the timeline.
    pub active_pattern: u8,
}

/// The single window that shows the four pattern sections' arrangements at once (read-only).
#[derive(Clone, Debug)]
pub struct PatternsOverview {
    pub serial: u32,
    /// Number of visible bars (4..=32).
    pub window_bars: i32,
    /// First visible bar (0-indexed) when scrolling in stop mode.
    pub scroll_bar: i32,
    /// Last transport bar seen, used to sync `scroll_bar` when the song position moves.
    pub last_song_bar: i32,
}

const MIN_ZOOM_BARS: i32 = 4;
const MAX_ZOOM_BARS: i32 = 32;
const DEFAULT_ZOOM_BARS: i32 = 16;

#[derive(Clone, Copy, Debug)]
struct PatternResizeDrag {
    start_bar: i32,
    initial_end_bar: i32,
    anchor_x: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PatternBlockAction {
    Reassign(i32, i32, u8, u8),
}

/// The device name for a pattern section row.
fn device_name(row: usize) -> &'static str {
    match row {
        0 => "TB-303 I",
        1 => "TB-303 II",
        2 => "TR-808",
        _ => "TR-909",
    }
}

fn follow_view_start(start: f32, zoom_bars: i32, playhead: f32, min: i32, max: i32) -> f32 {
    if playhead < start || playhead >= start + zoom_bars as f32 {
        playhead.floor().clamp(min as f32, max as f32)
    } else {
        start
    }
}

impl App {
    /// Clears the selected bar's pattern override, or all overrides with Ctrl/Cmd.
    pub(crate) fn reset_pattern_automation(&mut self, ui: &Ui, id: &str) {
        if !self.panel.transport.song_mode {
            self.status = Some("Pattern editor is only available in Song mode".into());
            return;
        }
        let row = id
            .strip_prefix("pattern")
            .and_then(|s| s.split('.').next())
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| n.checked_sub(1))
            .filter(|&r| r < 4)
            .unwrap_or(0);
        let changed = if ui.input(|i| i.modifiers.command || i.modifiers.ctrl) {
            let changed = self.clear_pattern_automation_all(row);
            self.status = Some(format!(
                "Pattern automation for {} cleared",
                device_name(row)
            ));
            changed
        } else {
            let bar = self.panel.transport.position / BAR + 1;
            let changed = self.set_pattern_automation_bar(row, bar, None);
            self.status = Some(format!("Bar {bar}: pattern automation cleared"));
            changed
        };
        self.doc.modified |= changed;
    }

    /// Stores pattern selection in the original song track format (controller 1).
    fn set_pattern_automation_bar(&mut self, row: usize, bar: i32, pattern: Option<u8>) -> bool {
        if !self.panel.transport.song_mode || row >= 4 || !(1..=999).contains(&bar) {
            return false;
        }
        let Some(link) = &mut self.link else {
            return false;
        };
        let track = ROW_DEVICES[row].id() as usize;
        let start = (bar - 1) * BAR;
        let end = start + BAR;
        let previous: Vec<_> = link.loaded_tracks()[track]
            .iter()
            .filter(|event| event.ctrl == 1 && (start..end).contains(&event.tick))
            .copied()
            .collect();
        let new_value = pattern.map(|value| value.saturating_sub(1).min(31));
        if previous.len() == usize::from(new_value.is_some())
            && previous
                .first()
                .is_some_and(|event| event.tick == start && Some(event.value) == new_value)
            || previous.is_empty() && new_value.is_none()
        {
            return false;
        }
        link.delete_track_events(track, start, end, Some(1));
        if let Some(value) = new_value {
            link.insert_track_events(
                track,
                vec![TrackEvent {
                    tick: start,
                    ctrl: 1,
                    value,
                }],
            );
        }
        self.panel.pattern_automation = link.pattern_automation();
        if bar == 1
            && let Some(value) = new_value
        {
            self.panel.adopt_pattern(row, value);
        }
        self.doc.modified = true;
        true
    }

    /// Assigns one pattern to an inclusive run of bars, keeping all events outside that block.
    fn set_pattern_automation_run(
        &mut self,
        row: usize,
        start_bar: i32,
        end_bar: i32,
        old_pattern: u8,
        new_pattern: u8,
    ) -> bool {
        if !self.panel.transport.song_mode
            || row >= 4
            || start_bar < 1
            || start_bar > end_bar
            || end_bar > 999
            || old_pattern == new_pattern
        {
            return false;
        }
        let Some(link) = &mut self.link else {
            return false;
        };
        let track = ROW_DEVICES[row].id() as usize;
        let start = (start_bar - 1) * BAR;
        let end = end_bar * BAR;
        link.delete_track_events(track, start, end, Some(1));
        link.insert_track_events(
            track,
            vec![TrackEvent {
                tick: start,
                ctrl: 1,
                value: new_pattern.saturating_sub(1).min(31),
            }],
        );
        self.panel.pattern_automation = link.pattern_automation();
        if start_bar == 1 {
            self.panel
                .adopt_pattern(row, new_pattern.saturating_sub(1).min(31));
        }
        self.doc.modified = true;
        true
    }

    /// Resizes a pattern run by shifting all later pattern events by the same number of bars.
    fn resize_pattern_automation_run(
        &mut self,
        row: usize,
        current_end_bar: i32,
        target_end_bar: i32,
    ) -> bool {
        if !self.panel.transport.song_mode
            || row >= 4
            || !(1..999).contains(&target_end_bar)
            || target_end_bar == current_end_bar
        {
            return false;
        }
        let Some(link) = &mut self.link else {
            return false;
        };
        let track = ROW_DEVICES[row].id() as usize;
        let shift_from = current_end_bar * BAR;
        let delta = (target_end_bar - current_end_bar) * BAR;
        let later: Vec<_> = link.loaded_tracks()[track]
            .iter()
            .filter(|event| event.ctrl == 1 && event.tick >= shift_from)
            .map(|event| TrackEvent {
                tick: event.tick + delta,
                ..*event
            })
            .collect();
        if later.is_empty() {
            return false;
        }
        link.delete_track_events(track, shift_from, i32::MAX, Some(1));
        link.insert_track_events(track, later);
        self.panel.pattern_automation = link.pattern_automation();
        self.doc.modified = true;
        true
    }

    /// Removes standard pattern-selection events from the inclusive bar range.
    fn clear_pattern_automation(&mut self, row: usize, start_bar: i32, end_bar: i32) -> bool {
        if !self.panel.transport.song_mode || row >= 4 || start_bar > end_bar {
            return false;
        }
        let Some(link) = &mut self.link else {
            return false;
        };
        let track = ROW_DEVICES[row].id() as usize;
        let start = (start_bar - 1).max(0) * BAR;
        let end = end_bar.max(start_bar) * BAR;
        let had_events = link.loaded_tracks()[track]
            .iter()
            .any(|event| event.ctrl == 1 && (start..end).contains(&event.tick));
        if !had_events {
            return false;
        }
        link.delete_track_events(track, start, end, Some(1));
        self.panel.pattern_automation = link.pattern_automation();
        self.doc.modified = true;
        true
    }

    /// Clears a row's pattern changes and returns its selected pattern to A1.
    fn clear_pattern_automation_all(&mut self, row: usize) -> bool {
        let cleared = self.clear_pattern_automation(row, 1, 999);
        let reset = row < 4 && self.panel.rows[row].pattern != 0;
        if reset {
            self.panel.rows[row].pattern = 0;
            self.panel.rows[row].bank = 0;
            self.panel.events.push(Event::Pattern { row, pattern: 0 });
        }
        self.doc.modified |= reset;
        cleared || reset
    }

    /// Opens the pattern automation editor for pattern section `row` (0..=3).
    pub fn open_pattern_automation(&mut self, id: &str) {
        if !self.panel.transport.song_mode {
            self.status = Some("Pattern editor is only available in Song mode".into());
            return;
        }
        // Extract the row number from an id like `pattern2.pattern5`.
        let row = id
            .strip_prefix("pattern")
            .and_then(|s| s.split('.').next())
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| n.checked_sub(1))
            .filter(|&r| r < 4)
            .unwrap_or(0);

        self.pattern_automation_serial[row] += 1;
        let serial = self.pattern_automation_serial[row];
        let active_pattern = self.panel.rows[row].pattern.saturating_add(1).clamp(1, 32);
        self.pattern_automation[row] = Some(PatternAutomationEditor {
            row,
            title: format!("Automation: Pattern — {}", device_name(row)),
            serial,
            view_start: ((self.panel.transport.position / BAR + 1).max(1)) as f32,
            last_song_bar: self.panel.transport.position / BAR,
            zoom_bars: DEFAULT_ZOOM_BARS,
            pencil: false,
            resize_mode: false,
            resize_drag: None,
            selected_bar: Some((self.panel.transport.position / BAR) + 1),
            active_pattern,
        });
    }

    /// Opens the read-only overview of the four pattern sections' arrangements.
    pub fn open_patterns_overview(&mut self) {
        if !self.panel.transport.song_mode {
            return;
        }
        let song_bar = (self.panel.transport.position / BAR).clamp(0, 998);
        let serial = self.patterns_overview.as_ref().map_or(1, |o| o.serial + 1);
        self.patterns_overview = Some(PatternsOverview {
            serial,
            window_bars: 16,
            scroll_bar: song_bar,
            last_song_bar: song_bar,
        });
    }

    /// Renders the pattern automation editor window if open.
    pub fn pattern_automation_ui(&mut self, ctx: &egui::Context) {
        // The ReBirth pattern mode has no song-bar counter; its bar display is intentionally
        // disabled. Close these song-arrangement editors if the mode changes while they are up.
        if !self.panel.transport.song_mode {
            self.pattern_automation = std::array::from_fn(|_| None);
            return;
        }
        for row in 0..4 {
            let Some(mut editor) = self.pattern_automation[row].take() else {
                continue;
            };
            let mut open = true;
            let win_id = Id::unique(("orb_pattern_automation", editor.row, editor.serial));

            let default_size = Vec2::new(780.0, 600.0);
            let min_size = Vec2::new(600.0, 460.0);

            Window::new(&editor.title)
                .id(win_id)
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_pos(egui::Pos2::new(
                    100.0 + row as f32 * 24.0,
                    100.0 + row as f32 * 24.0,
                ))
                .default_size(default_size)
                .min_size(min_size)
                .show(ctx, |ui| {
                    self.pattern_automation_contents(ui, &mut editor);
                });

            if open {
                self.pattern_automation[row] = Some(editor);
            }
        }
    }

    /// The contents of the pattern automation editor window.
    fn pattern_automation_contents(&mut self, ui: &mut Ui, editor: &mut PatternAutomationEditor) {
        let row = editor.row;
        let is_playing = self.panel.transport.playing;
        let playhead_tick = self.panel.transport.position;
        let transport_bar = playhead_tick / BAR;
        let looping = self.panel.transport.looping;
        let loop_start = self.panel.transport.loop_start;
        let loop_end = self.panel.transport.loop_end;

        // Determine the full bar range: loop range when looping, otherwise 1..=999.
        let (full_start, full_end) = if looping && loop_end > loop_start {
            let start = (loop_start / BAR + 1).max(1);
            let end = (loop_end / BAR).min(999);
            (start, end.max(start))
        } else {
            (1, 999)
        };

        editor.zoom_bars = editor.zoom_bars.clamp(MIN_ZOOM_BARS, MAX_ZOOM_BARS);
        ui.horizontal(|ui| {
            ui.label(format!("Bars {}..{}", full_start, full_end));
            if looping {
                ui.label("(loop range)");
            } else {
                ui.label("(all bars)");
            }
            ui.separator();
            ui.label("Song position:");
            self.transport_bar_control(ui, ("pattern_automation", editor.row, editor.serial));
        });

        ui.horizontal(|ui| {
            ui.label("Zoom");
            for bars in [4, 8, 16, 32] {
                if ui
                    .selectable_label(editor.zoom_bars == bars, format!("{bars} Bars"))
                    .clicked()
                {
                    editor.zoom_bars = bars;
                }
            }
            ui.separator();
            if ui.selectable_label(editor.pencil, "✎ Pencil").clicked() {
                editor.pencil = !editor.pencil;
                if editor.pencil {
                    editor.resize_mode = false;
                    editor.resize_drag = None;
                }
            }
            let resize_clicked = ui
                .selectable_label(editor.resize_mode, "↔ Resize")
                .on_hover_text(
                    "Activate, then drag a block left or right; following blocks move with it",
                )
                .clicked();
            if resize_clicked {
                editor.resize_mode = !editor.resize_mode;
                editor.resize_drag = None;
                if editor.resize_mode {
                    editor.pencil = false;
                }
            }
            ui.separator();
            ui.label("Pattern");
            // The active pattern's color, then the dropdown to pick which pattern to paint.
            let (swatch, _) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::hover());
            ui.painter()
                .rect_filled(swatch, 2.0, pattern_color(editor.active_pattern));
            let name = crate::behavior::pattern_label(editor.active_pattern - 1);
            egui::ComboBox::from_id_salt(("pattern_picker", editor.row, editor.serial))
                .selected_text(name)
                .show_ui(ui, |ui| {
                    for p in 1..=32u8 {
                        let label = crate::behavior::pattern_label(p - 1);
                        let text = RichText::new(label).color(pattern_color(p));
                        if ui
                            .selectable_label(editor.active_pattern == p, text)
                            .clicked()
                        {
                            editor.active_pattern = p;
                        }
                    }
                });
        });
        let mut max_start = (full_end - editor.zoom_bars + 1).max(full_start);
        editor.view_start = editor.view_start.clamp(full_start as f32, max_start as f32);
        let playhead = (playhead_tick.max(0) as f32 / BAR as f32) + 1.0;
        if is_playing {
            editor.view_start = follow_view_start(
                editor.view_start,
                editor.zoom_bars,
                playhead,
                full_start,
                max_start,
            );
        } else if transport_bar != editor.last_song_bar {
            editor.view_start = playhead.floor().clamp(full_start as f32, max_start as f32);
        }
        editor.last_song_bar = transport_bar;
        ui.label(if editor.resize_mode {
            concat!(
                "Resize active: drag a block left or right; blocks after it move with it; ",
                "minimum length is one bar."
            )
        } else if editor.pencil {
            concat!(
                "Pencil active: click or drag to insert blocks; use the arrow to reassign; ",
                "right-click to clear."
            )
        } else {
            concat!(
                "Click to select; use the arrow to reassign; activate Pencil to insert or ",
                "Resize to change block length; right-click to clear."
            )
        });
        ui.add_space(4.0);
        let timeline_height = (ui.available_height() - 80.0).max(120.0);
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), timeline_height),
            Sense::click_and_drag(),
        );
        if response.hovered() {
            let wheel = ui.input(|input| input.smooth_scroll_delta.y);
            if wheel != 0.0 {
                editor.zoom_bars = (editor.zoom_bars + if wheel > 0.0 { -1 } else { 1 })
                    .clamp(MIN_ZOOM_BARS, MAX_ZOOM_BARS);
                max_start = (full_end - editor.zoom_bars + 1).max(full_start);
                editor.view_start = editor.view_start.clamp(full_start as f32, max_start as f32);
            }
        }
        let view_end = (editor.view_start.ceil() as i32 + editor.zoom_bars - 1).min(full_end);
        let view_start = editor.view_start;
        let mut values = self.panel.pattern_automation[row].clone();
        values.entry(1).or_insert(1);
        let mut resize_consumed = false;
        if editor.resize_mode {
            let default_pat = values.get(&1).copied().unwrap_or(1);
            let runs = pattern_runs(&values, default_pat, 1.0, 999);
            if let Some(pos) = response
                .hover_pos()
                .filter(|p| block_plot(rect).contains(*p))
            {
                let bar = block_bar_at(pos, rect, view_start, editor.zoom_bars);
                if runs
                    .iter()
                    .any(|(start, end, _)| *start <= bar && bar <= *end && *end < 999)
                {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
            }
            if response.drag_started()
                && let Some(pos) = response.interact_pointer_pos()
                && block_plot(rect).contains(pos)
            {
                let bar = block_bar_at(pos, rect, view_start, editor.zoom_bars);
                if let Some((start_bar, end_bar, _)) = runs
                    .iter()
                    .copied()
                    .find(|(start, end, _)| *start <= bar && bar <= *end && *end < 999)
                {
                    editor.resize_drag = Some(PatternResizeDrag {
                        start_bar,
                        initial_end_bar: end_bar,
                        anchor_x: pos.x,
                    });
                }
            }
            if let (Some(drag), Some(pos)) = (editor.resize_drag, response.interact_pointer_pos())
                && response.dragged()
            {
                let slot = block_plot(rect).width() / editor.zoom_bars as f32;
                let delta = ((pos.x - drag.anchor_x) / slot).round() as i32;
                let target_end = (drag.initial_end_bar + delta).clamp(drag.start_bar, 998);
                let default_pat =
                    self.panel.pattern_automation[row].get(&1).copied().unwrap_or(1);
                let current_end = pattern_runs(
                    &self.panel.pattern_automation[row],
                    default_pat,
                    1.0,
                    999,
                )
                .into_iter()
                .find(|(start, _, _)| *start == drag.start_bar)
                .map(|(_, end, _)| end);
                if let Some(current_end) = current_end {
                    resize_consumed =
                        self.resize_pattern_automation_run(row, current_end, target_end);
                    editor.selected_bar = Some(drag.start_bar);
                }
            }
            if response.drag_stopped() {
                editor.resize_drag = None;
            }
            resize_consumed |= response.dragged() || response.drag_started();
        }
        let (block_menu_clicked, block_edit) = paint_pattern_blocks(
            ui,
            rect,
            &values,
            view_start,
            editor.zoom_bars,
            editor.selected_bar,
            is_playing.then_some(playhead),
            true,
            editor.resize_mode,
            (editor.serial << 2) | editor.row as u32,
        );
        let block_menu_consumed = resize_consumed || block_menu_clicked || block_edit.is_some();
        if let Some(action) = block_edit {
            match action {
                PatternBlockAction::Reassign(start_bar, end_bar, old_pattern, new_pattern) => {
                    self.set_pattern_automation_run(
                        row,
                        start_bar,
                        end_bar,
                        old_pattern,
                        new_pattern,
                    );
                    editor.active_pattern = new_pattern;
                    editor.selected_bar = Some(start_bar);
                }
            }
        }
        if !block_menu_consumed
            && !editor.resize_mode
            && let Some(pos) = response
                .hover_pos()
                .filter(|pos| block_plot(rect).contains(*pos))
        {
            if response.secondary_clicked() {
                let bar = block_bar_at(pos, rect, view_start, editor.zoom_bars);
                self.set_pattern_automation_bar(row, bar, None);
                editor.selected_bar = Some(bar);
            } else if response.clicked() && !editor.pencil {
                let bar = block_bar_at(pos, rect, view_start, editor.zoom_bars);
                let default_pattern = values.get(&1).copied().unwrap_or(1);
                editor.active_pattern = pattern_at_bar(&values, default_pattern, bar);
                editor.selected_bar = Some(bar);
            } else if editor.pencil && (response.dragged() || response.clicked()) {
                let bar = block_bar_at(pos, rect, view_start, editor.zoom_bars);
                self.set_pattern_automation_bar(row, bar, Some(editor.active_pattern));
                editor.selected_bar = Some(bar);
            }
        }

        ui.horizontal(|ui| {
            ui.label("Bars");
            let response = ui.add_enabled(
                max_start > full_start,
                egui::Slider::new(&mut editor.view_start, full_start as f32..=max_start as f32)
                    .step_by(1.0)
                    .text("Start bar"),
            );
            if response.changed() {
                let bar = (editor.view_start.round() as i32).clamp(full_start, max_start);
                editor.view_start = bar as f32;
                let position = (bar - 1) * BAR;
                self.panel.transport.position = position;
                self.panel.events.push(Event::Set {
                    id: "transport.position".into(),
                    value: position,
                });
                editor.last_song_bar = bar - 1;
                editor.selected_bar = Some(bar);
            }
            let view_end = (editor.view_start.ceil() as i32 + editor.zoom_bars - 1).min(full_end);
            ui.label(format!("{:.1}–{}", editor.view_start, view_end));
        });

        ui.add_space(8.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Clear View").clicked() {
                self.clear_pattern_automation(row, view_start.floor() as i32, view_end);
            }
            if ui.button("Clear Range").clicked() {
                self.clear_pattern_automation(row, full_start, full_end);
            }
            if ui.button("Clear All").clicked() {
                self.clear_pattern_automation_all(row);
            }
        });
    }

    /// Renders the read-only overview of the four pattern sections' arrangements.
    pub fn patterns_overview_ui(&mut self, ctx: &egui::Context) {
        if !self.panel.transport.song_mode {
            self.patterns_overview = None;
            return;
        }
        let Some(mut overview) = self.patterns_overview.clone() else {
            return;
        };
        let playing = self.panel.transport.playing;
        let playhead = self.panel.transport.position;
        let (tick_start, tick_end) = patterns_tick_range(playhead, playing, &mut overview);
        let start_bar_1 = tick_start / BAR + 1;
        let end_bar_1 = tick_end / BAR;
        let mut open = true;
        let mut edit_row = None;
        let id = egui::Id::unique(("orb_patterns_overview", overview.serial));
        egui::Window::new("All Patterns")
            .id(id)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(Vec2::new(720.0, 560.0))
            .min_size(Vec2::new(480.0, 300.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("View:").color(crate::palette::INK_DIM));
                    for bars in [4, 8, 16, 32] {
                        if ui
                            .selectable_label(overview.window_bars == bars, format!("{bars} Bars"))
                            .clicked()
                        {
                            overview.window_bars = bars;
                        }
                    }
                    ui.separator();
                    ui.label(
                        RichText::new(format!("Bars {start_bar_1}–{end_bar_1}"))
                            .color(crate::palette::AMBER),
                    );
                });
                ui.add_space(4.0);
                let start_bar = (tick_start / BAR + 1) as f32;
                let bars = (tick_end - tick_start) / BAR;
                for row in 0..4 {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(device_name(row)).strong());
                        if ui.small_button("Edit…").clicked() {
                            edit_row = Some(row);
                        }
                    });
                    let (rect, _) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 64.0), Sense::hover());
                    let mut values = self.panel.pattern_automation[row].clone();
                    values.entry(1).or_insert(1);
                    let _ = paint_pattern_blocks(
                        ui,
                        rect,
                        &values,
                        start_bar,
                        bars,
                        None,
                        playing.then_some(playhead as f32 / BAR as f32 + 1.0),
                        false,
                        false,
                        overview.serial,
                    );
                    ui.add_space(2.0);
                }
            });
        if !open {
            self.patterns_overview = None;
        } else {
            self.patterns_overview = Some(overview);
        }
        if let Some(row) = edit_row {
            self.open_pattern_automation(&format!("pattern{}.pattern1", row + 1));
        }
    }
}

/// The plot area of a block timeline: the colored blocks and the bar numbers.
fn block_plot(rect: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(rect.min.x + 8.0, rect.min.y + 8.0),
        Pos2::new(rect.max.x - 8.0, rect.max.y - 24.0),
    )
}

/// The bar (1-based) under `pos` on a block timeline.
fn block_bar_at(pos: Pos2, rect: Rect, start: f32, zoom_bars: i32) -> i32 {
    let plot = block_plot(rect);
    let fraction = ((pos.x - plot.min.x) / plot.width()).clamp(0.0, 0.999_999);
    (start + fraction * zoom_bars as f32).floor() as i32
}

/// A distinct, fixed color for each pattern 1..=32 (never saved; derived from the number).
fn pattern_color(pattern: u8) -> Color32 {
    let p = pattern.clamp(1, 32);
    let hue = (p as f32 - 1.0) / 32.0 * 360.0;
    hsv(hue, 0.62, 0.88)
}

/// A color from HSV (hue in degrees, saturation and value in 0..=1).
fn hsv(hue: f32, sat: f32, val: f32) -> Color32 {
    let c = val * sat;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = val - c;
    let (r, g, b) = match (hue / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Color32::from_rgb(
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    )
}

/// The visible `(tick_start, tick_end)` of the patterns overview, anchored to the song position
/// while playing and clamped to 4..=32 bars.
fn patterns_tick_range(
    song_pos: i32,
    playing: bool,
    overview: &mut PatternsOverview,
) -> (i32, i32) {
    overview.window_bars = overview.window_bars.clamp(MIN_ZOOM_BARS, MAX_ZOOM_BARS);
    let song_bar = (song_pos / BAR).clamp(0, 998);
    let max_start_bar = (999 - overview.window_bars).max(0);
    if playing
        && (song_bar < overview.scroll_bar
            || song_bar >= overview.scroll_bar + overview.window_bars)
    {
        overview.scroll_bar = song_bar;
        overview.last_song_bar = song_bar;
    } else if playing {
        overview.last_song_bar = song_bar;
    } else if song_bar != overview.last_song_bar {
        overview.scroll_bar = song_bar;
        overview.last_song_bar = song_bar;
    }
    overview.scroll_bar = overview.scroll_bar.clamp(0, max_start_bar);
    let tick_start = overview.scroll_bar * BAR;
    let tick_end = (overview.scroll_bar + overview.window_bars) * BAR;
    (tick_start, tick_end)
}

/// Draws one device's arrangement as colored blocks: each run of consecutive bars with the same
/// pattern is one block. `patterns` maps event bar -> pattern (1..=32); the selected pattern holds
/// from that bar until the next event. In editable mode each block has a Reason-style dropdown
/// that can reassign the whole run. Returns the first requested block edit.
#[allow(clippy::too_many_arguments)]
fn paint_pattern_blocks(
    ui: &Ui,
    rect: Rect,
    patterns: &HashMap<i32, u8>,
    start: f32,
    zoom_bars: i32,
    selected_bar: Option<i32>,
    playhead: Option<f32>,
    editable: bool,
    resize_mode: bool,
    window_serial: u32,
) -> (bool, Option<PatternBlockAction>) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(0x15, 0x1a, 0x21));
    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, Color32::from_rgb(0x45, 0x55, 0x66)),
        egui::StrokeKind::Inside,
    );
    let plot = block_plot(rect);
    if plot.width() <= 1.0 || plot.height() <= 1.0 {
        return (false, None);
    }
    let bars = zoom_bars.max(1);
    let slot = plot.width() / bars as f32;
    let x0 = |bar: i32| plot.min.x + (bar as f32 - start) * slot;

    // Keep full run boundaries so a clipped block's dropdown reassigns the whole logical block.
    let default_pattern = patterns.get(&1).copied().unwrap_or(1).clamp(1, 32);
    let runs = pattern_runs(patterns, default_pattern, 1.0, 999);
    let mut menu_clicked = false;
    let mut edit = None;
    for (run_start, run_end, pattern) in runs {
        if run_end as f32 + 1.0 <= start || run_start as f32 >= start + bars as f32 {
            continue;
        }
        let color = pattern_color(pattern);
        let left = x0(run_start).max(plot.min.x);
        let right = x0(run_end + 1).min(plot.max.x);
        let block = Rect::from_min_max(
            Pos2::new(left, plot.min.y),
            Pos2::new(right, plot.max.y),
        );
        painter.rect_filled(block, 3.0, color);
        painter.rect_stroke(
            block,
            3.0,
            Stroke::new(1.0, color.gamma_multiply(0.85)),
            egui::StrokeKind::Inside,
        );
        if resize_mode && run_end < 999 && block.width() >= 12.0 {
            for offset in [3.0, 6.0] {
                let x = block.max.x - offset;
                painter.line_segment(
                    [
                        Pos2::new(x, block.min.y + 2.0),
                        Pos2::new(x, block.max.y - 2.0),
                    ],
                    Stroke::new(1.2, Color32::from_rgb(0x0b, 0x0b, 0x0b)),
                );
            }
        }
        if editable && !resize_mode && block.width() >= 14.0 {
            let arrow_rect = Rect::from_min_size(
                block.min + Vec2::new(1.0, (block.height() - 14.0).max(0.0) / 2.0),
                Vec2::new(12.0, 14.0_f32.min(block.height())),
            );
            painter.text(
                arrow_rect.center(),
                egui::Align2::CENTER_CENTER,
                "▾",
                egui::FontId::proportional(12.0),
                Color32::from_rgb(0x0b, 0x0b, 0x0b),
            );
            let button = ui.interact(
                arrow_rect,
                Id::unique(("pattern_block_dropdown", window_serial, run_start, run_end)),
                Sense::click(),
            );
            menu_clicked |= button.clicked();
            let current_bank = (pattern - 1) / 8;
            let current_loop = (pattern - 1) % 8 + 1;
            let mut picked = None;
            egui::Popup::menu(&button).show(|ui| {
                for bank in 0..4u8 {
                    let candidate = bank * 8 + current_loop;
                    if ui
                        .selectable_label(
                            bank == current_bank,
                            format!("Bank {}", char::from(b'A' + bank)),
                        )
                        .clicked()
                    {
                        picked = Some(candidate);
                        ui.close();
                    }
                }
                ui.separator();
                for pattern_loop in 1..=8u8 {
                    let candidate = current_bank * 8 + pattern_loop;
                    if ui
                        .selectable_label(
                            pattern_loop == current_loop,
                            format!("Pattern/Loop {pattern_loop}"),
                        )
                        .clicked()
                    {
                        picked = Some(candidate);
                        ui.close();
                    }
                }
            });
            if let Some(new_pattern) = picked {
                edit = Some(PatternBlockAction::Reassign(
                    run_start,
                    run_end,
                    pattern,
                    new_pattern,
                ));
            }
        }
        if block.width() >= if editable { 34.0 } else { 24.0 } {
            let text_x = if editable {
                block.min.x + 16.0
            } else {
                block.center().x
            };
            let align = if editable {
                egui::Align2::LEFT_CENTER
            } else {
                egui::Align2::CENTER_CENTER
            };
            painter.text(
                Pos2::new(text_x, block.center().y),
                align,
                crate::behavior::pattern_label(pattern - 1),
                egui::FontId::monospace(10.0),
                Color32::from_rgb(0x0b, 0x0b, 0x0b),
            );
        }
    }

    // The bar grid and its numbers.
    let first_boundary = start.ceil() as i32;
    let last_boundary = (start + bars as f32).floor() as i32;
    for bar in first_boundary..=last_boundary {
        let x = x0(bar);
        painter.line_segment(
            [Pos2::new(x, plot.min.y), Pos2::new(x, plot.max.y)],
            Stroke::new(0.7, Color32::from_rgb(0x32, 0x3e, 0x4d)),
        );
        painter.text(
            Pos2::new(x + slot * 0.5, plot.max.y + 10.0),
            egui::Align2::CENTER_CENTER,
            bar.to_string(),
            egui::FontId::monospace(8.0),
            Color32::from_rgb(0xff, 0xb0, 0x00),
        );
    }

    // The playhead and the selected bar.
    if let Some(bar) =
        selected_bar.filter(|bar| *bar as f32 >= start && (*bar as f32) < start + bars as f32)
    {
        let x = x0(bar);
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(x, plot.min.y), Pos2::new(x + slot, plot.max.y)),
            0.0,
            Color32::from_rgba_unmultiplied(0xff, 0xb0, 0x00, 24),
        );
    }
    if let Some(position) = playhead.filter(|p| *p >= start && *p < start + bars as f32) {
        let x = plot.min.x + (position - start) * slot;
        painter.line_segment(
            [Pos2::new(x, plot.min.y), Pos2::new(x, plot.max.y)],
            Stroke::new(2.0, Color32::from_rgb(0xff, 0x52, 0x52)),
        );
    }

    (menu_clicked, edit)
}

/// The effective pattern for a bar: the latest preceding pattern event, or the row's initial
/// pattern when there is no preceding event.
fn pattern_at_bar(patterns: &HashMap<i32, u8>, default_pattern: u8, bar: i32) -> u8 {
    patterns
        .iter()
        .filter(|(event_bar, _)| **event_bar <= bar)
        .max_by_key(|(event_bar, _)| **event_bar)
        .map_or(default_pattern, |(_, &p)| p.clamp(1, 32))
}

/// The visible runs of bars with the same effective pattern.
fn pattern_runs(
    patterns: &HashMap<i32, u8>,
    default_pattern: u8,
    start: f32,
    bars: i32,
) -> Vec<(i32, i32, u8)> {
    let first_bar = (start.floor() as i32).max(1);
    let end_bar = (start + bars as f32).ceil() as i32;
    if first_bar >= end_bar {
        return Vec::new();
    }
    let mut runs = Vec::new();
    let mut run_start = first_bar;
    let mut run_pattern = pattern_at_bar(patterns, default_pattern, first_bar);
    let mut changes: Vec<_> = patterns
        .iter()
        .filter(|(bar, _)| **bar > first_bar && **bar < end_bar)
        .map(|(&bar, &pattern)| (bar, pattern.clamp(1, 32)))
        .collect();
    changes.sort_unstable_by_key(|(bar, _)| *bar);
    for (bar, pattern) in changes {
        if pattern != run_pattern {
            runs.push((run_start, bar - 1, run_pattern));
            run_start = bar;
            run_pattern = pattern;
        }
    }
    runs.push((run_start, end_bar - 1, run_pattern));
    runs
}

#[cfg(test)]
mod tests {
    use super::{
        HashMap, PatternsOverview, block_bar_at, block_plot, follow_view_start, pattern_color,
        pattern_runs, patterns_tick_range,
    };
    use crate::model::BAR;
    use eframe::egui::{Pos2, Rect};

    #[test]
    fn block_timeline_maps_columns_to_bars() {
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 200.0));
        let plot = block_plot(rect);
        assert_eq!(
            block_bar_at(Pos2::new(plot.min.x, 50.0), rect, 10.0, 10),
            10
        );
        assert_eq!(
            block_bar_at(Pos2::new(plot.max.x, 50.0), rect, 10.0, 10),
            19
        );
    }

    #[test]
    fn every_pattern_has_a_distinct_color() {
        let mut seen = std::collections::HashSet::new();
        for p in 1..=32u8 {
            let c = pattern_color(p);
            assert!(seen.insert((c.r(), c.g(), c.b())), "pattern {p} color");
        }
    }

    #[test]
    fn block_runs_hold_the_previous_pattern_until_the_next_event() {
        let patterns = HashMap::from([(1, 2), (5, 4), (9, 4), (12, 8)]);
        assert_eq!(
            pattern_runs(&patterns, 1, 3.0, 10),
            [(3, 4, 2), (5, 11, 4), (12, 12, 8)]
        );
    }

    #[test]
    fn overview_keeps_the_full_window_visible_while_playing() {
        let mut o = PatternsOverview {
            serial: 1,
            window_bars: 8,
            scroll_bar: 0,
            last_song_bar: 0,
        };
        assert_eq!(patterns_tick_range(3 * BAR, true, &mut o), (0, 8 * BAR));
        assert_eq!(
            patterns_tick_range(8 * BAR, true, &mut o),
            (8 * BAR, 16 * BAR)
        );
    }

    #[test]
    fn playhead_scrolls_by_the_visible_window() {
        assert_eq!(follow_view_start(1.0, 16, 16.9, 1, 984), 1.0);
        assert_eq!(follow_view_start(1.0, 16, 17.0, 1, 984), 17.0);
        assert_eq!(follow_view_start(20.0, 16, 19.0, 1, 984), 19.0);
    }
}
