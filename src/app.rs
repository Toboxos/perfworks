use egui::{Color32, CornerRadius, FontId, Margin, Rect, RichText, Stroke};
use uuid::Uuid;

use crate::canvas::{
    handle_viewport_navigation, render_workspace, CanvasRenderPreview, ControlScheme, SelectedItem,
    Viewport,
};
use crate::library::{get_footprint_by_id, standard_library, FootprintCategory};
use crate::model::{
    AxisLabelStyle, BoardKind, BoardTheme, ConnectorKind, GridPos, Perfboard, PlacedConnector,
    Project, RgbColor, Rotation, SolderLine, Wire, WireLayer, WireMode, WorldPos, PITCH_MM,
};

/// Active interactive tool in the CAD workspace
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolMode {
    /// Select, inspect, and move boards, connectors, waypoints, or solder traces
    Select,
    /// Move Perfboards freely on the 2D plane
    MoveBoard,
    /// Place a single connector pin (1x1)
    PlaceSinglePin,
    /// Place a parametric multi-pin header line (cols x rows)
    PlaceHeaderLine,
    /// Place a component from the standard library (JST-XH, Screw Terminal, DIP, etc.)
    PlaceLibraryFootprint,
    /// Route a wire starting and ending at holes with arbitrary waypoints in between (Straight or Spline)
    DrawWire,
    /// Draw a solder line spanning neighbor holes
    DrawSolder,
    /// Cut/uncut stripboard copper tracks at a hole
    CutTrack,
    /// Delete clicked items
    Eraser,
}

impl ToolMode {
    pub fn short_label(&self) -> &'static str {
        match self {
            ToolMode::Select => "Select / Edit",
            ToolMode::MoveBoard => "Move Board",
            ToolMode::PlaceSinglePin => "Single Pin",
            ToolMode::PlaceHeaderLine => "Header Line",
            ToolMode::PlaceLibraryFootprint => "Library Part",
            ToolMode::DrawWire => "Route Wire",
            ToolMode::DrawSolder => "Solder Trace",
            ToolMode::CutTrack => "Track Cutter",
            ToolMode::Eraser => "Eraser",
        }
    }

    pub fn shortcut_hint(&self) -> &'static str {
        match self {
            ToolMode::Select => "V / Esc",
            ToolMode::MoveBoard => "M",
            ToolMode::PlaceSinglePin => "P",
            ToolMode::PlaceHeaderLine => "H",
            ToolMode::PlaceLibraryFootprint => "L",
            ToolMode::DrawWire => "W",
            ToolMode::DrawSolder => "S",
            ToolMode::CutTrack => "X",
            ToolMode::Eraser => "D / Del",
        }
    }
}

/// Wire color presets
pub const WIRE_PALETTE: &[(&str, RgbColor)] = &[
    ("Red (VCC)", RgbColor::new(238, 72, 72)),
    ("Black (GND)", RgbColor::new(58, 64, 74)),
    ("Blue (SDA)", RgbColor::new(56, 182, 255)),
    ("Yellow (SCL)", RgbColor::new(250, 200, 56)),
    ("Green (SIG)", RgbColor::new(62, 206, 122)),
    ("Orange (PWM)", RgbColor::new(255, 142, 54)),
    ("Purple (AUX)", RgbColor::new(168, 112, 255)),
    ("White (CLK)", RgbColor::new(232, 236, 242)),
];

pub struct PerfworkApp {
    pub project: Project,
    pub undo_stack: Vec<Project>,
    pub redo_stack: Vec<Project>,

    pub viewport: Viewport,
    pub control_scheme: ControlScheme,
    pub tool: ToolMode,
    pub selected: Option<SelectedItem>,

    // Tool configuration state
    pub active_rotation: Rotation,
    pub single_pin_square: bool,
    pub header_cols: u32,
    pub header_rows: u32,
    pub header_female: bool,
    pub selected_footprint_id: String,

    // Wire tool configuration & active drawing state
    pub wire_mode: WireMode,
    pub wire_color: RgbColor,
    pub wire_layer: WireLayer,
    pub wire_snap_waypoints_half_grid: bool,
    pub active_wire_start: Option<GridPos>,
    pub active_wire_waypoints: Vec<WorldPos>,

    // Solder tool active drawing state
    pub active_solder_nodes: Vec<GridPos>,

    // Dragging state on canvas
    pub dragging_board: Option<(Uuid, WorldPos, GridPos)>,
    pub dragging_connector: Option<(Uuid, GridPos)>,
    pub dragging_wire_waypoint: Option<(Uuid, usize)>,

    // View toggles
    pub show_x_ray_blocked_holes: bool,
    pub show_pin_labels: bool,
    pub show_net_highlights: bool,

    // Modals & notifications
    pub show_new_board_modal: bool,
    pub new_board_name: String,
    pub new_board_cols: u32,
    pub new_board_rows: u32,
    pub new_board_col_style: AxisLabelStyle,
    pub new_board_row_style: AxisLabelStyle,
    pub new_board_kind: BoardKind,
    pub new_board_theme: BoardTheme,

    pub show_json_modal: bool,
    pub json_buffer: String,
    pub status_toast: Option<(String, bool)>, // (message, is_error)
}

impl Default for PerfworkApp {
    fn default() -> Self {
        Self {
            project: Project::new_starter(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            viewport: Viewport::default(),
            control_scheme: ControlScheme::Touchpad,
            tool: ToolMode::Select,
            selected: None,

            active_rotation: Rotation::Deg0,
            single_pin_square: false,
            header_cols: 4,
            header_rows: 1,
            header_female: false,
            selected_footprint_id: "jst_xh_3".to_string(),

            wire_mode: WireMode::Straight,
            wire_color: WIRE_PALETTE[2].1,
            wire_layer: WireLayer::TopComponent,
            wire_snap_waypoints_half_grid: false,
            active_wire_start: None,
            active_wire_waypoints: Vec::new(),

            active_solder_nodes: Vec::new(),

            dragging_board: None,
            dragging_connector: None,
            dragging_wire_waypoint: None,

            show_x_ray_blocked_holes: true,
            show_pin_labels: true,
            show_net_highlights: true,

            show_new_board_modal: false,
            new_board_name: "Aux Perfboard".to_string(),
            new_board_cols: 16,
            new_board_rows: 12,
            new_board_col_style: AxisLabelStyle::Numbers1Based,
            new_board_row_style: AxisLabelStyle::LettersUpper,
            new_board_kind: BoardKind::Perfboard,
            new_board_theme: BoardTheme::ClassicGreen,

            show_json_modal: false,
            json_buffer: String::new(),
            status_toast: Some((
                "Welcome to Perfwork! Switch Mouse / Touchpad navigation anytime in the bottom-right status bar."
                    .to_string(),
                false,
            )),
        }
    }
}

impl PerfworkApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_modern_cad_theme(&cc.egui_ctx);
        Self::default()
    }

    fn record_undo(&mut self) {
        self.undo_stack.push(self.project.clone());
        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            let current = std::mem::replace(&mut self.project, prev);
            self.redo_stack.push(current);
            self.selected = None;
            self.status_toast = Some(("Undid last action".to_string(), false));
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            let current = std::mem::replace(&mut self.project, next);
            self.undo_stack.push(current);
            self.selected = None;
            self.status_toast = Some(("Redid action".to_string(), false));
        }
    }

    fn build_preview_connector(&self, anchor: GridPos) -> Option<PlacedConnector> {
        let (kind, body_color, label) = match self.tool {
            ToolMode::PlaceSinglePin => (
                ConnectorKind::SinglePin {
                    square_pad: self.single_pin_square,
                },
                RgbColor::new(225, 185, 65),
                "PIN".to_string(),
            ),
            ToolMode::PlaceHeaderLine => (
                ConnectorKind::HeaderLine {
                    cols: self.header_cols,
                    rows: self.header_rows,
                    female: self.header_female,
                },
                if self.header_female {
                    RgbColor::new(42, 46, 54)
                } else {
                    RgbColor::new(52, 56, 66)
                },
                format!("{}x{}", self.header_cols, self.header_rows),
            ),
            ToolMode::PlaceLibraryFootprint => {
                let fp = get_footprint_by_id(&self.selected_footprint_id)?;
                (
                    ConnectorKind::LibraryFootprint {
                        footprint_id: fp.id.to_string(),
                    },
                    fp.default_color,
                    fp.short_name.to_string(),
                )
            }
            _ => return None,
        };

        Some(PlacedConnector {
            id: Uuid::nil(),
            designator: "NEW".to_string(),
            label,
            anchor,
            rotation: self.active_rotation,
            kind,
            body_color,
            pin_labels: Vec::new(),
        })
    }

    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }

        ctx.input(|i| {
            // Undo / Redo
            if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::Z) {
                if i.modifiers.shift {
                    self.redo();
                } else {
                    self.undo();
                }
            }

            if i.key_pressed(egui::Key::Escape) {
                if self.active_wire_start.is_some() {
                    self.active_wire_start = None;
                    self.active_wire_waypoints.clear();
                } else if !self.active_solder_nodes.is_empty() {
                    self.active_solder_nodes.clear();
                } else if self.tool != ToolMode::Select {
                    self.tool = ToolMode::Select;
                } else {
                    self.selected = None;
                }
            }

            if i.key_pressed(egui::Key::R) {
                self.active_rotation = self.active_rotation.next_cw();
                if let Some(SelectedItem::Connector(cid)) = self.selected {
                    if let Some(c) = self.project.connectors.iter().find(|c| c.id == cid).cloned() {
                        let mut rotated = c.clone();
                        rotated.rotation = rotated.rotation.next_cw();
                        if self
                            .project
                            .validate_connector_placement(&rotated, Some(cid))
                            .is_ok()
                        {
                            self.record_undo();
                            if let Some(target) =
                                self.project.connectors.iter_mut().find(|c| c.id == cid)
                            {
                                target.rotation = rotated.rotation;
                            }
                        } else {
                            self.status_toast = Some((
                                "Cannot rotate: target holes are blocked or off-board".to_string(),
                                true,
                            ));
                        }
                    }
                }
            }

            if i.key_pressed(egui::Key::V) {
                self.tool = ToolMode::Select;
            }
            if i.key_pressed(egui::Key::M) {
                self.tool = ToolMode::MoveBoard;
            }
            if i.key_pressed(egui::Key::P) {
                self.tool = ToolMode::PlaceSinglePin;
            }
            if i.key_pressed(egui::Key::H) {
                self.tool = ToolMode::PlaceHeaderLine;
            }
            if i.key_pressed(egui::Key::L) {
                self.tool = ToolMode::PlaceLibraryFootprint;
            }
            if i.key_pressed(egui::Key::W) {
                self.tool = ToolMode::DrawWire;
            }
            if i.key_pressed(egui::Key::S) && !i.modifiers.command && !i.modifiers.ctrl {
                self.tool = ToolMode::DrawSolder;
            }
            if i.key_pressed(egui::Key::X) {
                self.tool = ToolMode::CutTrack;
            }
            if i.key_pressed(egui::Key::Tab) && self.tool == ToolMode::DrawWire {
                self.wire_mode = match self.wire_mode {
                    WireMode::Straight => WireMode::Spline,
                    WireMode::Spline => WireMode::Straight,
                };
            }
            if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) {
                self.delete_selected();
            }
        });
    }

    fn delete_selected(&mut self) {
        let Some(sel) = self.selected.clone() else {
            return;
        };
        self.record_undo();
        match sel {
            SelectedItem::Board(id) => {
                self.project.boards.retain(|b| b.id != id);
            }
            SelectedItem::Connector(id) => {
                self.project.connectors.retain(|c| c.id != id);
            }
            SelectedItem::Wire(id) => {
                self.project.wires.retain(|w| w.id != id);
            }
            SelectedItem::SolderLine(id) => {
                self.project.solder_lines.retain(|s| s.id != id);
            }
        }
        self.selected = None;
    }
}

impl eframe::App for PerfworkApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_keyboard_shortcuts(ctx);

        // 1. Top Command Ribbon (Fusion / KiCad style)
        egui::TopBottomPanel::top("top_ribbon")
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_rgb(28, 31, 38))
                    .inner_margin(Margin::symmetric(12, 8))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(46, 51, 62))),
            )
            .show(ctx, |ui| {
                self.render_top_ribbon(ui);
            });

        // 2. Bottom Status Bar with KiCad-inspired Control Scheme Dropdown, Coordinates, Zoom & DRC
        egui::TopBottomPanel::bottom("bottom_status_bar")
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_rgb(24, 27, 33))
                    .inner_margin(Margin::symmetric(12, 6))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(46, 51, 62))),
            )
            .show(ctx, |ui| {
                self.render_bottom_status_bar(ui, ctx);
            });

        // 3. Left Dock: Tool Palette & Standard Connector / Component Library
        egui::SidePanel::left("left_library_panel")
            .default_width(275.0)
            .width_range(240.0..=360.0)
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_rgb(26, 29, 36))
                    .inner_margin(Margin::same(10))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(44, 49, 60))),
            )
            .show(ctx, |ui| {
                self.render_left_panel(ui);
            });

        // 4. Right Dock: Context Inspector (Board dimensions/labels, Connector properties, Wire mode/color)
        egui::SidePanel::right("right_inspector_panel")
            .default_width(290.0)
            .width_range(250.0..=380.0)
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_rgb(26, 29, 36))
                    .inner_margin(Margin::same(10))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(44, 49, 60))),
            )
            .show(ctx, |ui| {
                self.render_right_inspector(ui);
            });

        // 5. Center Interactive 2D CAD Canvas
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(20, 22, 27)))
            .show(ctx, |ui| {
                self.render_center_canvas(ui);
            });

        // 6. Modals (Add Perfboard Dialog & Import/Export JSON Dialog)
        self.render_modals(ctx);
    }
}

impl PerfworkApp {
    fn render_top_ribbon(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            // App Branding Badge
            ui.label(
                RichText::new("◆ PERFWORK")
                    .font(FontId::proportional(16.5))
                    .strong()
                    .color(Color32::from_rgb(88, 202, 255)),
            );
            ui.label(
                RichText::new("CAD")
                    .font(FontId::monospace(11.5))
                    .color(Color32::from_rgb(142, 152, 170)),
            );

            ui.separator();

            // Project & File Actions
            if ui
                .button("＋ New Board")
                .on_hover_text("Add another perfboard, stripboard, or breadboard to the plane")
                .clicked()
            {
                self.show_new_board_modal = true;
            }

            if ui
                .button("↺ Demo Circuit")
                .on_hover_text("Load the sample multi-board JST-XH & MCU prototype")
                .clicked()
            {
                self.record_undo();
                self.project = Project::new_starter();
                self.selected = None;
                self.status_toast = Some(("Loaded starter prototype circuit".to_string(), false));
            }

            if ui
                .button("🧹 Clear All")
                .on_hover_text("Reset to a single clean perfboard")
                .clicked()
            {
                self.record_undo();
                self.project = Project::new_empty("Untitled Perfboard");
                self.selected = None;
                self.status_toast = Some(("Created clean perfboard workspace".to_string(), false));
            }

            #[cfg(not(target_arch = "wasm32"))]
            {
                if ui.button("💾 Save").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Perfwork Project", &["perfwork.json", "json"])
                        .set_file_name("circuit.perfwork.json")
                        .save_file()
                    {
                        if let Ok(json) = serde_json::to_string_pretty(&self.project) {
                            if std::fs::write(&path, json).is_ok() {
                                self.status_toast =
                                    Some((format!("Saved project to {}", path.display()), false));
                            }
                        }
                    }
                }
                if ui.button("📂 Open").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Perfwork Project", &["json"])
                        .pick_file()
                    {
                        if let Ok(data) = std::fs::read_to_string(&path) {
                            if let Ok(proj) = serde_json::from_str::<Project>(&data) {
                                self.record_undo();
                                self.project = proj;
                                self.selected = None;
                                self.status_toast =
                                    Some((format!("Loaded {}", path.display()), false));
                            }
                        }
                    }
                }
            }

            if ui
                .button("⇄ JSON")
                .on_hover_text("View, copy, or paste project JSON")
                .clicked()
            {
                self.json_buffer =
                    serde_json::to_string_pretty(&self.project).unwrap_or_default();
                self.show_json_modal = true;
            }

            ui.separator();

            // Undo / Redo
            if ui
                .add_enabled(!self.undo_stack.is_empty(), egui::Button::new("↶ Undo"))
                .clicked()
            {
                self.undo();
            }
            if ui
                .add_enabled(!self.redo_stack.is_empty(), egui::Button::new("↷ Redo"))
                .clicked()
            {
                self.redo();
            }

            ui.separator();

            // Primary Tool Mode Buttons
            let tools = [
                (ToolMode::Select, "↖ Select (V)"),
                (ToolMode::MoveBoard, "✥ Move Board (M)"),
                (ToolMode::PlaceSinglePin, "⊙ Single Pin (P)"),
                (ToolMode::PlaceHeaderLine, "⋮⋮ Header (H)"),
                (ToolMode::PlaceLibraryFootprint, "⊞ Library (L)"),
                (ToolMode::DrawWire, "〰 Wire (W)"),
                (ToolMode::DrawSolder, "Solder (S)"),
                (ToolMode::CutTrack, "⊗ Cut Track (X)"),
                (ToolMode::Eraser, "⌫ Erase (D)"),
            ];

            for (mode, label) in tools {
                let active = self.tool == mode;
                let btn = egui::Button::new(RichText::new(label).color(if active {
                    Color32::from_rgb(255, 255, 255)
                } else {
                    Color32::from_rgb(200, 208, 222)
                }))
                .fill(if active {
                    Color32::from_rgb(38, 118, 195)
                } else {
                    Color32::from_rgb(38, 43, 52)
                });
                if ui.add(btn).clicked() {
                    self.tool = mode;
                    self.active_wire_start = None;
                    self.active_wire_waypoints.clear();
                    self.active_solder_nodes.clear();
                }
            }
        });
    }

    fn render_left_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading(
                RichText::new("Placement & Tools")
                    .size(14.5)
                    .strong()
                    .color(Color32::from_rgb(220, 228, 240)),
            );
            ui.add_space(6.0);

            // Rotation & Quick Settings Card
            egui::Frame::NONE
                .fill(Color32::from_rgb(33, 37, 46))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Orientation:");
                        if ui
                            .button(format!("⟳ {}° (R)", self.active_rotation.degrees()))
                            .clicked()
                        {
                            self.active_rotation = self.active_rotation.next_cw();
                        }
                    });
                    ui.checkbox(
                        &mut self.show_x_ray_blocked_holes,
                        "Show Blocked Overhang Holes",
                    );
                    ui.checkbox(&mut self.show_pin_labels, "Show Pin Labels");
                });

            ui.add_space(10.0);

            // 1. Connector Pins & Parametric Headers Section
            ui.label(
                RichText::new("1. CONNECTOR PINS & HEADERS")
                    .size(11.5)
                    .strong()
                    .color(Color32::from_rgb(130, 185, 240)),
            );
            ui.add_space(4.0);

            egui::Frame::NONE
                .fill(Color32::from_rgb(33, 37, 46))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let is_single = self.tool == ToolMode::PlaceSinglePin;
                        if ui
                            .selectable_label(is_single, "⊙ Place Single Pin (1×1)")
                            .clicked()
                        {
                            self.tool = ToolMode::PlaceSinglePin;
                        }
                        ui.checkbox(&mut self.single_pin_square, "Square Pad");
                    });

                    ui.separator();

                    let is_hdr = self.tool == ToolMode::PlaceHeaderLine;
                    if ui
                        .selectable_label(
                            is_hdr,
                            format!(
                                "⋮⋮ Place Header Line ({}×{})",
                                self.header_cols, self.header_rows
                            ),
                        )
                        .clicked()
                    {
                        self.tool = ToolMode::PlaceHeaderLine;
                    }

                    ui.horizontal(|ui| {
                        ui.label("Pins (L):");
                        if ui
                            .add(egui::DragValue::new(&mut self.header_cols).range(1..=40))
                            .changed()
                        {
                            self.tool = ToolMode::PlaceHeaderLine;
                        }
                        ui.label("Rows:");
                        if ui
                            .add(egui::DragValue::new(&mut self.header_rows).range(1..=4))
                            .changed()
                        {
                            self.tool = ToolMode::PlaceHeaderLine;
                        }
                    });

                    // Quick preset buttons for common headers
                    ui.horizontal_wrapped(|ui| {
                        for (c, r, lbl) in [
                            (2, 1, "1×2"),
                            (3, 1, "1×3"),
                            (4, 1, "1×4"),
                            (6, 1, "1×6"),
                            (8, 1, "1×8"),
                            (10, 1, "1×10"),
                            (4, 2, "2×4"),
                            (5, 2, "2×5"),
                        ] {
                            if ui.small_button(lbl).clicked() {
                                self.header_cols = c;
                                self.header_rows = r;
                                self.tool = ToolMode::PlaceHeaderLine;
                            }
                        }
                    });

                    ui.checkbox(&mut self.header_female, "Female Socket Header");
                });

            ui.add_space(10.0);

            // 2. Wire Routing & Solder Lines Section
            ui.label(
                RichText::new("2. WIRES & SOLDER CONNECTIONS")
                    .size(11.5)
                    .strong()
                    .color(Color32::from_rgb(130, 185, 240)),
            );
            ui.add_space(4.0);

            egui::Frame::NONE
                .fill(Color32::from_rgb(33, 37, 46))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                self.tool == ToolMode::DrawWire
                                    && self.wire_mode == WireMode::Straight,
                                "╱╲ Straight Wire",
                            )
                            .clicked()
                        {
                            self.tool = ToolMode::DrawWire;
                            self.wire_mode = WireMode::Straight;
                        }
                        if ui
                            .selectable_label(
                                self.tool == ToolMode::DrawWire
                                    && self.wire_mode == WireMode::Spline,
                                "〰 Spline Wire",
                            )
                            .clicked()
                        {
                            self.tool = ToolMode::DrawWire;
                            self.wire_mode = WireMode::Spline;
                        }
                    });

                    // Wire Color Swatches
                    ui.horizontal_wrapped(|ui| {
                        for (name, col) in WIRE_PALETTE {
                            let selected = self.wire_color == *col;
                            let (rect, resp) =
                                ui.allocate_exact_size(egui::vec2(22.0, 18.0), egui::Sense::click());
                            ui.painter().rect_filled(rect, 3.0, col.to_color32());
                            if selected {
                                ui.painter().rect_stroke(
                                    rect,
                                    3.0,
                                    Stroke::new(2.0_f32, Color32::WHITE),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            if resp.on_hover_text(*name).clicked() {
                                self.wire_color = *col;
                                self.tool = ToolMode::DrawWire;
                            }
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Layer:");
                        egui::ComboBox::from_id_salt("wire_layer_combo")
                            .selected_text(self.wire_layer.display_name())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.wire_layer,
                                    WireLayer::TopComponent,
                                    WireLayer::TopComponent.display_name(),
                                );
                                ui.selectable_value(
                                    &mut self.wire_layer,
                                    WireLayer::BottomSolder,
                                    WireLayer::BottomSolder.display_name(),
                                );
                            });
                    });

                    ui.checkbox(
                        &mut self.wire_snap_waypoints_half_grid,
                        "Snap Wire Bends to 0.5 Grid",
                    );

                    ui.separator();

                    if ui
                        .selectable_label(
                            self.tool == ToolMode::DrawSolder,
                            "☍ Draw Solder Bridge / Trace (S)",
                        )
                        .on_hover_text(
                            "Click or drag across neighbor holes to solder them together. Soldered holes block new connector pins but connect to other solder traces.",
                        )
                        .clicked()
                    {
                        self.tool = ToolMode::DrawSolder;
                    }
                });

            ui.add_space(10.0);

            // 3. Standard Component & Connector Library (JST-XH, Screw Terminals, DIP, etc.)
            ui.label(
                RichText::new("3. STANDARD COMPONENT LIBRARY")
                    .size(11.5)
                    .strong()
                    .color(Color32::from_rgb(130, 185, 240)),
            );
            ui.add_space(4.0);

            let lib = standard_library();
            for category in FootprintCategory::ALL {
                egui::CollapsingHeader::new(
                    RichText::new(category.display_name())
                        .size(12.5)
                        .color(Color32::from_rgb(210, 218, 232)),
                )
                .default_open(category == FootprintCategory::JstXh || category == FootprintCategory::IcSocket)
                .show(ui, |ui| {
                    for fp in lib.iter().filter(|f| f.category == category) {
                        let is_active = self.tool == ToolMode::PlaceLibraryFootprint
                            && self.selected_footprint_id == fp.id;
                        let resp = ui.selectable_label(
                            is_active,
                            format!("{}  ({} pins)", fp.short_name, fp.pins.len()),
                        );
                        if resp.on_hover_text(format!("{}\n{}", fp.name, fp.description)).clicked()
                        {
                            self.selected_footprint_id = fp.id.to_string();
                            self.tool = ToolMode::PlaceLibraryFootprint;
                        }
                    }
                });
            }
        });
    }

    fn render_right_inspector(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading(
                RichText::new("Inspector & Boards")
                    .size(14.5)
                    .strong()
                    .color(Color32::from_rgb(220, 228, 240)),
            );
            ui.add_space(6.0);

            // Selected Item Inspector
            match self.selected.clone() {
                Some(SelectedItem::Board(board_id)) => {
                    self.render_board_inspector(ui, board_id);
                }
                Some(SelectedItem::Connector(conn_id)) => {
                    self.render_connector_inspector(ui, conn_id);
                }
                Some(SelectedItem::Wire(wire_id)) => {
                    self.render_wire_inspector(ui, wire_id);
                }
                Some(SelectedItem::SolderLine(solder_id)) => {
                    self.render_solder_inspector(ui, solder_id);
                }
                None => {
                    egui::Frame::NONE
                        .fill(Color32::from_rgb(33, 37, 46))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::same(10))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("No item selected")
                                    .strong()
                                    .color(Color32::from_rgb(185, 194, 210)),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(
                                    "Click any perfboard, connector, wire, or solder trace on the canvas to inspect and customize its properties.",
                                )
                                .size(12.0)
                                .color(Color32::from_rgb(142, 152, 170)),
                            );
                        });
                }
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(6.0);

            // Perfboards List & Quick Management
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("PERFBOARDS ON PLANE")
                        .size(11.5)
                        .strong()
                        .color(Color32::from_rgb(130, 185, 240)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("＋ Add Board").clicked() {
                        self.show_new_board_modal = true;
                    }
                });
            });
            ui.add_space(4.0);

            let board_summaries: Vec<(Uuid, String, u32, u32, GridPos, BoardKind)> = self
                .project
                .boards
                .iter()
                .map(|b| (b.id, b.name.clone(), b.cols, b.rows, b.origin, b.kind))
                .collect();

            for (bid, bname, cols, rows, origin, kind) in board_summaries {
                let is_sel = self.selected == Some(SelectedItem::Board(bid));
                egui::Frame::NONE
                    .fill(if is_sel {
                        Color32::from_rgb(38, 62, 92)
                    } else {
                        Color32::from_rgb(33, 37, 46)
                    })
                    .corner_radius(CornerRadius::same(5))
                    .inner_margin(Margin::same(7))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(
                                    is_sel,
                                    RichText::new(&bname).strong(),
                                )
                                .clicked()
                            {
                                self.selected = Some(SelectedItem::Board(bid));
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!("{}×{}", cols, rows))
                                            .monospace()
                                            .size(11.5),
                                    );
                                },
                            );
                        });
                        ui.label(
                            RichText::new(format!(
                                "{} • Origin ({}, {})",
                                kind.display_name(),
                                origin.col,
                                origin.row
                            ))
                            .size(11.0)
                            .color(Color32::from_rgb(148, 158, 176)),
                        );
                    });
                ui.add_space(4.0);
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            // Project Summary / Bill of Materials
            ui.label(
                RichText::new("CIRCUIT SUMMARY")
                    .size(11.5)
                    .strong()
                    .color(Color32::from_rgb(130, 185, 240)),
            );
            ui.add_space(4.0);
            egui::Frame::NONE
                .fill(Color32::from_rgb(33, 37, 46))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    ui.label(format!("• Boards: {}", self.project.boards.len()));
                    ui.label(format!("• Connectors / Parts: {}", self.project.connectors.len()));
                    ui.label(format!("• Routed Wires: {}", self.project.wires.len()));
                    ui.label(format!("• Solder Lines: {}", self.project.solder_lines.len()));
                });
        });
    }

    fn render_board_inspector(&mut self, ui: &mut egui::Ui, board_id: Uuid) {
        let Some(idx) = self.project.boards.iter().position(|b| b.id == board_id) else {
            return;
        };

        let mut delete_clicked = false;
        egui::Frame::NONE
            .fill(Color32::from_rgb(33, 37, 46))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("PERFBOARD PROPERTIES")
                        .size(11.5)
                        .strong()
                        .color(Color32::from_rgb(110, 208, 255)),
                );
                ui.add_space(6.0);

                let board = &mut self.project.boards[idx];

                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut board.name);
                });

                ui.add_space(4.0);
                ui.label(RichText::new("Dimensions (Holes):").strong().size(12.0));
                ui.horizontal(|ui| {
                    ui.label("Columns (X):");
                    ui.add(egui::DragValue::new(&mut board.cols).range(2..=120));
                    ui.label("Rows (Y):");
                    ui.add(egui::DragValue::new(&mut board.rows).range(2..=120));
                });
                ui.label(
                    RichText::new(format!(
                        "Physical size: {:.1} mm × {:.1} mm",
                        (board.cols as f32 + 1.5) * PITCH_MM,
                        (board.rows as f32 + 1.5) * PITCH_MM
                    ))
                    .size(11.0)
                    .color(Color32::from_rgb(148, 158, 175)),
                );

                ui.add_space(6.0);
                ui.label(RichText::new("Plane Position (Origin):").strong().size(12.0));
                ui.horizontal(|ui| {
                    ui.label("Col X:");
                    ui.add(egui::DragValue::new(&mut board.origin.col).range(-200..=200));
                    ui.label("Row Y:");
                    ui.add(egui::DragValue::new(&mut board.origin.row).range(-200..=200));
                });

                ui.add_space(6.0);
                ui.label(RichText::new("Row & Column Identifiers:").strong().size(12.0));
                egui::ComboBox::from_label("Column Labels")
                    .selected_text(board.col_label_style.display_name())
                    .show_ui(ui, |ui| {
                        for style in AxisLabelStyle::ALL {
                            ui.selectable_value(
                                &mut board.col_label_style,
                                style,
                                style.display_name(),
                            );
                        }
                    });

                egui::ComboBox::from_label("Row Labels")
                    .selected_text(board.row_label_style.display_name())
                    .show_ui(ui, |ui| {
                        for style in AxisLabelStyle::ALL {
                            ui.selectable_value(
                                &mut board.row_label_style,
                                style,
                                style.display_name(),
                            );
                        }
                    });

                ui.checkbox(&mut board.labels_both_sides, "Show Identifiers on All 4 Edges");

                ui.add_space(6.0);
                ui.label(RichText::new("Board Type & Appearance:").strong().size(12.0));
                egui::ComboBox::from_label("Type")
                    .selected_text(board.kind.display_name())
                    .show_ui(ui, |ui| {
                        for k in BoardKind::ALL {
                            ui.selectable_value(&mut board.kind, k, k.display_name());
                        }
                    });

                egui::ComboBox::from_label("Theme")
                    .selected_text(board.theme.display_name())
                    .show_ui(ui, |ui| {
                        for t in BoardTheme::ALL {
                            ui.selectable_value(&mut board.theme, t, t.display_name());
                        }
                    });

                ui.checkbox(&mut board.mounting_holes, "Corner Mounting Holes");

                ui.add_space(8.0);
                if ui
                    .button(RichText::new("🗑 Delete Board").color(Color32::from_rgb(255, 110, 110)))
                    .clicked()
                {
                    delete_clicked = true;
                }
            });

        if delete_clicked {
            self.delete_selected();
        }
    }

    fn render_connector_inspector(&mut self, ui: &mut egui::Ui, conn_id: Uuid) {
        let Some(idx) = self
            .project
            .connectors
            .iter()
            .position(|c| c.id == conn_id)
        else {
            return;
        };

        let mut delete_clicked = false;
        let mut rotate_clicked = false;

        egui::Frame::NONE
            .fill(Color32::from_rgb(33, 37, 46))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("CONNECTOR / COMPONENT")
                        .size(11.5)
                        .strong()
                        .color(Color32::from_rgb(110, 208, 255)),
                );
                ui.add_space(6.0);

                let conn = &mut self.project.connectors[idx];
                ui.horizontal(|ui| {
                    ui.label("Designator:");
                    ui.text_edit_singleline(&mut conn.designator);
                });
                ui.horizontal(|ui| {
                    ui.label("Value / Label:");
                    ui.text_edit_singleline(&mut conn.label);
                });

                ui.add_space(4.0);
                ui.label(format!(
                    "Anchor (Pin 1): Col {}, Row {}",
                    conn.anchor.col, conn.anchor.row
                ));
                ui.horizontal(|ui| {
                    ui.label(format!("Rotation: {}°", conn.rotation.degrees()));
                    if ui.button("⟳ Rotate 90° (R)").clicked() {
                        rotate_clicked = true;
                    }
                });

                if let ConnectorKind::HeaderLine { cols, rows, female } = &mut conn.kind {
                    ui.separator();
                    ui.label("Header Dimensions:");
                    ui.horizontal(|ui| {
                        ui.label("Pins:");
                        ui.add(egui::DragValue::new(cols).range(1..=40));
                        ui.label("Rows:");
                        ui.add(egui::DragValue::new(rows).range(1..=4));
                    });
                    ui.checkbox(female, "Female Socket Header");
                }

                let blocked = conn.blocked_overhang_holes();
                if !blocked.is_empty() {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "⚠ Housing overhangs & blocks {} adjacent hole(s)",
                            blocked.len()
                        ))
                        .size(11.5)
                        .color(Color32::from_rgb(255, 168, 92)),
                    );
                }

                ui.add_space(8.0);
                if ui
                    .button(
                        RichText::new("🗑 Delete Component")
                            .color(Color32::from_rgb(255, 110, 110)),
                    )
                    .clicked()
                {
                    delete_clicked = true;
                }
            });

        if rotate_clicked {
            if let Some(c) = self.project.connectors.get(idx).cloned() {
                let mut rotated = c;
                rotated.rotation = rotated.rotation.next_cw();
                if self
                    .project
                    .validate_connector_placement(&rotated, Some(conn_id))
                    .is_ok()
                {
                    self.record_undo();
                    self.project.connectors[idx].rotation = rotated.rotation;
                } else {
                    self.status_toast = Some((
                        "Rotation blocked by occupied hole or board edge".to_string(),
                        true,
                    ));
                }
            }
        }

        if delete_clicked {
            self.delete_selected();
        }
    }

    fn render_wire_inspector(&mut self, ui: &mut egui::Ui, wire_id: Uuid) {
        let Some(idx) = self.project.wires.iter().position(|w| w.id == wire_id) else {
            return;
        };

        let mut delete_clicked = false;
        egui::Frame::NONE
            .fill(Color32::from_rgb(33, 37, 46))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("WIRE PROPERTIES")
                        .size(11.5)
                        .strong()
                        .color(Color32::from_rgb(110, 208, 255)),
                );
                ui.add_space(6.0);

                let wire = &mut self.project.wires[idx];
                ui.horizontal(|ui| {
                    ui.label("Net / Label:");
                    ui.text_edit_singleline(&mut wire.label);
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Curve Mode:");
                    ui.selectable_value(&mut wire.mode, WireMode::Straight, "Straight");
                    ui.selectable_value(&mut wire.mode, WireMode::Spline, "Spline");
                });

                ui.add_space(4.0);
                ui.label("Wire Color:");
                ui.horizontal_wrapped(|ui| {
                    for (name, col) in WIRE_PALETTE {
                        let sel = wire.color == *col;
                        let (rect, resp) =
                            ui.allocate_exact_size(egui::vec2(20.0, 16.0), egui::Sense::click());
                        ui.painter().rect_filled(rect, 3.0, col.to_color32());
                        if sel {
                            ui.painter().rect_stroke(
                                rect,
                                3.0,
                                Stroke::new(2.0_f32, Color32::WHITE),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if resp.on_hover_text(*name).clicked() {
                            wire.color = *col;
                        }
                    }
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Gauge / Width:");
                    ui.add(
                        egui::Slider::new(&mut wire.thickness, 0.6..=2.2).step_by(0.1),
                    );
                });

                ui.label(format!(
                    "Start Hole: ({}, {}) → End Hole: ({}, {})",
                    wire.start_hole.col, wire.start_hole.row, wire.end_hole.col, wire.end_hole.row
                ));
                ui.label(format!("Intermediate Waypoints: {}", wire.waypoints.len()));
                if !wire.waypoints.is_empty() && ui.small_button("Clear Intermediate Bends").clicked()
                {
                    wire.waypoints.clear();
                }

                ui.add_space(8.0);
                if ui
                    .button(RichText::new("🗑 Delete Wire").color(Color32::from_rgb(255, 110, 110)))
                    .clicked()
                {
                    delete_clicked = true;
                }
            });

        if delete_clicked {
            self.delete_selected();
        }
    }

    fn render_solder_inspector(&mut self, ui: &mut egui::Ui, solder_id: Uuid) {
        let Some(idx) = self
            .project
            .solder_lines
            .iter()
            .position(|s| s.id == solder_id)
        else {
            return;
        };

        let mut delete_clicked = false;
        egui::Frame::NONE
            .fill(Color32::from_rgb(33, 37, 46))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("SOLDER LINE / BRIDGE")
                        .size(11.5)
                        .strong()
                        .color(Color32::from_rgb(110, 208, 255)),
                );
                ui.add_space(6.0);

                let solder = &mut self.project.solder_lines[idx];
                ui.label(format!("Spans {} connected holes", solder.nodes.len()));
                ui.label(
                    RichText::new(
                        "• Blocks covered holes for new connector pins\n• Can connect with other solder traces at shared holes",
                    )
                    .size(11.5)
                    .color(Color32::from_rgb(155, 165, 182)),
                );

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Solder Width:");
                    ui.add(
                        egui::Slider::new(&mut solder.width_scale, 0.7..=1.5).step_by(0.05),
                    );
                });

                ui.add_space(8.0);
                if ui
                    .button(
                        RichText::new("🗑 Remove Solder Trace")
                            .color(Color32::from_rgb(255, 110, 110)),
                    )
                    .clicked()
                {
                    delete_clicked = true;
                }
            });

        if delete_clicked {
            self.delete_selected();
        }
    }

    fn render_bottom_status_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            // Active Tool Pill
            egui::Frame::NONE
                .fill(Color32::from_rgb(38, 108, 180))
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::symmetric(8, 2))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!(
                            "Tool: {} ({})",
                            self.tool.short_label(),
                            self.tool.shortcut_hint()
                        ))
                        .size(11.5)
                        .strong()
                        .color(Color32::WHITE),
                    );
                });

            ui.separator();

            // Toast or Control Hint
            if let Some((ref msg, is_err)) = self.status_toast {
                ui.label(
                    RichText::new(msg).size(12.0).color(if is_err {
                        Color32::from_rgb(255, 115, 115)
                    } else {
                        Color32::from_rgb(175, 220, 185)
                    }),
                );
            } else {
                ui.label(
                    RichText::new(self.control_scheme.hint_text())
                        .size(11.5)
                        .color(Color32::from_rgb(148, 158, 175)),
                );
            }

            // Right-aligned Zoom Controls & KiCad-style Control Mode Dropdown
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // KiCad-style Control Mode Selector Dropdown in Bottom Status Bar
                egui::ComboBox::from_id_salt("status_bar_control_scheme")
                    .selected_text(
                        RichText::new(format!("🖱 {}", self.control_scheme.display_name()))
                            .size(12.0)
                            .color(Color32::from_rgb(225, 232, 245)),
                    )
                    .width(235.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.control_scheme,
                            ControlScheme::Touchpad,
                            ControlScheme::Touchpad.display_name(),
                        )
                        .on_hover_text(
                            "KiCad-style Touchpad control: 2-finger scroll pans in X/Y, pinch or Cmd/Ctrl+scroll zooms",
                        );
                        ui.selectable_value(
                            &mut self.control_scheme,
                            ControlScheme::Mouse,
                            ControlScheme::Mouse.display_name(),
                        )
                        .on_hover_text(
                            "Standard CAD Mouse control: Scroll wheel zooms at cursor, Middle/Right drag pans",
                        );
                    });

                ui.separator();

                let zoom_pct = (self.viewport.pixels_per_pitch / 27.0 * 100.0).round() as i32;
                if ui.small_button("Fit").on_hover_text("Fit all boards in view").clicked() {
                    let screen = ctx.screen_rect();
                    self.viewport.fit_project(screen, &self.project);
                }
                if ui.small_button("＋").clicked() {
                    let screen = ctx.screen_rect();
                    self.viewport
                        .zoom_at_screen_pos(screen, screen.center(), 1.2);
                }
                ui.label(
                    RichText::new(format!("{}%", zoom_pct))
                        .monospace()
                        .size(11.5),
                );
                if ui.small_button("－").clicked() {
                    let screen = ctx.screen_rect();
                    self.viewport
                        .zoom_at_screen_pos(screen, screen.center(), 1.0 / 1.2);
                }
            });
        });
    }

    fn render_center_canvas(&mut self, ui: &mut egui::Ui) {
        let (canvas_rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());

        let right_drag_available =
            self.active_wire_start.is_none() && self.active_solder_nodes.is_empty();
        handle_viewport_navigation(
            ui,
            &response,
            canvas_rect,
            &mut self.viewport,
            self.control_scheme,
            right_drag_available,
        );

        let pointer_screen = response.hover_pos().or_else(|| ui.input(|i| i.pointer.latest_pos()));
        let pointer_world = pointer_screen.map(|p| self.viewport.screen_to_world(canvas_rect, p));
        let hovered_hole = pointer_world.map(|w| w.snap_to_grid());

        // Handle Canvas Interactions per Tool Mode
        if let (Some(world_pos), Some(grid_pos)) = (pointer_world, hovered_hole) {
            self.handle_canvas_interaction(ui, &response, world_pos, grid_pos);
        }

        // Build Ghost Preview if in a Placement tool
        let ghost_connector = hovered_hole.and_then(|h| self.build_preview_connector(h));

        // Build active wire preview
        let wire_cursor_world = pointer_world.unwrap_or(WorldPos::new(0.0, 0.0));
        let wire_preview = self.active_wire_start.map(|start| {
            let snapped_cursor = if let Some(h) = hovered_hole {
                if self.project.find_board_at_hole(h).is_some()
                    && world_pos_near_grid_center(wire_cursor_world)
                {
                    WorldPos::from_grid(h)
                } else if self.wire_snap_waypoints_half_grid {
                    wire_cursor_world.snap_half_grid()
                } else {
                    wire_cursor_world
                }
            } else {
                wire_cursor_world
            };
            (
                start,
                self.active_wire_waypoints.as_slice(),
                snapped_cursor,
                self.wire_mode,
                self.wire_color,
            )
        });

        let solder_preview = if !self.active_solder_nodes.is_empty() {
            Some(self.active_solder_nodes.as_slice())
        } else {
            None
        };

        let painter = ui.painter_at(canvas_rect);
        render_workspace(
            &painter,
            canvas_rect,
            &self.viewport,
            &self.project,
            &self.selected,
            &CanvasRenderPreview {
                ghost_connector: ghost_connector.as_ref(),
                active_wire_in_progress: wire_preview,
                active_solder_in_progress: solder_preview,
                hovered_hole,
                show_x_ray_blocked_holes: self.show_x_ray_blocked_holes,
                show_pin_labels: self.show_pin_labels,
                show_net_highlights: self.show_net_highlights,
            },
        );

        // Draw HUD Coordinate & Hole Occupancy Overlay in the top-left of the canvas
        if let Some(hole) = hovered_hole {
            self.draw_canvas_hud_overlay(&painter, canvas_rect, hole);
        }
    }

    fn handle_canvas_interaction(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        world_pos: WorldPos,
        grid_pos: GridPos,
    ) {
        let space_down = ui.input(|i| i.key_down(egui::Key::Space));
        if space_down {
            return;
        }

        // Right-click cancels active wire/solder drawing or rotates component during placement
        if response.clicked_by(egui::PointerButton::Secondary) {
            if self.active_wire_start.is_some() {
                if !self.active_wire_waypoints.is_empty() {
                    self.active_wire_waypoints.pop();
                } else {
                    self.active_wire_start = None;
                }
                return;
            }
            if !self.active_solder_nodes.is_empty() {
                self.commit_active_solder_line();
                return;
            }
            if matches!(
                self.tool,
                ToolMode::PlaceSinglePin
                    | ToolMode::PlaceHeaderLine
                    | ToolMode::PlaceLibraryFootprint
            ) {
                self.active_rotation = self.active_rotation.next_cw();
                return;
            }
        }

        match self.tool {
            ToolMode::Select => {
                // 1. Start dragging or selecting on primary press
                if response. clicked_by(egui::PointerButton::Primary) {
                    self.selected = self.hit_test(world_pos, grid_pos);
                }

                if response.drag_started_by(egui::PointerButton::Primary) {
                    // Check if user grabbed a wire waypoint handle first
                    if let Some(SelectedItem::Wire(wid)) = self.selected {
                        if let Some(w) = self.project.wires.iter().find(|w| w.id == wid) {
                            for (i, wp) in w.waypoints.iter().enumerate() {
                                if wp.distance(world_pos) < 0.48 {
                                    self.record_undo();
                                    self.dragging_wire_waypoint = Some((wid, i));
                                    return;
                                }
                            }
                        }
                    }

                    // Otherwise hit-test and start dragging connector or board
                    let hit = self.hit_test(world_pos, grid_pos);
                    self.selected = hit.clone();
                    match hit {
                        Some(SelectedItem::Connector(cid)) => {
                            if let Some(anchor) = self
                                .project
                                .connectors
                                .iter()
                                .find(|c| c.id == cid)
                                .map(|c| c.anchor)
                            {
                                self.record_undo();
                                self.dragging_connector = Some((
                                    cid,
                                    GridPos::new(
                                        grid_pos.col - anchor.col,
                                        grid_pos.row - anchor.row,
                                    ),
                                ));
                            }
                        }
                        Some(SelectedItem::Board(bid)) => {
                            if let Some(origin) = self
                                .project
                                .boards
                                .iter()
                                .find(|b| b.id == bid)
                                .map(|b| b.origin)
                            {
                                self.record_undo();
                                self.dragging_board = Some((bid, world_pos, origin));
                            }
                        }
                        _ => {}
                    }
                }

                if response.dragged_by(egui::PointerButton::Primary) {
                    if let Some((wid, wp_idx)) = self.dragging_wire_waypoint {
                        if let Some(w) = self.project.wires.iter_mut().find(|w| w.id == wid) {
                            if let Some(wp) = w.waypoints.get_mut(wp_idx) {
                                *wp = if self.wire_snap_waypoints_half_grid {
                                    world_pos.snap_half_grid()
                                } else {
                                    world_pos
                                };
                            }
                        }
                    } else if let Some((cid, offset)) = self.dragging_connector {
                        let new_anchor =
                            GridPos::new(grid_pos.col - offset.col, grid_pos.row - offset.row);
                        if let Some(c) = self.project.connectors.iter().find(|c| c.id == cid).cloned()
                        {
                            let mut candidate = c;
                            candidate.anchor = new_anchor;
                            if self
                                .project
                                .validate_connector_placement(&candidate, Some(cid))
                                .is_ok()
                            {
                                if let Some(target) =
                                    self.project.connectors.iter_mut().find(|c| c.id == cid)
                                {
                                    target.anchor = new_anchor;
                                }
                            }
                        }
                    } else if let Some((bid, start_world, start_origin)) = self.dragging_board {
                        let dcol = (world_pos.x - start_world.x).round() as i32;
                        let drow = (world_pos.y - start_world.y).round() as i32;
                        if let Some(b) = self.project.boards.iter_mut().find(|b| b.id == bid) {
                            b.origin = GridPos::new(start_origin.col + dcol, start_origin.row + drow);
                        }
                    }
                }

                if response.drag_stopped() {
                    self.dragging_board = None;
                    self.dragging_connector = None;
                    self.dragging_wire_waypoint = None;
                }
            }

            ToolMode::MoveBoard => {
                if response.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(b) = self.project.find_board_at_world(world_pos) {
                        let bid = b.id;
                        let orig = b.origin;
                        self.selected = Some(SelectedItem::Board(bid));
                        self.record_undo();
                        self.dragging_board = Some((bid, world_pos, orig));
                    }
                }
                if response.dragged_by(egui::PointerButton::Primary) {
                    if let Some((bid, start_world, start_origin)) = self.dragging_board {
                        let dcol = (world_pos.x - start_world.x).round() as i32;
                        let drow = (world_pos.y - start_world.y).round() as i32;
                        if let Some(b) = self.project.boards.iter_mut().find(|b| b.id == bid) {
                            b.origin = GridPos::new(start_origin.col + dcol, start_origin.row + drow);
                        }
                    }
                }
                if response.drag_stopped() {
                    self.dragging_board = None;
                }
            }

            ToolMode::PlaceSinglePin
            | ToolMode::PlaceHeaderLine
            | ToolMode::PlaceLibraryFootprint => {
                if response.clicked_by(egui::PointerButton::Primary) {
                    if let Some(mut new_conn) = self.build_preview_connector(grid_pos) {
                        match self.project.validate_connector_placement(&new_conn, None) {
                            Ok(()) => {
                                self.record_undo();
                                let prefix = match &new_conn.kind {
                                    ConnectorKind::SinglePin { .. } => "P",
                                    ConnectorKind::HeaderLine { .. } => "H",
                                    ConnectorKind::LibraryFootprint { footprint_id } => {
                                        get_footprint_by_id(footprint_id)
                                            .map(|f| f.designator_prefix)
                                            .unwrap_or("J")
                                    }
                                };
                                new_conn.id = Uuid::new_v4();
                                new_conn.designator = self.project.next_designator(prefix);
                                let id = new_conn.id;
                                self.project.connectors.push(new_conn);
                                self.selected = Some(SelectedItem::Connector(id));
                                self.status_toast =
                                    Some(("Placed component on perfboard".to_string(), false));
                            }
                            Err(conflicts) => {
                                let (_, first_reason) = &conflicts[0];
                                self.status_toast = Some((
                                    format!("Cannot place here: {}", first_reason),
                                    true,
                                ));
                            }
                        }
                    }
                }
            }

            ToolMode::DrawWire => {
                if response.clicked_by(egui::PointerButton::Primary) {
                    let occ = self.project.hole_occupancy(grid_pos);
                    if let Some(start_hole) = self.active_wire_start {
                        // Clicking on a valid board hole finishes the wire (unless Shift is held to force an intermediate waypoint)
                        let shift_held = ui.input(|i| i.modifiers.shift);
                        if occ.board_id.is_some()
                            && occ.blocked_by_housing.is_none()
                            && !shift_held
                            && grid_pos != start_hole
                            && world_pos_near_grid_center(world_pos)
                        {
                            self.record_undo();
                            let wire = Wire {
                                id: Uuid::new_v4(),
                                label: String::new(),
                                start_hole,
                                end_hole: grid_pos,
                                waypoints: std::mem::take(&mut self.active_wire_waypoints),
                                mode: self.wire_mode,
                                color: self.wire_color,
                                thickness: 1.0,
                                layer: self.wire_layer,
                            };
                            let id = wire.id;
                            self.project.wires.push(wire);
                            self.active_wire_start = None;
                            self.selected = Some(SelectedItem::Wire(id));
                            self.status_toast = Some((
                                "Completed wire connection. Click another hole to start a new wire."
                                    .to_string(),
                                false,
                            ));
                        } else {
                            // Add an arbitrary waypoint in world space!
                            let wp = if self.wire_snap_waypoints_half_grid {
                                world_pos.snap_half_grid()
                            } else {
                                world_pos
                            };
                            self.active_wire_waypoints.push(wp);
                        }
                    } else {
                        // Starting a new wire: must start on a board hole that isn't blocked by a housing overhang
                        if occ.board_id.is_none() {
                            self.status_toast = Some((
                                "Wires must start and end at a perfboard hole".to_string(),
                                true,
                            ));
                        } else if let Some((_, ref des)) = occ.blocked_by_housing {
                            self.status_toast = Some((
                                format!("Hole is covered by {} housing overhang", des),
                                true,
                            ));
                        } else {
                            self.active_wire_start = Some(grid_pos);
                            self.active_wire_waypoints.clear();
                            self.status_toast = Some((
                                "Wire started: click anywhere to add waypoints (Straight or Spline), click a destination hole to finish."
                                    .to_string(),
                                false,
                            ));
                        }
                    }
                }
            }

            ToolMode::DrawSolder => {
                // Paint solder line across adjacent holes by dragging or clicking
                if response.drag_started_by(egui::PointerButton::Primary)
                    || response.clicked_by(egui::PointerButton::Primary)
                {
                    let occ = self.project.hole_occupancy(grid_pos);
                    if occ.can_place_solder() {
                        if let Some(&last) = self.active_solder_nodes.last() {
                            if last == grid_pos {
                                // Double-click / second click on same hole finishes solder trace
                                self.commit_active_solder_line();
                            } else if last.is_neighbor(grid_pos) {
                                self.active_solder_nodes.push(grid_pos);
                            } else {
                                // Automatically step Manhattan/diagonal neighbors if on board
                                self.extend_solder_nodes_toward(grid_pos);
                            }
                        } else {
                            self.active_solder_nodes.push(grid_pos);
                        }
                    }
                }

                if response.dragged_by(egui::PointerButton::Primary)
                    && !self.active_solder_nodes.is_empty()
                {
                    self.extend_solder_nodes_toward(grid_pos);
                }

                if response.drag_stopped_by(egui::PointerButton::Primary)
                    && self.active_solder_nodes.len() >= 2
                {
                    self.commit_active_solder_line();
                }
            }

            ToolMode::CutTrack => {
                if response.clicked_by(egui::PointerButton::Primary) {
                    if let Some(board) = self
                        .project
                        .boards
                        .iter_mut()
                        .find(|b| b.contains_global_hole(grid_pos))
                    {
                        if let Some(local) = board.global_to_local(grid_pos) {
                            if let Some(pos) = board.track_cuts.iter().position(|&p| p == local) {
                                board.track_cuts.remove(pos);
                                self.status_toast =
                                    Some(("Restored stripboard track at hole".to_string(), false));
                            } else {
                                board.track_cuts.push(local);
                                self.status_toast =
                                    Some(("Added stripboard track cut at hole".to_string(), false));
                            }
                        }
                    }
                }
            }

            ToolMode::Eraser => {
                if response.clicked_by(egui::PointerButton::Primary) {
                    if let Some(hit) = self.hit_test(world_pos, grid_pos) {
                        if !matches!(hit, SelectedItem::Board(_)) {
                            self.selected = Some(hit);
                            self.delete_selected();
                        }
                    }
                }
            }
        }
    }

    fn extend_solder_nodes_toward(&mut self, target: GridPos) {
        let Some(&last) = self.active_solder_nodes.last() else {
            return;
        };
        if last == target {
            return;
        }
        if last.is_neighbor(target) && self.project.hole_occupancy(target).can_place_solder() {
            if self.active_solder_nodes.len() >= 2
                && self.active_solder_nodes[self.active_solder_nodes.len() - 2] == target
            {
                // Backtrack if user drags backward
                self.active_solder_nodes.pop();
            } else if !self.active_solder_nodes.contains(&target) {
                self.active_solder_nodes.push(target);
            }
        }
    }

    fn commit_active_solder_line(&mut self) {
        if self.active_solder_nodes.len() >= 2 {
            self.record_undo();
            let nodes = std::mem::take(&mut self.active_solder_nodes);
            let line = SolderLine::new(nodes);
            let id = line.id;
            self.project.solder_lines.push(line);
            self.selected = Some(SelectedItem::SolderLine(id));
            self.status_toast = Some((
                "Created solder trace (covered holes are now blocked for new pins, and can join other solder lines)"
                    .to_string(),
                false,
            ));
        } else {
            self.active_solder_nodes.clear();
        }
    }

    fn hit_test(&self, world_pos: WorldPos, grid_pos: GridPos) -> Option<SelectedItem> {
        // 1. Check wires
        for wire in self.project.wires.iter().rev() {
            if wire.distance_to_point(world_pos) < 0.36 {
                return Some(SelectedItem::Wire(wire.id));
            }
        }
        // 2. Check connectors
        for conn in self.project.connectors.iter().rev() {
            if conn.contains_world_pos(world_pos)
                || conn.pin_positions().iter().any(|(p, _)| *p == grid_pos)
            {
                return Some(SelectedItem::Connector(conn.id));
            }
        }
        // 3. Check solder lines
        for solder in self.project.solder_lines.iter().rev() {
            if solder.distance_to_point(world_pos) < 0.38 {
                return Some(SelectedItem::SolderLine(solder.id));
            }
        }
        // 4. Check boards
        if let Some(board) = self.project.find_board_at_world(world_pos) {
            return Some(SelectedItem::Board(board.id));
        }
        None
    }

    fn draw_canvas_hud_overlay(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        hole: GridPos,
    ) {
        let occ = self.project.hole_occupancy(hole);
        let board_info = if let Some(board) = self.project.find_board_at_hole(hole) {
            if let Some(local) = board.global_to_local(hole) {
                format!("{} • Hole {}", board.name, board.format_hole_coord(local))
            } else {
                board.name.clone()
            }
        } else {
            format!("Plane Grid ({}, {})", hole.col, hole.row)
        };

        let mut status_parts = Vec::new();
        if let Some((_, des, pin)) = &occ.pin_owner {
            status_parts.push(format!("Pin: {} #{}", des, pin));
        }
        if let Some((_, des)) = &occ.blocked_by_housing {
            status_parts.push(format!("Blocked ({} Overhang)", des));
        }
        if !occ.solder_lines.is_empty() {
            status_parts.push(format!(
                "Soldered ({} trace{}) — Blocks new pins",
                occ.solder_lines.len(),
                if occ.solder_lines.len() > 1 { "s" } else { "" }
            ));
        }
        if !occ.wire_endpoints.is_empty() {
            status_parts.push(format!("Wires: {}", occ.wire_endpoints.len()));
        }

        let detail = if status_parts.is_empty() {
            if occ.board_id.is_some() {
                "Free Hole".to_string()
            } else {
                "Off-Board".to_string()
            }
        } else {
            status_parts.join("  |  ")
        };

        let text = format!("{}   —   {}", board_info, detail);
        let hud_pos = egui::pos2(canvas_rect.min.x + 12.0, canvas_rect.min.y + 12.0);
        let galley = painter.layout_no_wrap(
            text,
            FontId::monospace(12.0),
            Color32::from_rgb(225, 232, 245),
        );
        let bg_rect = egui::Rect::from_min_size(
            hud_pos,
            galley.size() + egui::vec2(16.0, 10.0),
        );
        painter.rect_filled(
            bg_rect,
            5.0,
            Color32::from_rgba_unmultiplied(24, 27, 34, 215),
        );
        painter.rect_stroke(
            bg_rect,
            5.0,
            Stroke::new(1.0_f32, Color32::from_rgb(56, 64, 78)),
            egui::StrokeKind::Inside,
        );
        painter.galley(
            hud_pos + egui::vec2(8.0, 5.0),
            galley,
            Color32::from_rgb(225, 232, 245),
        );
    }

    fn render_modals(&mut self, ctx: &egui::Context) {
        if self.show_new_board_modal {
            egui::Window::new("Add Perfboard / Stripboard")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Board Name:");
                        ui.text_edit_singleline(&mut self.new_board_name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Columns (X):");
                        ui.add(egui::DragValue::new(&mut self.new_board_cols).range(2..=120));
                        ui.label("Rows (Y):");
                        ui.add(egui::DragValue::new(&mut self.new_board_rows).range(2..=120));
                    });
                    egui::ComboBox::from_label("Column Identifier Style")
                        .selected_text(self.new_board_col_style.display_name())
                        .show_ui(ui, |ui| {
                            for s in AxisLabelStyle::ALL {
                                ui.selectable_value(
                                    &mut self.new_board_col_style,
                                    s,
                                    s.display_name(),
                                );
                            }
                        });
                    egui::ComboBox::from_label("Row Identifier Style")
                        .selected_text(self.new_board_row_style.display_name())
                        .show_ui(ui, |ui| {
                            for s in AxisLabelStyle::ALL {
                                ui.selectable_value(
                                    &mut self.new_board_row_style,
                                    s,
                                    s.display_name(),
                                );
                            }
                        });
                    egui::ComboBox::from_label("Substrate Type")
                        .selected_text(self.new_board_kind.display_name())
                        .show_ui(ui, |ui| {
                            for k in BoardKind::ALL {
                                ui.selectable_value(&mut self.new_board_kind, k, k.display_name());
                            }
                        });
                    egui::ComboBox::from_label("Board Color Theme")
                        .selected_text(self.new_board_theme.display_name())
                        .show_ui(ui, |ui| {
                            for t in BoardTheme::ALL {
                                ui.selectable_value(&mut self.new_board_theme, t, t.display_name());
                            }
                        });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Create Board").clicked() {
                            self.record_undo();
                            // Place next to existing boards automatically
                            let offset_col = self
                                .project
                                .boards
                                .iter()
                                .map(|b| b.origin.col + b.cols as i32 + 4)
                                .max()
                                .unwrap_or(0);
                            let mut board = Perfboard::new(
                                self.new_board_name.clone(),
                                GridPos::new(offset_col, 0),
                                self.new_board_cols,
                                self.new_board_rows,
                            );
                            board.col_label_style = self.new_board_col_style;
                            board.row_label_style = self.new_board_row_style;
                            board.kind = self.new_board_kind;
                            board.theme = self.new_board_theme;
                            let id = board.id;
                            self.project.boards.push(board);
                            self.selected = Some(SelectedItem::Board(id));
                            self.show_new_board_modal = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_new_board_modal = false;
                        }
                    });
                });
        }

        if self.show_json_modal {
            egui::Window::new("Project JSON Import / Export")
                .collapsible(false)
                .resizable(true)
                .default_size(egui::vec2(520.0, 380.0))
                .show(ctx, |ui| {
                    ui.label("Copy this JSON to save/share your layout, or paste JSON below and click Load:");
                    egui::ScrollArea::vertical()
                        .max_height(280.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.json_buffer)
                                    .font(FontId::monospace(11.5))
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(16),
                            );
                        });
                    ui.horizontal(|ui| {
                        if ui.button("Load from JSON").clicked() {
                            match serde_json::from_str::<Project>(&self.json_buffer) {
                                Ok(proj) => {
                                    self.record_undo();
                                    self.project = proj;
                                    self.selected = None;
                                    self.show_json_modal = false;
                                    self.status_toast =
                                        Some(("Loaded project from JSON".to_string(), false));
                                }
                                Err(e) => {
                                    self.status_toast =
                                        Some((format!("Invalid JSON: {}", e), true));
                                }
                            }
                        }
                        if ui.button("Close").clicked() {
                            self.show_json_modal = false;
                        }
                    });
                });
        }
    }
}

fn world_pos_near_grid_center(w: WorldPos) -> bool {
    let dx = (w.x - w.x.round()).abs();
    let dy = (w.y - w.y.round()).abs();
    dx < 0.42 && dy < 0.42
}

fn configure_modern_cad_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = Color32::from_rgb(28, 31, 38);
    visuals.panel_fill = Color32::from_rgb(26, 29, 36);
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(33, 37, 46);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(38, 43, 54);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 56, 70);
    visuals.widgets.active.bg_fill = Color32::from_rgb(42, 118, 196);
    visuals.selection.bg_fill = Color32::from_rgb(42, 118, 196);
    ctx.set_visuals(visuals);
}
