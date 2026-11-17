//! Pattern automation editor window: per-bar pattern selection for a pattern
//! section. Each bar (1..=999, or the loop range when looping) holds exactly
//! one pattern number (1..=32), as in the original ReBirth. No snap grid:
//! one pattern per bar, no subdivision. A zoom slider and horizontal range slider navigate
//! the 1..=999 song range without making the editor excessively wide.

use std::collections::HashMap;

use eframe::egui::{self, Color32, Id, Pos2, Rect, Sense, Stroke, Ui, Vec2, Window};
use orb_engine::seq::Event as TrackEvent;

use crate::app::App;
use crate::link::ROW_DEVICES;
use crate::model::BAR;

/// The open pattern automation editor window, if any.
#[derive(Clone, Debug)]
pub struct PatternAutomationEditor {
    /// The pattern section (row) 0..=3.
    pub row: usize,
    /// Human-readable title.
    pub title: String,
    /// Serial to distinguish multiple open windows.
    pub serial: u32,
    /// The first bar of the graph's visible range.
    pub view_start: f32,
    /// The last transport bar seen, used to reveal a bar selected with the transport arrows.
    pub last_song_bar: i32,
    /// Number of bars visible across the graph.
    pub zoom_bars: i32,
    /// The bar last selected in the graph.
    pub selected_bar: Option<i32>,
    /// Whether pointer drags edit points or pan the graph.
    pub tool: GraphTool,
}

/// Pointer behavior in the pattern graph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GraphTool {
    /// Drag the graph to pan through the song.
    Hand,
    /// Click or drag to write pattern selections.
    #[default]
    Edit,
}

const MIN_ZOOM_BARS: i32 = 4;
const MAX_ZOOM_BARS: i32 = 32;
const DEFAULT_ZOOM_BARS: i32 = 16;

/// The device name for a pattern section row.
fn device_name(row: usize) -> &'static str {
    match row {
        0 => "TB-303 I",
        1 => "TB-303 II",
        2 => "TR-808",
        _ => "TR-909",
    }
}

fn auto_scroll_start(start: f32, zoom_bars: i32, playhead: f32, min: i32, max: i32) -> f32 {
    if playhead < start {
        playhead.floor().max(min as f32)
    } else if playhead >= start + zoom_bars as f32 {
        (playhead - zoom_bars as f32 + 1.0).clamp(min as f32, max as f32)
    } else {
        start
    }
}

impl App {
    /// Clears the selected bar's pattern override, or all overrides with Ctrl/Cmd.
    pub(crate) fn reset_pattern_automation(&mut self, ui: &Ui, id: &str) {
        if !self.panel.transport.song_mode {
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
            let changed = self.clear_pattern_automation(row, 1, 999);
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
        if row >= 4 || !(1..=999).contains(&bar) {
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

    /// Removes standard pattern-selection events from the inclusive bar range.
    fn clear_pattern_automation(&mut self, row: usize, start_bar: i32, end_bar: i32) -> bool {
        if row >= 4 || start_bar > end_bar {
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

    /// Opens the pattern automation editor for pattern section `row` (0..=3).
    pub fn open_pattern_automation(&mut self, id: &str) {
        // Extract the row number from an id like `pattern2.pattern5`.
        let row = id
            .strip_prefix("pattern")
            .and_then(|s| s.split('.').next())
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| n.checked_sub(1))
            .filter(|&r| r < 4)
            .unwrap_or(0);

        let serial = self.pattern_automation.as_ref().map_or(1, |e| e.serial + 1);
        self.pattern_automation = Some(PatternAutomationEditor {
            row,
            title: format!("Automation: Pattern — {}", device_name(row)),
            serial,
            view_start: ((self.panel.transport.position / BAR + 1).max(1)) as f32,
            last_song_bar: self.panel.transport.position / BAR,
            zoom_bars: DEFAULT_ZOOM_BARS,
            selected_bar: Some((self.panel.transport.position / BAR) + 1),
            tool: GraphTool::Edit,
        });
    }

    /// Renders the pattern automation editor window if open.
    pub fn pattern_automation_ui(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.pattern_automation.take() else {
            return;
        };
        let mut open = true;
        let win_id = Id::unique(("orb_pattern_automation", editor.serial));

        let default_size = Vec2::new(780.0, 600.0);
        let min_size = Vec2::new(600.0, 460.0);

        Window::new(&editor.title)
            .id(win_id)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_pos(egui::Pos2::new(100.0, 100.0))
            .default_size(default_size)
            .min_size(min_size)
            .show(ctx, |ui| {
                self.pattern_automation_contents(ui, &mut editor);
            });

        if !open {
            self.pattern_automation = None;
        } else {
            self.pattern_automation = Some(editor);
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
            self.transport_bar_control(ui, "pattern_automation");
        });

        ui.horizontal(|ui| {
            ui.label("Zoom");
            ui.add(
                egui::Slider::new(&mut editor.zoom_bars, MIN_ZOOM_BARS..=MAX_ZOOM_BARS)
                    .step_by(1.0)
                    .suffix(" bars"),
            );
            if ui
                .selectable_label(editor.tool == GraphTool::Hand, "☞")
                .on_hover_text("Hand: drag the graph to scroll")
                .clicked()
            {
                editor.tool = GraphTool::Hand;
            }
            if ui
                .selectable_label(editor.tool == GraphTool::Edit, "✎")
                .on_hover_text("Edit pattern points")
                .clicked()
            {
                editor.tool = GraphTool::Edit;
            }
        });
        let mut max_start = (full_end - editor.zoom_bars + 1).max(full_start);
        editor.view_start = editor.view_start.clamp(full_start as f32, max_start as f32);
        let playhead = (playhead_tick.max(0) as f32 / BAR as f32) + 1.0;
        if is_playing || transport_bar != editor.last_song_bar {
            editor.view_start = auto_scroll_start(
                editor.view_start,
                editor.zoom_bars,
                playhead,
                full_start,
                max_start,
            );
        }
        editor.last_song_bar = transport_bar;
        ui.label("Click or drag to choose a pattern (1–32); right-click a bar to clear it.");
        ui.add_space(4.0);
        let graph_height = (ui.available_height() - 95.0).clamp(240.0, 460.0);
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), graph_height),
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
        values
            .entry(1)
            .or_insert(self.panel.rows[row].pattern.saturating_add(1));
        paint_pattern_graph(
            ui,
            rect,
            &values,
            view_start,
            editor.zoom_bars,
            editor.selected_bar,
            is_playing.then_some(playhead),
        );
        if let Some(pos) = response
            .hover_pos()
            .filter(|pos| pattern_plot(rect).contains(*pos))
        {
            if editor.tool == GraphTool::Hand && response.dragged() {
                let delta = ui.input(|input| input.pointer.delta().x);
                editor.view_start = (editor.view_start
                    - delta / pattern_plot(rect).width() * editor.zoom_bars as f32)
                    .clamp(full_start as f32, max_start as f32);
            } else if editor.tool == GraphTool::Edit && response.secondary_clicked() {
                let bar = graph_bar_at(pos, rect, view_start, view_end);
                self.set_pattern_automation_bar(row, bar, None);
                editor.selected_bar = Some(bar);
            } else if editor.tool == GraphTool::Edit && (response.clicked() || response.dragged()) {
                let bar = graph_bar_at(pos, rect, view_start, view_end);
                let value = graph_pattern_at(pos, rect);
                self.set_pattern_automation_bar(row, bar, Some(value));
                editor.selected_bar = Some(bar);
            }
        }

        ui.horizontal(|ui| {
            ui.label("Bars");
            ui.add_enabled(
                max_start > full_start,
                egui::Slider::new(&mut editor.view_start, full_start as f32..=max_start as f32)
                    .step_by(0.05)
                    .text("Start bar"),
            );
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
                self.clear_pattern_automation(row, 1, 999);
            }
        });
    }
}

fn pattern_plot(rect: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(rect.min.x + 40.0, rect.min.y + 10.0),
        Pos2::new(rect.max.x - 10.0, rect.max.y - 28.0),
    )
}

fn graph_bar_at(pos: Pos2, rect: Rect, start: f32, zoom_bars: i32) -> i32 {
    let plot = pattern_plot(rect);
    let fraction = ((pos.x - plot.min.x) / plot.width()).clamp(0.0, 0.999_999);
    (start + fraction * zoom_bars as f32).floor() as i32
}

fn graph_pattern_at(pos: Pos2, rect: Rect) -> u8 {
    let plot = pattern_plot(rect);
    let fraction = ((pos.y - plot.min.y) / plot.height()).clamp(0.0, 1.0);
    (32.0 - (fraction * 31.0).round()).clamp(1.0, 32.0) as u8
}

fn paint_pattern_graph(
    ui: &Ui,
    rect: Rect,
    patterns: &HashMap<i32, u8>,
    start: f32,
    zoom_bars: i32,
    selected_bar: Option<i32>,
    playhead: Option<f32>,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(0x15, 0x1a, 0x21));
    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, Color32::from_rgb(0x45, 0x55, 0x66)),
        egui::StrokeKind::Inside,
    );
    let plot = pattern_plot(rect);
    if plot.width() <= 1.0 || plot.height() <= 1.0 {
        return;
    }
    let bars = zoom_bars.max(1);
    let slot = plot.width() / bars as f32;
    let x_for_bar = |bar: i32| plot.min.x + (bar as f32 + 0.5 - start) * slot;
    let y_for_pattern =
        |pattern: u8| plot.max.y - ((pattern.saturating_sub(1)) as f32 / 31.0) * plot.height();

    if let Some(position) = playhead.filter(|p| *p >= start && *p < start + bars as f32) {
        let x = plot.min.x + (position - start) * slot;
        let color = Color32::from_rgb(0xff, 0x52, 0x52);
        painter.line_segment(
            [Pos2::new(x, plot.min.y), Pos2::new(x, plot.max.y)],
            Stroke::new(2.0, color),
        );
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(x - 5.0, plot.min.y),
                Pos2::new(x + 5.0, plot.min.y),
                Pos2::new(x, plot.min.y + 8.0),
            ],
            color,
            Stroke::NONE,
        ));
    }

    if let Some(bar) = selected_bar
        .filter(|bar| *bar as f32 + 0.5 >= start && *bar as f32 + 0.5 < start + bars as f32)
    {
        let x0 = plot.min.x + (bar as f32 - start) * slot;
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(x0, plot.min.y), Pos2::new(x0 + slot, plot.max.y)),
            0.0,
            Color32::from_rgba_unmultiplied(0xff, 0xb0, 0x00, 18),
        );
    }

    for pattern in 1..=32 {
        let y = y_for_pattern(pattern);
        let major = pattern % 8 == 0;
        painter.line_segment(
            [Pos2::new(plot.min.x, y), Pos2::new(plot.max.x, y)],
            Stroke::new(
                if major { 1.0 } else { 0.5 },
                if major {
                    Color32::from_rgb(0x42, 0x50, 0x60)
                } else {
                    Color32::from_rgb(0x24, 0x2d, 0x38)
                },
            ),
        );
        painter.text(
            Pos2::new(plot.min.x - 4.0, y),
            egui::Align2::RIGHT_CENTER,
            crate::behavior::pattern_label(pattern - 1),
            egui::FontId::monospace(8.0),
            Color32::from_rgb(0x9a, 0xa8, 0xb8),
        );
    }

    let first_boundary = start.ceil() as i32;
    let last_boundary = (start + bars as f32).floor() as i32;
    for bar in first_boundary..=last_boundary {
        let x = plot.min.x + (bar as f32 - start) * slot;
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

    let mut points: Vec<_> = patterns
        .iter()
        .filter(|(bar, _)| **bar as f32 + 0.5 >= start && **bar as f32 + 0.5 < start + bars as f32)
        .map(|(&bar, &pattern)| (bar, pattern.clamp(1, 32)))
        .collect();
    points.sort_by_key(|(bar, _)| *bar);
    for pair in points.windows(2) {
        if pair[1].0 == pair[0].0 + 1 {
            let x1 = x_for_bar(pair[0].0) + slot * 0.5;
            let x2 = x_for_bar(pair[1].0) - slot * 0.5;
            let y1 = y_for_pattern(pair[0].1);
            let y2 = y_for_pattern(pair[1].1);
            painter.line_segment(
                [Pos2::new(x1, y1), Pos2::new(x2, y1)],
                Stroke::new(2.0, Color32::from_rgb(0x38, 0xd0, 0x70)),
            );
            painter.line_segment(
                [Pos2::new(x2, y1), Pos2::new(x2, y2)],
                Stroke::new(2.0, Color32::from_rgb(0x38, 0xd0, 0x70)),
            );
        }
    }
    for (bar, pattern) in points {
        let center = Pos2::new(x_for_bar(bar), y_for_pattern(pattern));
        painter.circle_filled(center, 4.0, Color32::from_rgb(0x56, 0xff, 0x9e));
        painter.circle_stroke(center, 4.0, Stroke::new(1.0, Color32::BLACK));
    }
}

#[cfg(test)]
mod tests {
    use super::{auto_scroll_start, graph_bar_at, graph_pattern_at, pattern_plot};
    use eframe::egui::{Pos2, Rect};

    #[test]
    fn pattern_graph_maps_columns_and_pattern_extremes() {
        let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 360.0));
        let plot = pattern_plot(rect);
        assert_eq!(
            graph_bar_at(Pos2::new(plot.min.x, 100.0), rect, 10.0, 10),
            10
        );
        assert_eq!(
            graph_bar_at(Pos2::new(plot.max.x, 100.0), rect, 10.0, 10),
            19
        );
        assert_eq!(graph_pattern_at(Pos2::new(100.0, plot.min.y), rect), 32);
        assert_eq!(graph_pattern_at(Pos2::new(100.0, plot.max.y), rect), 1);
    }

    #[test]
    fn playhead_scrolls_only_when_it_leaves_the_visible_bars() {
        assert_eq!(auto_scroll_start(1.0, 16, 16.9, 1, 984), 1.0);
        assert_eq!(auto_scroll_start(1.0, 16, 17.0, 1, 984), 2.0);
        assert_eq!(auto_scroll_start(20.0, 16, 19.0, 1, 984), 19.0);
    }
}
