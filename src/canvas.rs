use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::library::{get_footprint_by_id, FootprintShapeStyle};
use crate::model::{
    sample_catmull_rom, AxisLabelStyle, BoardKind, BoardTheme, ConnectorKind, GridPos, Perfboard,
    PlacedConnector, Project, RgbColor, SolderLine, Wire, WireMode, WorldPos,
};

/// Control scheme selectable in the bottom status bar (inspired by KiCad)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlScheme {
    /// KiCad-style Touchpad: two-finger scroll pans in X/Y, pinch or Cmd/Ctrl+scroll zooms at cursor,
    /// Shift+drag or middle-drag also pans.
    Touchpad,
    /// Traditional CAD Mouse: scroll wheel zooms toward cursor, middle-drag or right-drag pans.
    Mouse,
}

impl ControlScheme {
    pub fn display_name(self) -> &'static str {
        match self {
            ControlScheme::Touchpad => "Touchpad (KiCad Pan/Pinch)",
            ControlScheme::Mouse => "Mouse (Wheel Zoom / Mid-Pan)",
        }
    }

    pub fn hint_text(self) -> &'static str {
        match self {
            ControlScheme::Touchpad => {
                "2-Finger Scroll: Pan  |  Pinch / Ctrl+Scroll: Zoom  |  Space+Drag: Pan"
            }
            ControlScheme::Mouse => {
                "Wheel: Zoom at Cursor  |  Middle/Right-Drag or Space+Drag: Pan"
            }
        }
    }
}

/// 2D Camera / Viewport mapping between World coordinates (in 2.54mm grid pitches) and Screen pixels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Viewport {
    /// World coordinate at the center of the canvas
    pub center: WorldPos,
    /// Pixels per grid pitch (default 28.0 px per 2.54mm pitch)
    pub pixels_per_pitch: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            center: WorldPos::new(16.0, 8.5),
            pixels_per_pitch: 27.0,
        }
    }
}

impl Viewport {
    pub fn world_to_screen(&self, canvas_rect: Rect, world: WorldPos) -> Pos2 {
        let center_screen = canvas_rect.center();
        Pos2::new(
            center_screen.x + (world.x - self.center.x) * self.pixels_per_pitch,
            center_screen.y + (world.y - self.center.y) * self.pixels_per_pitch,
        )
    }

    pub fn grid_to_screen(&self, canvas_rect: Rect, grid: GridPos) -> Pos2 {
        self.world_to_screen(canvas_rect, WorldPos::from_grid(grid))
    }

    pub fn screen_to_world(&self, canvas_rect: Rect, screen: Pos2) -> WorldPos {
        let center_screen = canvas_rect.center();
        WorldPos::new(
            self.center.x + (screen.x - center_screen.x) / self.pixels_per_pitch,
            self.center.y + (screen.y - center_screen.y) / self.pixels_per_pitch,
        )
    }

    pub fn zoom_at_screen_pos(&mut self, canvas_rect: Rect, screen_pos: Pos2, zoom_factor: f32) {
        let before = self.screen_to_world(canvas_rect, screen_pos);
        self.pixels_per_pitch = (self.pixels_per_pitch * zoom_factor).clamp(8.0, 110.0);
        let after = self.screen_to_world(canvas_rect, screen_pos);
        self.center.x += before.x - after.x;
        self.center.y += before.y - after.y;
    }

    pub fn pan_by_pixels(&mut self, delta_px: Vec2) {
        self.center.x -= delta_px.x / self.pixels_per_pitch;
        self.center.y -= delta_px.y / self.pixels_per_pitch;
    }

    pub fn fit_project(&mut self, canvas_rect: Rect, project: &Project) {
        if project.boards.is_empty() {
            self.center = WorldPos::new(10.0, 8.0);
            self.pixels_per_pitch = 28.0;
            return;
        }
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for b in &project.boards {
            let (bmin, bmax) = b.world_bounds();
            min_x = min_x.min(bmin.x);
            min_y = min_y.min(bmin.y);
            max_x = max_x.max(bmax.x);
            max_y = max_y.max(bmax.y);
        }
        self.center = WorldPos::new((min_x + max_x) * 0.5, (min_y + max_y) * 0.5);
        let span_x = (max_x - min_x + 4.0).max(8.0);
        let span_y = (max_y - min_y + 4.0).max(8.0);
        let scale_x = canvas_rect.width() / span_x;
        let scale_y = canvas_rect.height() / span_y;
        self.pixels_per_pitch = scale_x.min(scale_y).clamp(12.0, 64.0);
    }
}

/// Currently selected entity on the canvas
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectedItem {
    Board(Uuid),
    Connector(Uuid),
    Wire(Uuid),
    SolderLine(Uuid),
}

/// Preview state painted while placing a new item or drawing a wire/solder trace
pub struct CanvasRenderPreview<'a> {
    pub ghost_connector: Option<&'a PlacedConnector>,
    pub active_wire_in_progress: Option<(GridPos, &'a [WorldPos], WorldPos, WireMode, RgbColor)>,
    pub active_solder_in_progress: Option<&'a [GridPos]>,
    pub hovered_hole: Option<GridPos>,
    pub show_x_ray_blocked_holes: bool,
    pub show_pin_labels: bool,
    pub show_net_highlights: bool,
}

/// Process KiCad-inspired Touchpad / Mouse camera controls on the canvas response
pub fn handle_viewport_navigation(
    ui: &egui::Ui,
    response: &egui::Response,
    canvas_rect: Rect,
    viewport: &mut Viewport,
    scheme: ControlScheme,
    right_drag_available: bool,
) {
    if !response.hovered() && !response.dragged() {
        return;
    }

    let pointer_pos = ui
        .input(|i| i.pointer.hover_pos())
        .unwrap_or_else(|| canvas_rect.center());

    // 1. Pinch-to-zoom (`zoom_delta()`) works in both Touchpad and Mouse modes
    let zoom_delta = ui.input(|i| i.zoom_delta());
    if (zoom_delta - 1.0).abs() > 0.001 {
        viewport.zoom_at_screen_pos(canvas_rect, pointer_pos, zoom_delta);
    }

    // 2. Smooth scroll delta (`smooth_scroll_delta`)
    let scroll = ui.input(|i| i.smooth_scroll_delta);
    let modifiers = ui.input(|i| i.modifiers);

    if scroll.length_sq() > 0.01 {
        match scheme {
            ControlScheme::Touchpad => {
                // KiCad Touchpad behavior:
                // - Cmd/Ctrl + scroll or Alt + scroll zooms
                // - Plain two-finger scroll pans smoothly in 2D
                if modifiers.command || modifiers.ctrl || modifiers.alt {
                    let factor = (scroll.y * 0.008).exp();
                    viewport.zoom_at_screen_pos(canvas_rect, pointer_pos, factor);
                } else if modifiers.shift {
                    // Shift + vertical scroll pans horizontally
                    viewport.pan_by_pixels(egui::vec2(scroll.y + scroll.x, 0.0));
                } else {
                    viewport.pan_by_pixels(scroll);
                }
            }
            ControlScheme::Mouse => {
                // Traditional CAD Mouse behavior:
                // - Scroll wheel zooms toward cursor
                // - Shift + scroll pans horizontally, Ctrl + scroll pans vertically
                if modifiers.shift {
                    viewport.pan_by_pixels(egui::vec2(scroll.y, 0.0));
                } else if modifiers.ctrl {
                    viewport.pan_by_pixels(egui::vec2(0.0, scroll.y));
                } else {
                    let factor = (scroll.y * 0.0065).exp();
                    viewport.zoom_at_screen_pos(canvas_rect, pointer_pos, factor);
                }
            }
        }
    }

    // 3. Middle-mouse drag, Space + Left drag, or Right-mouse drag (when not canceling a tool) pans
    let space_down = ui.input(|i| i.key_down(egui::Key::Space));
    let middle_drag = response.dragged_by(egui::PointerButton::Middle);
    let right_drag = right_drag_available && response.dragged_by(egui::PointerButton::Secondary);
    let space_left_drag = space_down && response.dragged_by(egui::PointerButton::Primary);

    if middle_drag || right_drag || space_left_drag {
        let delta = response.drag_delta();
        viewport.pan_by_pixels(delta);
    }
}

/// Render the entire 2D CAD workspace (background grid, perfboards, pads, stripboard tracks,
/// solder lines, connectors/ICs/JST-XH footprints, wires, and interactive overlays)
pub fn render_workspace(
    painter: &egui::Painter,
    canvas_rect: Rect,
    viewport: &Viewport,
    project: &Project,
    selected: &Option<SelectedItem>,
    preview: &CanvasRenderPreview<'_>,
) {
    // 1. Dark Fusion/KiCad-inspired background with subtle vignette/grid
    painter.rect_filled(canvas_rect, 0.0, Color32::from_rgb(20, 22, 27));
    draw_world_grid(painter, canvas_rect, viewport);

    // 2. Render each Perfboard / Stripboard / Breadboard
    for board in &project.boards {
        let is_selected = matches!(selected, Some(SelectedItem::Board(id)) if *id == board.id);
        draw_board(painter, canvas_rect, viewport, project, board, is_selected);
    }

    // 3. Render blocked overhang indicators if enabled or while placing a component
    if preview.show_x_ray_blocked_holes || preview.ghost_connector.is_some() {
        draw_blocked_hole_markers(painter, canvas_rect, viewport, project);
    }

    // 4. Render Solder Lines (metallic solder traces spanning adjacent holes)
    for solder in &project.solder_lines {
        let is_selected = matches!(selected, Some(SelectedItem::SolderLine(id)) if *id == solder.id);
        draw_solder_line(painter, canvas_rect, viewport, solder, is_selected, false);
    }

    // Active solder line preview
    if let Some(nodes) = preview.active_solder_in_progress {
        if !nodes.is_empty() {
            let temp = SolderLine::new(nodes.to_vec());
            draw_solder_line(painter, canvas_rect, viewport, &temp, true, true);
        }
    }

    // 5. Render Bottom-layer wires before top components
    for wire in project
        .wires
        .iter()
        .filter(|w| w.layer == crate::model::WireLayer::BottomSolder)
    {
        let is_selected = matches!(selected, Some(SelectedItem::Wire(id)) if *id == wire.id);
        draw_wire(painter, canvas_rect, viewport, wire, is_selected);
    }

    // 6. Render Placed Connectors, Headers, and Library Components
    for conn in &project.connectors {
        let is_selected = matches!(selected, Some(SelectedItem::Connector(id)) if *id == conn.id);
        draw_connector(
            painter,
            canvas_rect,
            viewport,
            conn,
            is_selected,
            false,
            true,
            preview.show_pin_labels,
        );
    }

    // 7. Render Top-layer wires
    for wire in project
        .wires
        .iter()
        .filter(|w| w.layer == crate::model::WireLayer::TopComponent)
    {
        let is_selected = matches!(selected, Some(SelectedItem::Wire(id)) if *id == wire.id);
        draw_wire(painter, canvas_rect, viewport, wire, is_selected);
    }

    // 8. Render Active Wire in Progress
    if let Some((start_hole, waypoints, cursor_world, mode, color)) = preview.active_wire_in_progress
    {
        draw_wire_in_progress(
            painter,
            canvas_rect,
            viewport,
            start_hole,
            waypoints,
            cursor_world,
            mode,
            color,
        );
    }

    // 9. Render Ghost Connector Preview (with live DRC validity check)
    if let Some(ghost) = preview.ghost_connector {
        let validation = project.validate_connector_placement(ghost, None);
        let is_valid = validation.is_ok();
        draw_connector(
            painter,
            canvas_rect,
            viewport,
            ghost,
            false,
            true,
            is_valid,
            preview.show_pin_labels,
        );
        if let Err(conflicts) = validation {
            for (pos, _reason) in conflicts {
                let p = viewport.grid_to_screen(canvas_rect, pos);
                let r = viewport.pixels_per_pitch * 0.44;
                painter.circle_stroke(p, r, Stroke::new(2.2_f32, Color32::from_rgb(255, 78, 78)));
                painter.line_segment(
                    [p + egui::vec2(-r * 0.6, -r * 0.6), p + egui::vec2(r * 0.6, r * 0.6)],
                    Stroke::new(2.2_f32, Color32::from_rgb(255, 78, 78)),
                );
                painter.line_segment(
                    [p + egui::vec2(r * 0.6, -r * 0.6), p + egui::vec2(-r * 0.6, r * 0.6)],
                    Stroke::new(2.2_f32, Color32::from_rgb(255, 78, 78)),
                );
            }
        }
    }

    // 10. Render Hovered Hole Crosshair Reticle
    if let Some(hole) = preview.hovered_hole {
        let p = viewport.grid_to_screen(canvas_rect, hole);
        let r = (viewport.pixels_per_pitch * 0.42).max(4.0);
        painter.circle_stroke(
            p,
            r,
            Stroke::new(1.4_f32, Color32::from_rgba_unmultiplied(96, 200, 255, 190)),
        );
    }
}

fn draw_world_grid(painter: &egui::Painter, canvas_rect: Rect, viewport: &Viewport) {
    let ppp = viewport.pixels_per_pitch;
    let top_left = viewport.screen_to_world(canvas_rect, canvas_rect.min);
    let bottom_right = viewport.screen_to_world(canvas_rect, canvas_rect.max);

    let min_col = top_left.x.floor() as i32 - 1;
    let max_col = bottom_right.x.ceil() as i32 + 1;
    let min_row = top_left.y.floor() as i32 - 1;
    let max_row = bottom_right.y.ceil() as i32 + 1;

    let major_stroke = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(54, 60, 74, 110));
    let minor_stroke = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(34, 38, 48, 95));
    let origin_x_stroke = Stroke::new(1.2_f32, Color32::from_rgba_unmultiplied(180, 68, 68, 120));
    let origin_y_stroke = Stroke::new(1.2_f32, Color32::from_rgba_unmultiplied(68, 170, 96, 120));

    for col in min_col..=max_col {
        let x = viewport
            .world_to_screen(canvas_rect, WorldPos::new(col as f32, 0.0))
            .x;
        if col == 0 {
            painter.line_segment(
                [Pos2::new(x, canvas_rect.min.y), Pos2::new(x, canvas_rect.max.y)],
                origin_y_stroke,
            );
        } else if col % 5 == 0 {
            painter.line_segment(
                [Pos2::new(x, canvas_rect.min.y), Pos2::new(x, canvas_rect.max.y)],
                major_stroke,
            );
        } else if ppp >= 15.0 {
            painter.line_segment(
                [Pos2::new(x, canvas_rect.min.y), Pos2::new(x, canvas_rect.max.y)],
                minor_stroke,
            );
        }
    }

    for row in min_row..=max_row {
        let y = viewport
            .world_to_screen(canvas_rect, WorldPos::new(0.0, row as f32))
            .y;
        if row == 0 {
            painter.line_segment(
                [Pos2::new(canvas_rect.min.x, y), Pos2::new(canvas_rect.max.x, y)],
                origin_x_stroke,
            );
        } else if row % 5 == 0 {
            painter.line_segment(
                [Pos2::new(canvas_rect.min.x, y), Pos2::new(canvas_rect.max.x, y)],
                major_stroke,
            );
        } else if ppp >= 15.0 {
            painter.line_segment(
                [Pos2::new(canvas_rect.min.x, y), Pos2::new(canvas_rect.max.x, y)],
                minor_stroke,
            );
        }
    }
}

fn draw_board(
    painter: &egui::Painter,
    canvas_rect: Rect,
    viewport: &Viewport,
    project: &Project,
    board: &Perfboard,
    is_selected: bool,
) {
    let ppp = viewport.pixels_per_pitch;
    let (bmin, bmax) = board.world_bounds();
    let p_min = viewport.world_to_screen(canvas_rect, bmin);
    let p_max = viewport.world_to_screen(canvas_rect, bmax);
    let board_rect = Rect::from_min_max(p_min, p_max);

    // Quick frustum cull
    if !canvas_rect.intersects(board_rect.expand(40.0)) {
        return;
    }

    let rounding = (ppp * 0.32).clamp(3.0, 14.0);

    // Drop shadow
    let shadow_offset = egui::vec2(ppp * 0.16, ppp * 0.22);
    painter.rect_filled(
        board_rect.translate(shadow_offset),
        rounding,
        Color32::from_rgba_unmultiplied(0, 0, 0, 115),
    );

    // Board substrate body
    painter.rect_filled(board_rect, rounding, board.theme.substrate_fill());

    // Inner bevel & selection border
    let border_stroke = if is_selected {
        Stroke::new(2.5_f32, Color32::from_rgb(76, 194, 255))
    } else {
        Stroke::new(1.6_f32, board.theme.substrate_border())
    };
    painter.rect_stroke(board_rect, rounding, border_stroke, egui::StrokeKind::Outside);

    // Board title above top-left corner
    if ppp >= 11.0 {
        let title_pos = Pos2::new(board_rect.min.x + 4.0, board_rect.min.y - 16.0);
        let subtitle = format!("{} ({}×{})", board.name, board.cols, board.rows);
        painter.text(
            title_pos,
            egui::Align2::LEFT_BOTTOM,
            subtitle,
            egui::FontId::proportional(12.5),
            if is_selected {
                Color32::from_rgb(110, 210, 255)
            } else {
                Color32::from_rgb(168, 178, 195)
            },
        );
    }

    // Optional corner mounting holes
    if board.mounting_holes && board.cols >= 6 && board.rows >= 6 {
        let mount_offset = 0.68;
        let corners = [
            WorldPos::new(
                board.origin.col as f32 - mount_offset,
                board.origin.row as f32 - mount_offset,
            ),
            WorldPos::new(
                (board.origin.col + board.cols as i32 - 1) as f32 + mount_offset,
                board.origin.row as f32 - mount_offset,
            ),
            WorldPos::new(
                board.origin.col as f32 - mount_offset,
                (board.origin.row + board.rows as i32 - 1) as f32 + mount_offset,
            ),
            WorldPos::new(
                (board.origin.col + board.cols as i32 - 1) as f32 + mount_offset,
                (board.origin.row + board.rows as i32 - 1) as f32 + mount_offset,
            ),
        ];
        for mc in corners {
            let sp = viewport.world_to_screen(canvas_rect, mc);
            painter.circle_filled(sp, ppp * 0.24, Color32::from_rgb(175, 165, 130));
            painter.circle_filled(sp, ppp * 0.15, Color32::from_rgb(20, 22, 27));
        }
    }

    // Draw Stripboard / Breadboard underlying conductive strips
    match board.kind {
        BoardKind::Perfboard => {}
        BoardKind::StripboardHorizontal => {
            let strip_color = Color32::from_rgba_unmultiplied(198, 146, 74, 155);
            let half_h = ppp * 0.27;
            for r in 0..(board.rows as i32) {
                let p_start = viewport
                    .grid_to_screen(canvas_rect, board.local_to_global(GridPos::new(0, r)));
                let p_end = viewport.grid_to_screen(
                    canvas_rect,
                    board.local_to_global(GridPos::new(board.cols as i32 - 1, r)),
                );
                let rect = Rect::from_min_max(
                    Pos2::new(p_start.x - ppp * 0.34, p_start.y - half_h),
                    Pos2::new(p_end.x + ppp * 0.34, p_end.y + half_h),
                );
                painter.rect_filled(rect, half_h * 0.6, strip_color);
            }
        }
        BoardKind::StripboardVertical => {
            let strip_color = Color32::from_rgba_unmultiplied(198, 146, 74, 155);
            let half_w = ppp * 0.27;
            for c in 0..(board.cols as i32) {
                let p_start = viewport
                    .grid_to_screen(canvas_rect, board.local_to_global(GridPos::new(c, 0)));
                let p_end = viewport.grid_to_screen(
                    canvas_rect,
                    board.local_to_global(GridPos::new(c, board.rows as i32 - 1)),
                );
                let rect = Rect::from_min_max(
                    Pos2::new(p_start.x - half_w, p_start.y - ppp * 0.34),
                    Pos2::new(p_end.x + half_w, p_end.y + ppp * 0.34),
                );
                painter.rect_filled(rect, half_w * 0.6, strip_color);
            }
        }
        BoardKind::Breadboard => {
            // Draw 5-hole vertical terminal strip groupings and center IC trench
            let strip_color = Color32::from_rgba_unmultiplied(160, 172, 185, 110);
            let mid_row = (board.rows as i32) / 2;
            // Center trench visual groove
            if board.rows >= 6 {
                let trench_y = viewport
                    .world_to_screen(
                        canvas_rect,
                        WorldPos::new(0.0, (board.origin.row + mid_row) as f32 - 0.5),
                    )
                    .y;
                let trench_rect = Rect::from_min_max(
                    Pos2::new(board_rect.min.x + ppp * 0.3, trench_y - ppp * 0.25),
                    Pos2::new(board_rect.max.x - ppp * 0.3, trench_y + ppp * 0.25),
                );
                painter.rect_filled(
                    trench_rect,
                    2.0,
                    Color32::from_rgba_unmultiplied(0, 0, 0, 50),
                );
            }
            for c in 0..(board.cols as i32) {
                let top_end = (mid_row - 1).max(0);
                if top_end > 0 {
                    let p0 = viewport
                        .grid_to_screen(canvas_rect, board.local_to_global(GridPos::new(c, 0)));
                    let p1 = viewport.grid_to_screen(
                        canvas_rect,
                        board.local_to_global(GridPos::new(c, top_end)),
                    );
                    painter.line_segment([p0, p1], Stroke::new(ppp * 0.36, strip_color));
                }
                if mid_row < board.rows as i32 - 1 {
                    let p0 = viewport.grid_to_screen(
                        canvas_rect,
                        board.local_to_global(GridPos::new(c, mid_row)),
                    );
                    let p1 = viewport.grid_to_screen(
                        canvas_rect,
                        board.local_to_global(GridPos::new(c, board.rows as i32 - 1)),
                    );
                    painter.line_segment([p0, p1], Stroke::new(ppp * 0.36, strip_color));
                }
            }
        }
    }

    // Draw pads and drill holes
    let pad_radius = (ppp * 0.34).max(2.0);
    let drill_radius = (ppp * 0.165).max(1.0);
    let copper_color = match board.theme {
        BoardTheme::BreadboardWhite => Color32::from_rgb(95, 102, 112),
        _ => Color32::from_rgb(206, 162, 84),
    };
    let drill_color = match board.theme {
        BoardTheme::BreadboardWhite => Color32::from_rgb(42, 46, 52),
        _ => Color32::from_rgb(18, 20, 24),
    };

    for r in 0..(board.rows as i32) {
        for c in 0..(board.cols as i32) {
            let local = GridPos::new(c, r);
            let global = board.local_to_global(local);
            let center = viewport.grid_to_screen(canvas_rect, global);

            if !canvas_rect.expand(pad_radius * 2.0).contains(center) {
                continue;
            }

            if board.kind == BoardKind::Breadboard {
                let sq = Rect::from_center_size(center, Vec2::splat(drill_radius * 2.3));
                painter.rect_filled(sq, 1.5, copper_color);
                let inner = Rect::from_center_size(center, Vec2::splat(drill_radius * 1.4));
                painter.rect_filled(inner, 1.0, drill_color);
            } else {
                painter.circle_filled(center, pad_radius, copper_color);
                painter.circle_filled(center, drill_radius, drill_color);
            }

            // Track cut on stripboard
            if board.track_cuts.contains(&local) {
                let cut_r = pad_radius * 1.05;
                painter.circle_filled(center, cut_r * 0.78, board.theme.substrate_fill());
                painter.line_segment(
                    [
                        center + egui::vec2(-cut_r, -cut_r),
                        center + egui::vec2(cut_r, cut_r),
                    ],
                    Stroke::new(2.0_f32, Color32::from_rgb(245, 85, 85)),
                );
                painter.line_segment(
                    [
                        center + egui::vec2(cut_r, -cut_r),
                        center + egui::vec2(-cut_r, cut_r),
                    ],
                    Stroke::new(2.0_f32, Color32::from_rgb(245, 85, 85)),
                );
            }
        }
    }

    // Draw row & column coordinate identifiers along board edges
    if ppp >= 13.0 {
        let font_size = (ppp * 0.37).clamp(8.0, 14.0);
        let font_id = egui::FontId::monospace(font_size);
        let label_color = board.theme.label_color();

        // Column identifiers (Top & optionally Bottom)
        if board.col_label_style != AxisLabelStyle::Hidden {
            for c in 0..board.cols {
                let text = board.col_label_style.format_index(c);
                let top_pos = viewport.world_to_screen(
                    canvas_rect,
                    WorldPos::new((board.origin.col + c as i32) as f32, board.origin.row as f32 - 0.72),
                );
                painter.text(
                    top_pos,
                    egui::Align2::CENTER_CENTER,
                    &text,
                    font_id.clone(),
                    label_color,
                );

                if board.labels_both_sides {
                    let bot_pos = viewport.world_to_screen(
                        canvas_rect,
                        WorldPos::new(
                            (board.origin.col + c as i32) as f32,
                            (board.origin.row + board.rows as i32 - 1) as f32 + 0.72,
                        ),
                    );
                    painter.text(
                        bot_pos,
                        egui::Align2::CENTER_CENTER,
                        &text,
                        font_id.clone(),
                        label_color,
                    );
                }
            }
        }

        // Row identifiers (Left & optionally Right)
        if board.row_label_style != AxisLabelStyle::Hidden {
            for r in 0..board.rows {
                let text = board.row_label_style.format_index(r);
                let left_pos = viewport.world_to_screen(
                    canvas_rect,
                    WorldPos::new(board.origin.col as f32 - 0.74, (board.origin.row + r as i32) as f32),
                );
                painter.text(
                    left_pos,
                    egui::Align2::CENTER_CENTER,
                    &text,
                    font_id.clone(),
                    label_color,
                );

                if board.labels_both_sides {
                    let right_pos = viewport.world_to_screen(
                        canvas_rect,
                        WorldPos::new(
                            (board.origin.col + board.cols as i32 - 1) as f32 + 0.74,
                            (board.origin.row + r as i32) as f32,
                        ),
                    );
                    painter.text(
                        right_pos,
                        egui::Align2::CENTER_CENTER,
                        &text,
                        font_id.clone(),
                        label_color,
                    );
                }
            }
        }
    }

    let _ = project;
}

fn draw_blocked_hole_markers(
    painter: &egui::Painter,
    canvas_rect: Rect,
    viewport: &Viewport,
    project: &Project,
) {
    let ppp = viewport.pixels_per_pitch;
    let r = (ppp * 0.25).max(3.0);
    let stroke = Stroke::new(1.6_f32, Color32::from_rgba_unmultiplied(255, 120, 70, 200));

    for conn in &project.connectors {
        for hole in conn.blocked_overhang_holes() {
            let p = viewport.grid_to_screen(canvas_rect, hole);
            painter.circle_filled(
                p,
                r * 1.15,
                Color32::from_rgba_unmultiplied(255, 90, 50, 45),
            );
            painter.line_segment([p + egui::vec2(-r, -r), p + egui::vec2(r, r)], stroke);
            painter.line_segment([p + egui::vec2(r, -r), p + egui::vec2(-r, r)], stroke);
        }
    }
}

fn draw_solder_line(
    painter: &egui::Painter,
    canvas_rect: Rect,
    viewport: &Viewport,
    solder: &SolderLine,
    is_selected: bool,
    is_preview: bool,
) {
    if solder.nodes.is_empty() {
        return;
    }
    let ppp = viewport.pixels_per_pitch;
    let base_width = (ppp * 0.56 * solder.width_scale).max(4.0);
    let highlight_width = (base_width * 0.45).max(2.0);

    let outer_color = if is_preview {
        Color32::from_rgba_unmultiplied(195, 208, 222, 185)
    } else if is_selected {
        Color32::from_rgb(130, 215, 255)
    } else {
        Color32::from_rgb(172, 182, 194)
    };

    let sheen_color = if is_preview {
        Color32::from_rgba_unmultiplied(238, 245, 255, 200)
    } else {
        Color32::from_rgb(225, 232, 240)
    };

    let screen_pts: Vec<Pos2> = solder
        .nodes
        .iter()
        .map(|&g| viewport.grid_to_screen(canvas_rect, g))
        .collect();

    // Draw thick metallic segments
    for pair in screen_pts.windows(2) {
        painter.line_segment([pair[0], pair[1]], Stroke::new(base_width, outer_color));
        painter.line_segment(
            [pair[0], pair[1]],
            Stroke::new(highlight_width, sheen_color),
        );
    }

    // Draw rounded solder joints at every node
    for &p in &screen_pts {
        painter.circle_filled(p, base_width * 0.52, outer_color);
        painter.circle_filled(p, highlight_width * 0.58, sheen_color);
        // Subtle flux/pin center dimple
        painter.circle_filled(
            p,
            (ppp * 0.10).max(1.2),
            Color32::from_rgba_unmultiplied(90, 98, 110, 180),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_connector(
    painter: &egui::Painter,
    canvas_rect: Rect,
    viewport: &Viewport,
    conn: &PlacedConnector,
    is_selected: bool,
    is_ghost: bool,
    is_valid: bool,
    show_pin_labels: bool,
) {
    let ppp = viewport.pixels_per_pitch;
    let alpha = if is_ghost { 185 } else { 255 };

    let (aabb_min, aabb_max) = conn.world_aabb();
    let p_min = viewport.world_to_screen(canvas_rect, aabb_min);
    let p_max = viewport.world_to_screen(canvas_rect, aabb_max);
    let body_rect = Rect::from_min_max(p_min, p_max);

    if !canvas_rect.intersects(body_rect.expand(30.0)) {
        return;
    }

    let fill_color = if is_ghost && !is_valid {
        Color32::from_rgba_unmultiplied(190, 45, 45, 160)
    } else {
        conn.body_color.with_alpha(alpha)
    };

    let border_color = if is_ghost && !is_valid {
        Color32::from_rgb(255, 85, 85)
    } else if is_selected {
        Color32::from_rgb(82, 202, 255)
    } else {
        Color32::from_rgba_unmultiplied(20, 22, 26, alpha)
    };

    // Transform helper from component-local pitch coordinates to screen coordinates
    let to_screen = |lx: f32, ly: f32| -> Pos2 {
        let (rx, ry) = conn.rotation.rotate_vec2(lx, ly);
        viewport.world_to_screen(
            canvas_rect,
            WorldPos::new(conn.anchor.col as f32 + rx, conn.anchor.row as f32 + ry),
        )
    };

    match &conn.kind {
        ConnectorKind::SinglePin { square_pad } => {
            let center = to_screen(0.0, 0.0);
            let outer = Vec2::splat(ppp * 0.76);
            let r = Rect::from_center_size(center, outer);
            painter.rect_filled(r, ppp * 0.14, Color32::from_rgba_unmultiplied(36, 40, 48, alpha));
            painter.rect_stroke(
                r,
                ppp * 0.14,
                Stroke::new(if is_selected { 2.0_f32 } else { 1.2_f32 }, border_color),
                egui::StrokeKind::Outside,
            );
            draw_pin_post(painter, center, ppp, *square_pad, alpha);
        }

        ConnectorKind::HeaderLine { cols, rows, female } => {
            let rounding = (ppp * 0.14).clamp(2.0, 6.0);
            painter.rect_filled(body_rect, rounding, fill_color);
            painter.rect_stroke(
                body_rect,
                rounding,
                Stroke::new(if is_selected { 2.2_f32 } else { 1.4_f32 }, border_color),
                egui::StrokeKind::Outside,
            );

            for r in 0..(*rows as i32) {
                for c in 0..(*cols as i32) {
                    let p = to_screen(c as f32, r as f32);
                    let is_pin1 = r == 0 && c == 0;
                    if *female {
                        let sock = Rect::from_center_size(p, Vec2::splat(ppp * 0.44));
                        painter.rect_filled(
                            sock,
                            1.5,
                            Color32::from_rgba_unmultiplied(18, 20, 24, alpha),
                        );
                        painter.rect_stroke(
                            sock,
                            1.5,
                            Stroke::new(1.1_f32, Color32::from_rgba_unmultiplied(215, 178, 72, alpha)),
                            egui::StrokeKind::Inside,
                        );
                    } else {
                        draw_pin_post(painter, p, ppp, is_pin1, alpha);
                    }
                }
            }
        }

        ConnectorKind::LibraryFootprint { footprint_id } => {
            if let Some(fp) = get_footprint_by_id(footprint_id) {
                let rounding = (ppp * 0.15).clamp(2.0, 7.0);
                painter.rect_filled(body_rect, rounding, fill_color);
                painter.rect_stroke(
                    body_rect,
                    rounding,
                    Stroke::new(if is_selected { 2.2_f32 } else { 1.5_f32 }, border_color),
                    egui::StrokeKind::Outside,
                );

                match fp.shape_style {
                    FootprintShapeStyle::JstXhShrouded => {
                        // Draw shrouded walls, polarizing slots, and side wings covering (-1, 0) and (N, 0)
                        let inner_min = to_screen(fp.body_min.0 + 0.22, fp.body_min.1 + 0.22);
                        let inner_max = to_screen(fp.body_max.0 - 0.22, fp.body_max.1 - 0.34);
                        let cavity = Rect::from_two_pos(inner_min, inner_max);
                        painter.rect_filled(
                            cavity,
                            2.0,
                            Color32::from_rgba_unmultiplied(205, 202, 188, alpha),
                        );
                        painter.rect_stroke(
                            cavity,
                            2.0,
                            Stroke::new(1.2_f32, Color32::from_rgba_unmultiplied(145, 140, 128, alpha)),
                            egui::StrokeKind::Inside,
                        );

                        // Latch boss along the bottom/front wall
                        let latch_a = to_screen(0.15, fp.body_max.1 - 0.30);
                        let latch_b = to_screen(
                            (fp.pins.len() as f32 - 1.0) - 0.15,
                            fp.body_max.1 - 0.04,
                        );
                        painter.rect_filled(
                            Rect::from_two_pos(latch_a, latch_b),
                            1.5,
                            Color32::from_rgba_unmultiplied(178, 174, 158, alpha),
                        );

                        // Highlight the blocked side-overhang holes on the housing
                        for &(bc, br) in &fp.blocked_offsets {
                            let bp = to_screen(bc as f32, br as f32);
                            let r = ppp * 0.22;
                            painter.circle_stroke(
                                bp,
                                r,
                                Stroke::new(
                                    1.3_f32,
                                    Color32::from_rgba_unmultiplied(210, 80, 60, alpha),
                                ),
                            );
                            painter.line_segment(
                                [bp + egui::vec2(-r * 0.7, -r * 0.7), bp + egui::vec2(r * 0.7, r * 0.7)],
                                Stroke::new(
                                    1.3_f32,
                                    Color32::from_rgba_unmultiplied(210, 80, 60, alpha),
                                ),
                            );
                        }
                    }

                    FootprintShapeStyle::ScrewTerminalBlock => {
                        // Draw slotted screw heads at each pin
                        for pin in &fp.pins {
                            let p = to_screen(pin.dcol as f32, pin.drow as f32);
                            let sr = ppp * 0.56;
                            painter.circle_filled(
                                p,
                                sr,
                                Color32::from_rgba_unmultiplied(185, 192, 202, alpha),
                            );
                            painter.circle_stroke(
                                p,
                                sr,
                                Stroke::new(1.2_f32, Color32::from_rgba_unmultiplied(70, 76, 84, alpha)),
                            );
                            painter.line_segment(
                                [
                                    p + egui::vec2(-sr * 0.65, -sr * 0.65),
                                    p + egui::vec2(sr * 0.65, sr * 0.65),
                                ],
                                Stroke::new(2.0_f32, Color32::from_rgba_unmultiplied(65, 70, 78, alpha)),
                            );
                        }
                    }

                    FootprintShapeStyle::DipSocket => {
                        // Draw center cavity and top Pin-1 half-moon notch
                        let notch_center = to_screen(1.5, -0.45);
                        painter.circle_filled(
                            notch_center,
                            ppp * 0.32,
                            Color32::from_rgba_unmultiplied(22, 24, 28, alpha),
                        );
                        let pin1_dot = to_screen(0.55, 0.0);
                        painter.circle_filled(
                            pin1_dot,
                            ppp * 0.14,
                            Color32::from_rgba_unmultiplied(220, 224, 230, alpha),
                        );
                    }

                    FootprintShapeStyle::AxialComponent => {
                        let p0 = to_screen(0.0, 0.0);
                        let p1 = to_screen(4.0, 0.0);
                        painter.line_segment(
                            [p0, p1],
                            Stroke::new(ppp * 0.18, Color32::from_rgba_unmultiplied(190, 198, 210, alpha)),
                        );
                        let body_a = to_screen(0.75, -0.42);
                        let body_b = to_screen(3.25, 0.42);
                        let axial_rect = Rect::from_two_pos(body_a, body_b);
                        painter.rect_filled(axial_rect, ppp * 0.2, fill_color);
                        painter.rect_stroke(
                            axial_rect,
                            ppp * 0.2,
                            Stroke::new(1.3_f32, border_color),
                            egui::StrokeKind::Outside,
                        );
                        // Color bands
                        for (bx, col) in [
                            (1.2, Color32::from_rgb(165, 72, 42)),
                            (1.7, Color32::from_rgb(30, 30, 30)),
                            (2.2, Color32::from_rgb(200, 42, 42)),
                            (2.85, Color32::from_rgb(212, 175, 55)),
                        ] {
                            let ba = to_screen(bx, -0.40);
                            let bb = to_screen(bx + 0.18, 0.40);
                            painter.rect_filled(Rect::from_two_pos(ba, bb), 1.0, col);
                        }
                    }

                    FootprintShapeStyle::To220Vertical => {
                        let tab_a = to_screen(-0.90, -0.82);
                        let tab_b = to_screen(2.90, -0.42);
                        painter.rect_filled(
                            Rect::from_two_pos(tab_a, tab_b),
                            2.0,
                            Color32::from_rgba_unmultiplied(185, 195, 208, alpha),
                        );
                    }

                    FootprintShapeStyle::TactileSwitch => {
                        let btn_center = to_screen(1.0, 1.0);
                        painter.circle_filled(
                            btn_center,
                            ppp * 0.62,
                            Color32::from_rgba_unmultiplied(28, 30, 35, alpha),
                        );
                        painter.circle_stroke(
                            btn_center,
                            ppp * 0.62,
                            Stroke::new(1.4_f32, Color32::from_rgba_unmultiplied(140, 148, 160, alpha)),
                        );
                    }
                }

                // Draw all electrical pins
                for pin in &fp.pins {
                    let p = to_screen(pin.dcol as f32, pin.drow as f32);
                    draw_pin_post(painter, p, ppp, pin.square, alpha);
                }
            }
        }
    }

    // Draw pin labels and component designator
    if ppp >= 16.0 && show_pin_labels {
        for (idx, (pin_pos, pin_name)) in conn.pin_positions().into_iter().enumerate() {
            let p = viewport.grid_to_screen(canvas_rect, pin_pos);
            let offset = egui::vec2(0.0, -ppp * 0.34);
            painter.text(
                p + offset,
                egui::Align2::CENTER_BOTTOM,
                &pin_name,
                egui::FontId::monospace((ppp * 0.28).clamp(7.5, 11.5)),
                if idx == 0 {
                    Color32::from_rgba_unmultiplied(255, 225, 130, alpha)
                } else {
                    Color32::from_rgba_unmultiplied(230, 236, 245, alpha)
                },
            );
        }
    }

    if ppp >= 12.0 && !conn.designator.is_empty() {
        let label_text = if conn.label.is_empty() {
            conn.designator.clone()
        } else {
            format!("{} ({})", conn.designator, conn.label)
        };
        let label_pos = Pos2::new(body_rect.center().x, body_rect.min.y - 3.0);
        painter.text(
            label_pos,
            egui::Align2::CENTER_BOTTOM,
            label_text,
            egui::FontId::proportional((ppp * 0.36).clamp(9.0, 13.0)),
            if is_selected {
                Color32::from_rgb(110, 215, 255)
            } else {
                Color32::from_rgba_unmultiplied(235, 240, 248, alpha)
            },
        );
    }
}

fn draw_pin_post(painter: &egui::Painter, center: Pos2, ppp: f32, square: bool, alpha: u8) {
    let gold_outer = Color32::from_rgba_unmultiplied(232, 190, 74, alpha);
    let gold_inner = Color32::from_rgba_unmultiplied(255, 228, 132, alpha);
    let r = (ppp * 0.22).max(2.5);

    if square {
        let sq = Rect::from_center_size(center, Vec2::splat(r * 2.1));
        painter.rect_filled(sq, 1.5, gold_outer);
    } else {
        painter.circle_filled(center, r, gold_outer);
    }
    let post = Rect::from_center_size(center, Vec2::splat((r * 0.95).max(2.0)));
    painter.rect_filled(post, 1.0, gold_inner);
}

fn draw_wire(
    painter: &egui::Painter,
    canvas_rect: Rect,
    viewport: &Viewport,
    wire: &Wire,
    is_selected: bool,
) {
    let ppp = viewport.pixels_per_pitch;
    let pts_world = wire.sampled_points();
    let pts_screen: Vec<Pos2> = pts_world
        .iter()
        .map(|&w| viewport.world_to_screen(canvas_rect, w))
        .collect();

    let width = (ppp * 0.24 * wire.thickness).clamp(2.2, 10.0);
    let jacket_color = wire.color.to_color32();

    // Subtle shadow / outline for contrast over board & pads
    let shadow_stroke = Stroke::new(
        width + 2.4,
        if is_selected {
            Color32::from_rgb(255, 255, 255)
        } else {
            Color32::from_rgba_unmultiplied(12, 14, 18, 190)
        },
    );
    painter.add(egui::Shape::line(pts_screen.clone(), shadow_stroke));

    // Main insulated wire jacket
    let wire_stroke = if wire.layer == crate::model::WireLayer::BottomSolder {
        Stroke::new(width * 0.9, wire.color.with_alpha(200))
    } else {
        Stroke::new(width, jacket_color)
    };
    painter.add(egui::Shape::line(pts_screen.clone(), wire_stroke));

    // Stripped metallic wire ends at start and end holes
    if let (Some(&first), Some(&last)) = (pts_screen.first(), pts_screen.last()) {
        let lug_r = (width * 0.72).max(3.0);
        for ep in [first, last] {
            painter.circle_filled(ep, lug_r, Color32::from_rgb(210, 218, 228));
            painter.circle_filled(ep, lug_r * 0.58, jacket_color);
        }
    }

    // Show intermediate waypoint handles when wire is selected
    if is_selected {
        for wp in &wire.waypoints {
            let sp = viewport.world_to_screen(canvas_rect, *wp);
            painter.circle_filled(sp, 4.5, Color32::WHITE);
            painter.circle_stroke(sp, 4.5, Stroke::new(1.5_f32, jacket_color));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_wire_in_progress(
    painter: &egui::Painter,
    canvas_rect: Rect,
    viewport: &Viewport,
    start_hole: GridPos,
    waypoints: &[WorldPos],
    cursor_world: WorldPos,
    mode: WireMode,
    color: RgbColor,
) {
    let mut ctrl = Vec::with_capacity(waypoints.len() + 2);
    ctrl.push(WorldPos::from_grid(start_hole));
    ctrl.extend_from_slice(waypoints);
    ctrl.push(cursor_world);

    let sampled = match mode {
        WireMode::Straight => ctrl.clone(),
        WireMode::Spline => sample_catmull_rom(&ctrl, 16),
    };

    let pts_screen: Vec<Pos2> = sampled
        .iter()
        .map(|&w| viewport.world_to_screen(canvas_rect, w))
        .collect();

    let width = (viewport.pixels_per_pitch * 0.24).clamp(2.5, 9.0);
    painter.add(egui::Shape::line(
        pts_screen,
        Stroke::new(width, color.with_alpha(220)),
    ));

    // Draw control point markers
    for wp in &ctrl {
        let sp = viewport.world_to_screen(canvas_rect, *wp);
        painter.circle_filled(sp, 4.0, Color32::WHITE);
        painter.circle_stroke(sp, 4.0, Stroke::new(1.5_f32, color.to_color32()));
    }
}
