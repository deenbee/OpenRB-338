//! Automation curve editor window: visual inspection and editing of automation curves
//! for a single knob, fader, or button.

use eframe::egui::{self, Color32, Pos2, Rect, Response, RichText, Sense, Stroke, Ui, Vec2};
use orb_engine::seq::Event as TrackEvent;
use orb_engine::song::Device;
use std::collections::HashSet;

use crate::app::App;
use crate::model::BAR;
use crate::palette::{AMBER, INK_DIM, SLATE_LINE};

/// Snap grid resolution for automation events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SnapMode {
    #[default]
    Snap4,
    Snap8,
    Snap16,
    Snap32,
}

impl SnapMode {
    pub fn step_ticks(self) -> i32 {
        match self {
            SnapMode::Snap4 => 192,
            SnapMode::Snap8 => 96,
            SnapMode::Snap16 => 48,
            SnapMode::Snap32 => 24,
        }
    }

    pub fn snap_tick(self, tick: i32) -> i32 {
        let step = self.step_ticks();
        ((tick + step / 2) / step) * step
    }

    pub fn label(self) -> &'static str {
        match self {
            SnapMode::Snap4 => "1/4",
            SnapMode::Snap8 => "1/8",
            SnapMode::Snap16 => "1/16",
            SnapMode::Snap32 => "1/32",
        }
    }
}

fn snap_tick_in_range(snap: SnapMode, tick: i32, start: i32, end: i32) -> i32 {
    let step = snap.step_ticks();
    let first = ((start + step - 1) / step) * step;
    let last = (end - 1).div_euclid(step) * step;
    snap.snap_tick(tick).clamp(first, last)
}

/// How pointer actions edit automation points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditMode {
    /// Current behavior: draw on click/drag and delete with right click.
    #[default]
    Draw,
    /// Drag existing points only.
    Move,
    /// Add points only on empty positions.
    Add,
    /// Delete existing points on click.
    Delete,
}

/// The point currently being moved, tracked across frames.
#[derive(Clone, Copy, Debug)]
struct DragPoint {
    tick: i32,
    value: u8,
}

/// The state of an open automation editor window.
#[derive(Clone, Debug)]
pub struct AutomationEditor {
    pub control_id: String,
    pub title: String,
    pub device: Device,
    pub ctrl: u8,
    pub is_binary: bool,
    pub loop_mode: bool,
    pub snap: SnapMode,
    pub edit_mode: EditMode,
    dragging: Option<DragPoint>,
    pub serial: u32,
}

/// Minimum number of bars visible in the all-automation window.
const ALL_AUTO_MIN_BARS: i32 = 4;
/// Maximum number of bars visible in the all-automation window.
const ALL_AUTO_MAX_BARS: i32 = 32;
/// Bars the all-automation view advances when the playhead reaches an edge.
const ALL_AUTO_SCROLL_STEP: i32 = 3;

fn rounded_scroll_distance(distance: i32) -> i32 {
    let remainder = distance % ALL_AUTO_SCROLL_STEP;
    if remainder == 0 {
        distance
    } else {
        distance + ALL_AUTO_SCROLL_STEP - remainder
    }
}

/// State of the single window that shows all song-automation lanes.
#[derive(Clone, Debug)]
pub struct AllAutomationEditor {
    /// Number of visible bars (clamped to 4..=32).
    pub window_bars: i32,
    /// First visible bar (0-indexed) when scrolling in stop mode.
    pub scroll_bar: i32,
    /// Last transport bar seen, used to sync `scroll_bar` when the song position moves.
    pub last_song_bar: i32,
    pub serial: u32,
}

#[derive(Clone)]
struct AutomationLane {
    id: String,
    title: String,
    max_value: u8,
    is_binary: bool,
    is_pattern: bool,
    pattern_row: Option<usize>,
    events: Vec<TrackEvent>,
}

/// Generates a human-readable title for a control layout ID.
pub fn control_title(id: &str) -> String {
    let (dev_part, name_part) = id.split_once('.').unwrap_or(("", id));
    let dev_name = match dev_part {
        "tb303_1" => "TB-303 I",
        "tb303_2" => "TB-303 II",
        "tr808" => "TR-808",
        "tr909" => "TR-909",
        "delay" => "Delay",
        "dist" => "Distortion",
        "comp" => "Compressor",
        "pcf" => "PCF",
        s if s.starts_with("mixer") => "Mixer",
        "master" => "Master",
        _ => dev_part,
    };
    let ctl_name = match name_part {
        "cutoff" => "Cutoff",
        "reso" => "Resonance",
        "env_mod" => "Env Mod",
        "decay" => "Decay",
        "accent" => "Accent",
        "tune" => "Tune",
        "wave" => "Waveform",
        "feedback" => "Feedback",
        "steps" => "Steps",
        "pan" => "Pan",
        "amount" => "Amount",
        "shape" => "Shape",
        "threshold" => "Threshold",
        "ratio" => "Ratio",
        "freq" => "Cutoff Frequency",
        "q" => "Resonance (Q)",
        "amt" => "Envelope Amount",
        "dec" => "Decay",
        "mode" => "Filter Mode",
        "pattern" => "Pattern",
        "on" => "On/Off",
        "level" => "Level",
        "fader" => "Fader",
        "dist" => "Distortion",
        "mute" => "Mute",
        "pcf" => "PCF",
        "comp" => "Compressor",
        _ => name_part,
    };
    format!("{dev_name} {ctl_name}")
}

/// Whether the control is a two-state switch/button (0 or 1).
pub fn is_binary_control(id: &str, ctrl: u8) -> bool {
    id.ends_with(".on")
        || id.ends_with(".mute")
        || id.ends_with(".dist")
        || id.ends_with(".triplet")
        || id.ends_with(".pcf")
        || id.ends_with(".comp")
        || id.ends_with(".wave")
        || id.starts_with("pattern") && id.ends_with(".on")
        || ctrl == 0
}

/// Maximum automation value for a control whose range is narrower than a MIDI byte.
fn control_value_max(id: &str, is_binary: bool) -> u8 {
    if is_binary {
        1
    } else {
        match id {
            "pcf.pattern" => 54,
            "delay.steps" => 32,
            _ => 127,
        }
    }
}

/// Updates `editor` from the transport state and returns the visible `(tick_start, tick_end)`
/// window. While playing, the window stays still until the playhead reaches an edge, then moves
/// in three-bar increments. A transport move while stopped anchors the window to that bar.
fn all_automation_tick_range(
    song_pos: i32,
    playing: bool,
    editor: &mut AllAutomationEditor,
) -> (i32, i32) {
    editor.window_bars = editor
        .window_bars
        .clamp(ALL_AUTO_MIN_BARS, ALL_AUTO_MAX_BARS);
    let song_bar = (song_pos / BAR).clamp(0, 998);
    let max_start_bar = (999 - editor.window_bars).max(0);
    if playing {
        if song_bar < editor.scroll_bar {
            let distance = editor.scroll_bar - song_bar;
            editor.scroll_bar = (editor.scroll_bar - rounded_scroll_distance(distance)).max(0);
        } else if song_bar >= editor.scroll_bar + editor.window_bars {
            let target_start = song_bar - editor.window_bars + 1;
            let distance = target_start - editor.scroll_bar;
            editor.scroll_bar =
                (editor.scroll_bar + rounded_scroll_distance(distance)).min(max_start_bar);
        }
        editor.last_song_bar = song_bar;
    } else if song_bar != editor.last_song_bar {
        editor.scroll_bar = song_bar;
        editor.last_song_bar = song_bar;
    }
    editor.scroll_bar = editor.scroll_bar.clamp(0, max_start_bar);
    let tick_start = editor.scroll_bar * BAR;
    let tick_end = (editor.scroll_bar + editor.window_bars) * BAR;
    (tick_start, tick_end)
}

impl App {
    /// Opens the overview of every control that currently has song automation.
    pub fn open_all_automation(&mut self) {
        if !self.panel.transport.song_mode {
            return;
        }
        let song_bar = (self.panel.transport.position / BAR).clamp(0, 998);
        let serial = self
            .all_auto_editor
            .as_ref()
            .map_or(1, |editor| editor.serial + 1);
        self.all_auto_editor = Some(AllAutomationEditor {
            window_bars: ALL_AUTO_MIN_BARS,
            scroll_bar: song_bar,
            last_song_bar: song_bar,
            serial,
        });
    }

    /// Opens the automation curve editor for control `id`.
    pub fn open_automation_editor(&mut self, id: &str) {
        let Some(link) = &self.link else {
            return;
        };
        let Some((device, ctrl)) = link.resolve_control(id) else {
            return;
        };
        let is_binary = is_binary_control(id, ctrl);
        let title = control_title(id);
        let loop_mode = self.panel.transport.looping;
        let serial = self.auto_editor.as_ref().map_or(1, |e| e.serial + 1);
        self.auto_editor = Some(AutomationEditor {
            control_id: id.to_string(),
            title,
            device,
            ctrl,
            is_binary,
            loop_mode,
            snap: SnapMode::Snap16,
            edit_mode: EditMode::Draw,
            dragging: None,
            serial,
        });
    }

    /// Renders one vertically scrollable overview of the active automation lanes.
    pub fn all_automation_ui(&mut self, ctx: &egui::Context) {
        if !self.panel.transport.song_mode {
            self.all_auto_editor = None;
            return;
        }
        let Some(mut editor) = self.all_auto_editor.clone() else {
            return;
        };
        let lanes = self.automation_lanes();
        let song_last_tick = self
            .link
            .as_ref()
            .into_iter()
            .flat_map(|link| link.loaded_tracks().iter().flatten())
            .map(|event| event.tick)
            .max()
            .unwrap_or(0);
        let range_end = song_range_end(&lanes, song_last_tick);
        let playing = self.panel.transport.playing;
        let playhead = self.panel.transport.position;
        let (tick_start, tick_end) = all_automation_tick_range(playhead, playing, &mut editor);
        let start_bar_1 = tick_start / BAR + 1;
        let end_bar_1 = tick_end / BAR;
        let mut open = true;
        let mut edit_control = None;
        let title = "All Automations";
        let id = egui::Id::unique(("orb_all_automation", editor.serial));
        let content_size = ctx.content_rect().size();
        let max_size = Vec2::new(
            (content_size.x - 48.0).clamp(0.0, 900.0),
            (content_size.y * 0.9).max(0.0),
        );
        let min_size = Vec2::new(480.0, 320.0).min(max_size);
        egui::Window::new(title)
            .id(id)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(Vec2::new(720.0, 620.0))
            .min_size(min_size)
            .max_size(max_size)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("View:").color(INK_DIM));
                    for bars in [4, 8, 16, 32] {
                        if ui
                            .selectable_label(editor.window_bars == bars, format!("{bars} Bars"))
                            .clicked()
                        {
                            editor.window_bars = bars;
                        }
                    }
                    ui.add(
                        egui::DragValue::new(&mut editor.window_bars)
                            .range(ALL_AUTO_MIN_BARS..=ALL_AUTO_MAX_BARS)
                            .suffix(" bars"),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!("Bars {start_bar_1}–{end_bar_1}")).color(AMBER),
                    );
                    ui.separator();
                    ui.label(RichText::new(format!("{} active lanes", lanes.len())).color(INK_DIM));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Song position:").color(INK_DIM));
                    self.transport_bar_control(ui, "all_automation");
                });
                ui.add_space(5.0);

                if lanes.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new("No active song automation").color(INK_DIM));
                    });
                    return;
                }

                let footer_h = if !playing { 32.0 } else { 0.0 };
                let list_h = (ui.available_height() - footer_h).max(120.0);
                egui::ScrollArea::vertical()
                    .id_salt("all_automation_lanes")
                    .max_height(list_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for lane in &lanes {
                            let lane_events: Vec<_> = lane
                                .events
                                .iter()
                                .copied()
                                .filter(|event| event.tick >= tick_start && event.tick < tick_end)
                                .collect();
                            egui::Frame::group(ui.style()).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&lane.title).strong());
                                    ui.label(
                                        RichText::new(format!("{} points", lane.events.len()))
                                            .color(INK_DIM),
                                    );
                                    if ui.small_button("Edit…").clicked() {
                                        edit_control = Some(lane.id.clone());
                                    }
                                });
                                let (rect, _) = ui.allocate_exact_size(
                                    Vec2::new(ui.available_width(), 112.0),
                                    Sense::hover(),
                                );
                                paint_lane(
                                    ui,
                                    rect,
                                    lane,
                                    &lane_events,
                                    tick_start,
                                    tick_end,
                                    playhead,
                                );
                            });
                            ui.add_space(3.0);
                        }
                    });

                if !playing {
                    ui.separator();
                    let max_start_bar = (999 - editor.window_bars).max(0);
                    let total_bars = ((range_end / BAR)
                        .max(self.panel.transport.loop_end / BAR)
                        .max(editor.scroll_bar + editor.window_bars)
                        .max(32))
                    .clamp(editor.window_bars, 999);
                    let slider_max_bar = (total_bars - editor.window_bars + 1).max(1);
                    let mut bar_1 = (editor.scroll_bar + 1).clamp(1, slider_max_bar);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Scroll:").color(INK_DIM));
                        if ui
                            .add_enabled(editor.scroll_bar > 0, egui::Button::new("◀"))
                            .clicked()
                        {
                            editor.scroll_bar = (editor.scroll_bar - 1).max(0);
                            bar_1 = editor.scroll_bar + 1;
                        }
                        let slider_w = (ui.available_width() - 90.0).max(80.0);
                        ui.spacing_mut().slider_width = slider_w;
                        if ui
                            .add(egui::Slider::new(&mut bar_1, 1..=slider_max_bar).prefix("Bar "))
                            .changed()
                        {
                            editor.scroll_bar = (bar_1 - 1).clamp(0, max_start_bar);
                        }
                        let right_btn = egui::Button::new("▶");
                        if ui.add_enabled(editor.scroll_bar < max_start_bar, right_btn).clicked() {
                            editor.scroll_bar = (editor.scroll_bar + 1).min(max_start_bar);
                        }
                    });
                }
            });

        if !open {
            self.all_auto_editor = None;
        } else {
            self.all_auto_editor = Some(editor);
        }
        if let Some(id) = edit_control {
            if let Some(row) = lanes
                .iter()
                .find(|lane| lane.id == id)
                .and_then(|lane| lane.pattern_row)
            {
                self.open_pattern_automation(&format!("pattern{}.pattern1", row + 1));
            } else {
                self.open_automation_editor(&id);
            }
        }
    }

    fn automation_lanes(&self) -> Vec<AutomationLane> {
        let Some(link) = &self.link else {
            return Vec::new();
        };
        let tracks = link.loaded_tracks();
        let mut seen = HashSet::new();
        let mut lanes = Vec::new();
        for control in crate::layout::controls() {
            let Some((device, ctrl)) = link.resolve_control(&control.id) else {
                continue;
            };
            let key = (device.id() as usize, ctrl);
            if !seen.insert(key) {
                continue;
            }
            let events: Vec<_> = tracks[key.0]
                .iter()
                .filter(|event| event.ctrl == ctrl)
                .copied()
                .collect();
            if events.is_empty() {
                continue;
            }
            let mut events = events;
            events.sort_by_key(|event| event.tick);
            let is_binary = is_binary_control(&control.id, ctrl);
            let max_value = control_value_max(&control.id, is_binary);
            let title = control_title(&control.id);
            lanes.push(AutomationLane {
                id: control.id,
                title,
                max_value,
                is_binary,
                is_pattern: false,
                pattern_row: None,
                events,
            });
        }
        for (row, patterns) in self.panel.pattern_automation.iter().enumerate() {
            let mut patterns = patterns.clone();
            patterns.entry(1).or_insert(1);
            let events = patterns
                .iter()
                .map(|(&bar, &pattern)| TrackEvent {
                    tick: (bar - 1) * BAR,
                    ctrl: 0,
                    value: pattern.clamp(1, 32),
                })
                .collect();
            let name = match row {
                0 => "TB-303 I",
                1 => "TB-303 II",
                2 => "TR-808",
                _ => "TR-909",
            };
            lanes.push(AutomationLane {
                id: format!("pattern{}.pattern1", row + 1),
                title: format!("{name} Pattern"),
                max_value: 32,
                is_binary: false,
                is_pattern: true,
                pattern_row: Some(row),
                events,
            });
        }
        for lane in &mut lanes {
            lane.events.sort_by_key(|event| event.tick);
        }
        lanes
    }

    /// Renders the automation curve editor window if open.
    pub fn automation_editor_ui(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.auto_editor.clone() else {
            return;
        };
        let mut open = true;
        let title = format!("Automation: {}", editor.title);
        let win_id = egui::Id::unique(("orb_auto_editor", editor.serial));

        let screen = ctx.content_rect();
        let default_size = Vec2::new(560.0, 320.0);

        egui::Window::new(title)
            .id(win_id)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(screen.center())
            .default_size(default_size)
            .min_size(Vec2::new(380.0, 220.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (mode, icon, help) in [
                        (
                            EditMode::Draw,
                            "✎",
                            "Draw and move points; right click to delete",
                        ),
                        (EditMode::Move, "↔", "Move existing points only"),
                        (EditMode::Add, "+", "Add points on empty positions only"),
                        (EditMode::Delete, "−", "Delete existing points only"),
                    ] {
                        let response = ui.selectable_label(editor.edit_mode == mode, icon);
                        if response.clicked() {
                            editor.edit_mode = mode;
                            editor.dragging = None;
                        }
                        response.on_hover_text(help);
                    }
                    ui.separator();
                    let loop_avail = self.panel.transport.looping;
                    if ui
                        .selectable_label(!editor.loop_mode, "Current Bar")
                        .clicked()
                    {
                        editor.loop_mode = false;
                    }
                    ui.add_enabled_ui(loop_avail, |ui| {
                        if ui
                            .selectable_label(editor.loop_mode, "Loop Range")
                            .clicked()
                        {
                            editor.loop_mode = true;
                        }
                    });
                    ui.separator();
                    ui.label(RichText::new("Snap:").color(INK_DIM));
                    for &snap in &[
                        SnapMode::Snap4,
                        SnapMode::Snap8,
                        SnapMode::Snap16,
                        SnapMode::Snap32,
                    ] {
                        if ui
                            .selectable_label(editor.snap == snap, snap.label())
                            .clicked()
                        {
                            editor.snap = snap;
                        }
                    }
                    ui.separator();
                    if ui.button("Clear Range").clicked() {
                        let (start, end) = self.editor_tick_range(&editor);
                        if let Some(link) = &mut self.link {
                            link.delete_track_events(
                                editor.device.id() as usize,
                                start,
                                end,
                                Some(editor.ctrl),
                            );
                            self.doc.modified = true;
                        }
                    }
                });

                ui.add_space(4.0);

                // Main automation drawing canvas
                let avail = ui.available_size();
                let canvas_size = Vec2::new(avail.x, avail.y - 4.0);
                let (rect, resp) = ui.allocate_exact_size(canvas_size, Sense::click_and_drag());
                self.paint_and_edit_canvas(ui, &resp, rect, &mut editor);
            });

        if !open {
            self.auto_editor = None;
        } else {
            self.auto_editor = Some(editor);
        }
    }

    /// Computes the start and end ticks to display in the editor.
    fn editor_tick_range(&self, editor: &AutomationEditor) -> (i32, i32) {
        let t = &self.panel.transport;
        if editor.loop_mode && t.looping {
            let start = t.loop_start;
            let end = t.loop_end.max(start + BAR);
            (start, end)
        } else {
            let b = t.position / BAR;
            (b * BAR, (b + 1) * BAR)
        }
    }

    /// Paints the grid, curve, and points, and handles mouse interaction.
    fn paint_and_edit_canvas(
        &mut self,
        ui: &mut Ui,
        resp: &Response,
        rect: Rect,
        editor: &mut AutomationEditor,
    ) {
        let painter = ui.painter_at(rect);
        // Background
        painter.rect_filled(rect, 4.0, Color32::from_rgb(0x15, 0x1a, 0x21));
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, SLATE_LINE),
            egui::StrokeKind::Inside,
        );

        let (tick_start, tick_end) = self.editor_tick_range(editor);
        let total_ticks = (tick_end - tick_start).max(1) as f32;
        let track_idx = editor.device.id() as usize;

        let margin_l = 32.0;
        let margin_b = 20.0;
        let margin_t = 12.0;
        let margin_r = 12.0;
        let plot_rect = Rect::from_min_max(
            Pos2::new(rect.min.x + margin_l, rect.min.y + margin_t),
            Pos2::new(rect.max.x - margin_r, rect.max.y - margin_b),
        );

        if plot_rect.width() <= 10.0 || plot_rect.height() <= 10.0 {
            return;
        }

        // Draw horizontal grid lines & labels
        let max_value = control_value_max(&editor.control_id, editor.is_binary);
        let max_val = max_value as f32;
        let grid_y_vals: Vec<u8> = if editor.is_binary {
            vec![0, 1]
        } else {
            (0..=4)
                .map(|i| (u16::from(max_value) * i / 4) as u8)
                .collect()
        };

        for &value in &grid_y_vals {
            let v = value as f32;
            let frac = v / max_val;
            let y = plot_rect.max.y - frac * plot_rect.height();
            painter.line_segment(
                [Pos2::new(plot_rect.min.x, y), Pos2::new(plot_rect.max.x, y)],
                Stroke::new(1.0, Color32::from_rgb(0x24, 0x2d, 0x38)),
            );
            let lbl = if editor.is_binary {
                if value == 1 {
                    "ON".to_string()
                } else {
                    "OFF".to_string()
                }
            } else {
                value.to_string()
            };
            painter.text(
                Pos2::new(plot_rect.min.x - 4.0, y),
                egui::Align2::RIGHT_CENTER,
                &lbl,
                egui::FontId::monospace(10.0),
                INK_DIM,
            );
        }

        // Draw vertical grid lines (bars & 16th note steps)
        let num_bars = ((tick_end - tick_start) / BAR).max(1);
        for b in 0..=num_bars {
            let t = tick_start + b * BAR;
            let frac = (t - tick_start) as f32 / total_ticks;
            let x = plot_rect.min.x + frac * plot_rect.width();
            painter.line_segment(
                [Pos2::new(x, plot_rect.min.y), Pos2::new(x, plot_rect.max.y)],
                Stroke::new(1.0, Color32::from_rgb(0x32, 0x3e, 0x4d)),
            );
            let bar_num = t / BAR + 1;
            painter.text(
                Pos2::new(x + 2.0, plot_rect.max.y + 10.0),
                egui::Align2::LEFT_CENTER,
                format!("{bar_num}"),
                egui::FontId::monospace(10.0),
                AMBER,
            );
        }

        // Playhead cursor line
        let cur_tick = self.panel.transport.position;
        if cur_tick >= tick_start && cur_tick <= tick_end {
            let frac = (cur_tick - tick_start) as f32 / total_ticks;
            let x = plot_rect.min.x + frac * plot_rect.width();
            painter.line_segment(
                [Pos2::new(x, plot_rect.min.y), Pos2::new(x, plot_rect.max.y)],
                Stroke::new(1.5, Color32::from_rgb(0xff, 0x4d, 0x4d)),
            );
        }

        // Get track events for this controller in the visible range
        let loaded = self.link.as_ref().map(|l| l.loaded_tracks());
        let events: Vec<TrackEvent> = loaded
            .map(|all| {
                all[track_idx]
                    .iter()
                    .filter(|e| e.ctrl == editor.ctrl && e.tick >= tick_start && e.tick < tick_end)
                    .copied()
                    .collect()
            })
            .unwrap_or_default();

        let tick_to_x = |t: i32| -> f32 {
            let frac = ((t - tick_start) as f32 / total_ticks).clamp(0.0, 1.0);
            plot_rect.min.x + frac * plot_rect.width()
        };
        let val_to_y = |v: u8| -> f32 {
            let frac = (v as f32 / max_val).clamp(0.0, 1.0);
            plot_rect.max.y - frac * plot_rect.height()
        };
        let x_to_tick = |x: f32| -> i32 {
            let frac = ((x - plot_rect.min.x) / plot_rect.width()).clamp(0.0, 1.0);
            tick_start + (frac * total_ticks).round() as i32
        };
        let y_to_val = |y: f32| -> u8 {
            let frac = ((plot_rect.max.y - y) / plot_rect.height()).clamp(0.0, 1.0);
            (frac * max_val).round() as u8
        };

        let curve_color = Color32::from_rgb(0x38, 0xd0, 0x70);
        let node_color = Color32::from_rgb(0x56, 0xff, 0x9e);

        let prev_val = loaded.as_ref().and_then(|all| {
            all[track_idx]
                .iter()
                .filter(|e| e.ctrl == editor.ctrl && e.tick <= tick_start)
                .max_by_key(|e| e.tick)
                .map(|e| e.value)
        });

        // Draw curve lines
        if !events.is_empty() {
            let mut pts: Vec<Pos2> = Vec::with_capacity(events.len() + 3);
            let start_val = prev_val.unwrap_or(events[0].value);
            pts.push(Pos2::new(plot_rect.min.x, val_to_y(start_val)));
            for e in &events {
                let p = Pos2::new(tick_to_x(e.tick), val_to_y(e.value));
                if editor.is_binary
                    && let Some(prev) = pts.last()
                {
                    pts.push(Pos2::new(p.x, prev.y));
                }
                pts.push(p);
            }
            let last = *events.last().unwrap();
            pts.push(Pos2::new(plot_rect.max.x, val_to_y(last.value)));

            for w in pts.windows(2) {
                painter.line_segment([w[0], w[1]], Stroke::new(2.0, curve_color));
            }
        } else if let Some(val) = prev_val {
            let y = val_to_y(val);
            painter.line_segment(
                [Pos2::new(plot_rect.min.x, y), Pos2::new(plot_rect.max.x, y)],
                Stroke::new(1.5, curve_color.gamma_multiply(0.4)),
            );
        }

        // Draw point handles and check hit
        let hover_pos = resp.hover_pos();
        let mut hit_idx: Option<usize> = None;
        for (i, e) in events.iter().enumerate() {
            let center = Pos2::new(tick_to_x(e.tick), val_to_y(e.value));
            let hovered = hover_pos.is_some_and(|h| h.distance(center) <= 8.0);
            if hovered {
                hit_idx = Some(i);
            }
            let fill = if hovered { AMBER } else { node_color };
            painter.circle_filled(center, 4.5, fill);
            painter.circle_stroke(center, 4.5, Stroke::new(1.0, Color32::BLACK));
        }

        // Mouse interactions on the canvas
        if let Some(pos) = hover_pos
            && plot_rect.contains(pos)
        {
            let delete_point = |link: &mut crate::link::Link, event: TrackEvent| {
                link.delete_track_events(track_idx, event.tick, event.tick + 1, Some(event.ctrl));
            };
            match editor.edit_mode {
                EditMode::Draw if resp.secondary_clicked() => {
                    if let Some(idx) = hit_idx
                        && let Some(link) = &mut self.link
                    {
                        delete_point(link, events[idx]);
                        self.doc.modified = true;
                    }
                }
                EditMode::Draw if resp.clicked() || resp.dragged() => {
                    let tick =
                        snap_tick_in_range(editor.snap, x_to_tick(pos.x), tick_start, tick_end);
                    let value = y_to_val(pos.y);
                    if let Some(link) = &mut self.link {
                        link.delete_track_events(track_idx, tick, tick + 1, Some(editor.ctrl));
                        link.insert_track_events(
                            track_idx,
                            vec![TrackEvent {
                                tick,
                                ctrl: editor.ctrl,
                                value,
                            }],
                        );
                        self.doc.modified = true;
                    }
                }
                EditMode::Move if resp.drag_started() => {
                    let press_pos = ui.input(|i| i.pointer.press_origin());
                    editor.dragging = press_pos
                        .and_then(|p| {
                            events.iter().position(|e| {
                                let center = Pos2::new(tick_to_x(e.tick), val_to_y(e.value));
                                p.distance(center) <= 8.0
                            })
                        })
                        .map(|idx| DragPoint {
                            tick: events[idx].tick,
                            value: events[idx].value,
                        });
                }
                EditMode::Move if resp.dragged() => {
                    if let Some(mut point) = editor.dragging
                        && let Some(link) = &mut self.link
                    {
                        let tick =
                            snap_tick_in_range(editor.snap, x_to_tick(pos.x), tick_start, tick_end);
                        let value = y_to_val(pos.y);
                        let occupied = tick != point.tick
                            && events
                                .iter()
                                .any(|event| event.ctrl == editor.ctrl && event.tick == tick);
                        if !occupied && (tick != point.tick || value != point.value) {
                            link.delete_track_events(
                                track_idx,
                                point.tick,
                                point.tick + 1,
                                Some(editor.ctrl),
                            );
                            link.insert_track_events(
                                track_idx,
                                vec![TrackEvent {
                                    tick,
                                    ctrl: editor.ctrl,
                                    value,
                                }],
                            );
                            point.tick = tick;
                            point.value = value;
                            editor.dragging = Some(point);
                            self.doc.modified = true;
                        }
                    }
                }
                EditMode::Add if resp.clicked() && hit_idx.is_none() => {
                    let tick =
                        snap_tick_in_range(editor.snap, x_to_tick(pos.x), tick_start, tick_end);
                    let occupied = events
                        .iter()
                        .any(|event| event.ctrl == editor.ctrl && event.tick == tick);
                    if !occupied && let Some(link) = &mut self.link {
                        link.insert_track_events(
                            track_idx,
                            vec![TrackEvent {
                                tick,
                                ctrl: editor.ctrl,
                                value: y_to_val(pos.y),
                            }],
                        );
                        self.doc.modified = true;
                    }
                }
                EditMode::Delete if resp.clicked() => {
                    if let Some(idx) = hit_idx
                        && let Some(link) = &mut self.link
                    {
                        delete_point(link, events[idx]);
                        self.doc.modified = true;
                    }
                }
                _ => {}
            }
        }
        if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
            editor.dragging = None;
        }
    }
}

fn song_range_end(lanes: &[AutomationLane], song_last_tick: i32) -> i32 {
    let last_tick = lanes
        .iter()
        .flat_map(|lane| lane.events.iter().map(|event| event.tick))
        .max()
        .unwrap_or(0)
        .max(song_last_tick);
    ((last_tick / BAR) + 1).clamp(1, 999) * BAR
}

fn paint_lane(
    ui: &Ui,
    rect: Rect,
    lane: &AutomationLane,
    events: &[TrackEvent],
    tick_start: i32,
    tick_end: i32,
    playhead: i32,
) {
    let max_value = lane.max_value;
    let is_binary = lane.is_binary;
    let is_pattern = lane.is_pattern;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(0x15, 0x1a, 0x21));
    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, SLATE_LINE),
        egui::StrokeKind::Inside,
    );
    let plot = Rect::from_min_max(
        Pos2::new(rect.min.x + 36.0, rect.min.y + 8.0),
        Pos2::new(rect.max.x - 8.0, rect.max.y - 17.0),
    );
    let max_value = max_value.max(1) as f32;
    let total_ticks = (tick_end - tick_start).max(1) as f32;
    let value_y =
        |value: u8| plot.max.y - (value as f32 / max_value).clamp(0.0, 1.0) * plot.height();
    let tick_x = |tick: i32| {
        plot.min.x + ((tick - tick_start) as f32 / total_ticks).clamp(0.0, 1.0) * plot.width()
    };

    let grid_values: &[u8] = if is_binary {
        &[0, 1]
    } else if is_pattern {
        &[1, 16, 32]
    } else {
        &[0, 64, 127]
    };
    for &value in grid_values {
        let value = if is_binary || is_pattern {
            value
        } else {
            ((u16::from(max_value as u8) * u16::from(value)) / 127) as u8
        };
        let y = value_y(value);
        painter.line_segment(
            [Pos2::new(plot.min.x, y), Pos2::new(plot.max.x, y)],
            Stroke::new(1.0, Color32::from_rgb(0x24, 0x2d, 0x38)),
        );
        let label = if is_binary {
            if value == 0 { "OFF" } else { "ON" }.to_string()
        } else if is_pattern {
            crate::behavior::pattern_label(value - 1)
        } else {
            value.to_string()
        };
        painter.text(
            Pos2::new(plot.min.x - 4.0, y),
            egui::Align2::RIGHT_CENTER,
            label,
            egui::FontId::monospace(9.0),
            INK_DIM,
        );
    }

    let bars = ((tick_end - tick_start) / BAR).max(1);
    let bar_stride = (bars / 16).max(1);
    for bar in 0..=bars {
        if bar != bars && bar % bar_stride != 0 {
            continue;
        }
        let tick = tick_start + bar * BAR;
        let x = tick_x(tick);
        painter.line_segment(
            [Pos2::new(x, plot.min.y), Pos2::new(x, plot.max.y)],
            Stroke::new(1.0, Color32::from_rgb(0x32, 0x3e, 0x4d)),
        );
        painter.text(
            Pos2::new(x + 2.0, rect.max.y - 8.0),
            egui::Align2::LEFT_CENTER,
            (tick / BAR + 1).to_string(),
            egui::FontId::monospace(9.0),
            AMBER,
        );
    }

    if events.is_empty() {
        painter.text(
            plot.center(),
            egui::Align2::CENTER_CENTER,
            "No points in this range",
            egui::FontId::proportional(10.0),
            INK_DIM,
        );
    } else {
        let color = Color32::from_rgb(0x38, 0xd0, 0x70);
        let mut points = Vec::with_capacity(events.len() * 2 + 2);
        points.push(Pos2::new(plot.min.x, value_y(events[0].value)));
        for event in events {
            let point = Pos2::new(tick_x(event.tick), value_y(event.value));
            if (is_binary || is_pattern)
                && let Some(prev) = points.last()
            {
                points.push(Pos2::new(point.x, prev.y));
            }
            points.push(point);
        }
        let last = events.last().unwrap();
        points.push(Pos2::new(plot.max.x, value_y(last.value)));
        for pair in points.windows(2) {
            painter.line_segment([pair[0], pair[1]], Stroke::new(2.0, color));
        }
        for event in events {
            let center = Pos2::new(tick_x(event.tick), value_y(event.value));
            painter.circle_filled(center, 3.5, Color32::from_rgb(0x56, 0xff, 0x9e));
            painter.circle_stroke(center, 3.5, Stroke::new(1.0, Color32::BLACK));
        }
    }
    if (tick_start..=tick_end).contains(&playhead) {
        let x = tick_x(playhead);
        painter.line_segment(
            [Pos2::new(x, plot.min.y), Pos2::new(x, plot.max.y)],
            Stroke::new(1.5, Color32::from_rgb(0xff, 0x4d, 0x4d)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AllAutomationEditor, AutomationLane, SnapMode, all_automation_tick_range,
        control_value_max, is_binary_control, snap_tick_in_range, song_range_end,
    };
    use crate::model::BAR;
    use orb_engine::seq::Event as TrackEvent;

    #[test]
    fn all_automation_playhead_traverses_visible_bars_before_scrolling() {
        let mut editor = AllAutomationEditor {
            window_bars: 2,
            scroll_bar: 0,
            last_song_bar: 0,
            serial: 1,
        };
        assert_eq!(
            all_automation_tick_range(BAR * 3, true, &mut editor),
            (0, BAR * 4)
        );
        assert_eq!(editor.window_bars, 4);
        assert_eq!(editor.scroll_bar, 0);
        assert_eq!(
            all_automation_tick_range(BAR * 4, true, &mut editor),
            (BAR * 3, BAR * 7)
        );
        assert_eq!(editor.scroll_bar, 3);
        assert_eq!(
            all_automation_tick_range(BAR * 6, true, &mut editor),
            (BAR * 3, BAR * 7)
        );
        assert_eq!(
            all_automation_tick_range(BAR * 7, true, &mut editor),
            (BAR * 6, BAR * 10)
        );

        editor.window_bars = 8;
        editor.scroll_bar = 0;
        assert_eq!(
            all_automation_tick_range(BAR * 7, true, &mut editor),
            (0, BAR * 8)
        );
        assert_eq!(
            all_automation_tick_range(BAR * 8, true, &mut editor),
            (BAR * 3, BAR * 11)
        );
        assert_eq!(
            all_automation_tick_range(0, true, &mut editor),
            (0, BAR * 8)
        );

        // When stopped at the same song bar, manual horizontal scroll is preserved.
        editor.scroll_bar = 10;
        editor.window_bars = 40;
        assert_eq!(
            all_automation_tick_range(0, false, &mut editor),
            (BAR * 10, BAR * 42)
        );
        assert_eq!(editor.window_bars, 32);

        // Moving the transport bar while stopped re-syncs to the new song position.
        assert_eq!(
            all_automation_tick_range(BAR * 5, false, &mut editor),
            (BAR * 5, BAR * 37)
        );
    }

    #[test]
    fn automation_ranges_match_the_controls() {
        assert_eq!(control_value_max("pcf.pattern", false), 54);
        assert_eq!(control_value_max("delay.steps", false), 32);
        assert_eq!(control_value_max("tb303_1.wave", true), 1);
        assert_eq!(control_value_max("tb303_1.cutoff", false), 127);
        assert!(is_binary_control("tb303_1.wave", 8));
    }

    #[test]
    fn snap_choices_use_rbs_positions_and_stay_inside_the_range() {
        assert_eq!(SnapMode::Snap4.step_ticks(), 192);
        assert_eq!(SnapMode::Snap8.step_ticks(), 96);
        assert_eq!(SnapMode::Snap16.step_ticks(), 48);
        assert_eq!(SnapMode::Snap32.step_ticks(), 24);
        assert_eq!(snap_tick_in_range(SnapMode::Snap32, 767, 0, 768), 744);
        assert_eq!(snap_tick_in_range(SnapMode::Snap16, 0, 768, 1536), 768);
    }

    #[test]
    fn song_range_ends_at_the_song_end_and_covers_all_automation() {
        let lane = AutomationLane {
            id: "tb303_1.cutoff".into(),
            title: "TB-303 I Cutoff".into(),
            max_value: 127,
            is_binary: false,
            is_pattern: false,
            pattern_row: None,
            events: vec![TrackEvent {
                tick: BAR * 2 + 10,
                ctrl: 1,
                value: 80,
            }],
        };
        assert_eq!(
            song_range_end(std::slice::from_ref(&lane), BAR * 4),
            BAR * 5
        );
        assert_eq!(song_range_end(&[lane], BAR), BAR * 3);
        assert_eq!(song_range_end(&[], 0), BAR);
    }
}
