use crate::icons::{toolbar_icon_button, CadIcon};
use egui::{Align2, Color32, CornerRadius, FontId, Margin, Pos2, Rect, RichText, Stroke};
use uuid::Uuid;

use crate::canvas::{
    handle_viewport_navigation, render_workspace, CanvasRenderPreview, ControlScheme, SelectedItem,
    Viewport,
};
use crate::library::{
    get_board_preset_by_id, get_footprint_by_id, standard_board_library, standard_library,
    FootprintCategory,
};
use crate::model::{
    AxisLabelStyle, BoardKind, BoardTheme, ConnectorKind, GridPos, Perfboard, PlacedConnector,
    Project, RgbColor, Rotation, SolderLine, Wire, WireLayer, WireMode, WorldPos, PITCH_MM,
};

/// Active interactive tool in the CAD workspace
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolMode {
    /// Select, inspect, and move boards, connectors, waypoints, or solder traces
    Select,
    MoveSelection,
    /// Move Perfboards freely on the 2D plane
    MoveBoard,
    /// Place a Perfboard / Stripboard / Breadboard from the library
    PlaceBoard,
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
            ToolMode::Select => "Select",
            ToolMode::MoveSelection => "Move selection",
            ToolMode::MoveBoard => "Move Board",
            ToolMode::PlaceBoard => "Place Board",
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
            ToolMode::MoveSelection => "M / Esc",
            ToolMode::MoveBoard => "M",
            ToolMode::PlaceBoard => "A",
            ToolMode::PlaceSinglePin => "P",
            ToolMode::PlaceHeaderLine => "H",
            ToolMode::PlaceLibraryFootprint => "A",
            ToolMode::DrawWire => "W",
            ToolMode::DrawSolder => "S",
            ToolMode::CutTrack => "X",
            ToolMode::Eraser => "D / Del",
        }
    }
}

/// Tabs in the unified Add / Library Dialog (`A` or Right-Click -> Add...)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddDialogTab {
    Boards,
    PinsAndHeaders,
    Components,
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
    /// Supports single and multi-selection (e.g. `Ctrl+A` / `Cmd+A` Select All, `Shift+Click` toggle)
    pub selected: Vec<SelectedItem>,
    clipboard: Vec<PlacedConnector>,
    pending_transform: Option<(Project, Project, GridPos)>,
    last_pointer_grid: GridPos,
    pub ghost_mode: bool,

    // Tool configuration state
    pub active_rotation: Rotation,
    pub single_pin_square: bool,
    pub header_cols: u32,
    pub header_rows: u32,
    pub header_female: bool,
    pub selected_footprint_id: String,

    // Board placement configuration (part of standard library)
    pub selected_board_preset_id: String,
    pub new_board_name: String,
    pub new_board_cols: u32,
    pub new_board_rows: u32,
    pub new_board_col_style: AxisLabelStyle,
    pub new_board_row_style: AxisLabelStyle,
    pub new_board_kind: BoardKind,
    pub new_board_theme: BoardTheme,

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
    pub right_press_pos: Option<Pos2>,

    // View toggles
    pub show_x_ray_blocked_holes: bool,
    pub show_pin_labels: bool,
    pub show_net_highlights: bool,
    pub show_inspector: bool,

    // Unified Add / Library Dialog (`A` or Right-Click -> Add)
    pub show_add_dialog: bool,
    pub add_dialog_tab: AddDialogTab,
    pub add_dialog_search: String,

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
            selected: Vec::new(),
            clipboard: Vec::new(),
            pending_transform: None,
            last_pointer_grid: GridPos::new(0, 0),
            ghost_mode: false,

            active_rotation: Rotation::Deg0,
            single_pin_square: false,
            header_cols: 4,
            header_rows: 1,
            header_female: false,
            selected_footprint_id: "jst_xh_3".to_string(),

            selected_board_preset_id: "board_perf_24x18".to_string(),
            new_board_name: "Perfboard".to_string(),
            new_board_cols: 24,
            new_board_rows: 18,
            new_board_col_style: AxisLabelStyle::Numbers1Based,
            new_board_row_style: AxisLabelStyle::LettersUpper,
            new_board_kind: BoardKind::Perfboard,
            new_board_theme: BoardTheme::ClassicGreen,

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
            right_press_pos: None,

            show_x_ray_blocked_holes: true,
            show_pin_labels: true,
            show_net_highlights: true,
            show_inspector: true,

            show_add_dialog: false,
            add_dialog_tab: AddDialogTab::Components,
            add_dialog_search: String::new(),

            show_json_modal: false,
            json_buffer: String::new(),
            status_toast: Some((
                "Press 'A' or Right-Click to Add Boards & Components  •  Ctrl+A Select All"
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

    fn cancel_transform(&mut self) {
        if let Some((before, _, _)) = self.pending_transform.take() {
            self.project = before;
            self.selected.clear();
        }
        self.tool = ToolMode::Select;
    }

    fn start_move(&mut self) {
        self.active_wire_start = None;
        self.active_wire_waypoints.clear();
        self.active_solder_nodes.clear();
        if self.selected.is_empty() {
            self.tool = ToolMode::MoveBoard;
            return;
        }
        if self.pending_transform.is_none() {
            self.pending_transform = Some((
                self.project.clone(),
                self.project.clone(),
                self.last_pointer_grid,
            ));
        }
        self.tool = ToolMode::MoveSelection;
        self.status_toast = Some((
            "Move selection: click to place, Esc to cancel".into(),
            false,
        ));
    }

    pub fn copy_components(&mut self) {
        self.clipboard = self
            .project
            .connectors
            .iter()
            .filter(|c| self.selected.contains(&SelectedItem::Connector(c.id)))
            .cloned()
            .collect();
        self.status_toast = Some((
            format!("Copied {} component(s)", self.clipboard.len()),
            false,
        ));
    }

    pub fn paste_components(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        if self.pending_transform.is_some() {
            self.cancel_transform();
        }
        let before = self.project.clone();
        let anchor = self.clipboard[0].anchor;
        self.selected.clear();
        for mut component in self.clipboard.clone() {
            component.id = Uuid::new_v4();
            let prefix = match &component.kind {
                ConnectorKind::SinglePin { .. } => "P",
                ConnectorKind::HeaderLine { .. } => "H",
                ConnectorKind::LibraryFootprint { footprint_id } => {
                    get_footprint_by_id(footprint_id)
                        .map(|fp| fp.designator_prefix)
                        .unwrap_or("J")
                }
            };
            component.designator = self.project.next_designator(prefix);
            self.selected.push(SelectedItem::Connector(component.id));
            self.project.connectors.push(component);
        }
        self.pending_transform = Some((before, self.project.clone(), anchor));
        self.tool = ToolMode::MoveSelection;
        self.status_toast = Some((
            "Paste: move copies to free holes and click to place; Esc cancels".into(),
            false,
        ));
    }

    fn update_transform(&mut self, target: GridPos) {
        let Some((_, baseline, anchor)) = &self.pending_transform else {
            return;
        };
        self.project = baseline.clone();
        let dc = target.col - anchor.col;
        let dr = target.row - anchor.row;
        for board in &mut self.project.boards {
            if self.selected.contains(&SelectedItem::Board(board.id)) {
                board.origin = board.origin.add(dc, dr);
            }
        }
        for component in &mut self.project.connectors {
            if self
                .selected
                .contains(&SelectedItem::Connector(component.id))
            {
                component.anchor = component.anchor.add(dc, dr);
            }
        }
        for wire in &mut self.project.wires {
            if self.selected.contains(&SelectedItem::Wire(wire.id)) {
                wire.start_hole = wire.start_hole.add(dc, dr);
                wire.end_hole = wire.end_hole.add(dc, dr);
                for point in &mut wire.waypoints {
                    point.x += dc as f32;
                    point.y += dr as f32;
                }
            }
        }
        for solder in &mut self.project.solder_lines {
            if self.selected.contains(&SelectedItem::SolderLine(solder.id)) {
                for point in &mut solder.nodes {
                    *point = point.add(dc, dr);
                }
            }
        }
    }

    fn commit_transform(&mut self) -> bool {
        if self.pending_transform.is_none() {
            return false;
        }
        let bad_wire = self.project.wires.iter().any(|w| {
            self.selected.contains(&SelectedItem::Wire(w.id))
                && (self.project.find_board_at_hole(w.start_hole).is_none()
                    || self.project.find_board_at_hole(w.end_hole).is_none())
        });
        let bad_solder = self.project.solder_lines.iter().any(|s| {
            self.selected.contains(&SelectedItem::SolderLine(s.id))
                && s.nodes
                    .iter()
                    .any(|p| self.project.find_board_at_hole(*p).is_none())
        });
        if bad_wire || bad_solder {
            self.status_toast = Some((
                "Wire ends and solder nodes must stay on board holes".into(),
                true,
            ));
            return false;
        }
        // Validate against the other items, including other members of the moving group.
        for component in &self.project.connectors {
            if self
                .selected
                .contains(&SelectedItem::Connector(component.id))
            {
                let mut validation_project = self.project.clone();
                // Remove self rather than ignoring its ID: hole_occupancy reports one
                // owner, so self could otherwise hide an overlapping existing pin.
                validation_project
                    .connectors
                    .retain(|c| c.id != component.id);
                // Existing soldered pins may remain attached when moving a whole assembly.
                validation_project
                    .solder_lines
                    .retain(|s| !self.selected.contains(&SelectedItem::SolderLine(s.id)));
                if let Err(conflicts) =
                    validation_project.validate_connector_placement(component, None)
                {
                    self.status_toast = Some((format!("Cannot place: {}", conflicts[0].1), true));
                    return false;
                }
            }
        }
        let (before, _, _) = self.pending_transform.take().unwrap();
        self.undo_stack.push(before);
        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.tool = ToolMode::Select;
        self.status_toast = Some(("Placed selection".into(), false));
        true
    }

    fn components_are_ghosted(&self) -> bool {
        self.ghost_mode
            || self.viewport.flipped
            || matches!(
                self.tool,
                ToolMode::DrawWire | ToolMode::DrawSolder | ToolMode::CutTrack
            )
    }

    pub fn undo(&mut self) {
        if self.pending_transform.is_some() {
            self.cancel_transform();
            return;
        }
        if let Some(prev) = self.undo_stack.pop() {
            let current = std::mem::replace(&mut self.project, prev);
            self.redo_stack.push(current);
            self.selected.clear();
            self.status_toast = Some(("Undid last action".to_string(), false));
        }
    }

    pub fn redo(&mut self) {
        if self.pending_transform.is_some() {
            self.cancel_transform();
        }
        if let Some(next) = self.redo_stack.pop() {
            let current = std::mem::replace(&mut self.project, next);
            self.undo_stack.push(current);
            self.selected.clear();
            self.status_toast = Some(("Redid action".to_string(), false));
        }
    }

    pub fn select_all(&mut self) {
        if self.pending_transform.is_some() {
            self.cancel_transform();
        }
        self.selected.clear();
        for b in &self.project.boards {
            self.selected.push(SelectedItem::Board(b.id));
        }
        for c in &self.project.connectors {
            self.selected.push(SelectedItem::Connector(c.id));
        }
        for w in &self.project.wires {
            self.selected.push(SelectedItem::Wire(w.id));
        }
        for s in &self.project.solder_lines {
            self.selected.push(SelectedItem::SolderLine(s.id));
        }
        let count = self.selected.len();
        self.status_toast = Some((format!("Selected all {} items", count), false));
    }

    pub fn clear_all(&mut self) {
        if self.pending_transform.is_some() {
            self.cancel_transform();
        }
        self.record_undo();
        self.project = Project::new_empty("Untitled Project");
        self.selected.clear();
        self.active_wire_start = None;
        self.active_wire_waypoints.clear();
        self.active_solder_nodes.clear();
        self.status_toast = Some((
            "Cleared workspace completely. Press 'A' or Right-Click → Add to place a board."
                .to_string(),
            false,
        ));
    }

    fn apply_board_preset(&mut self, preset_id: &str) {
        if let Some(preset) = get_board_preset_by_id(preset_id) {
            self.selected_board_preset_id = preset.id.to_string();
            self.new_board_name = preset.short_name.to_string();
            self.new_board_cols = preset.cols;
            self.new_board_rows = preset.rows;
            self.new_board_kind = preset.kind;
            self.new_board_theme = preset.theme;
            self.new_board_col_style = preset.col_label_style;
            self.new_board_row_style = preset.row_label_style;
        }
    }

    fn build_preview_board(&self, origin: GridPos) -> Option<Perfboard> {
        if self.tool != ToolMode::PlaceBoard {
            return None;
        }
        let mut board = Perfboard::new(
            self.new_board_name.clone(),
            origin,
            self.new_board_cols,
            self.new_board_rows,
        );
        board.col_label_style = self.new_board_col_style;
        board.row_label_style = self.new_board_row_style;
        board.kind = self.new_board_kind;
        board.theme = self.new_board_theme;
        Some(board)
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
            // Allow Escape to close the Add dialog even if search box has focus
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.show_add_dialog = false;
            }
            return;
        }

        ctx.input(|i| {
            if self.show_add_dialog || self.show_json_modal {
                if i.key_pressed(egui::Key::Escape) {
                    self.show_add_dialog = false;
                    self.show_json_modal = false;
                }
                return;
            }
            let command = i.modifiers.command || i.modifiers.ctrl;
            if i.events.iter().any(|e| matches!(e, egui::Event::Copy))
                || (command && i.key_pressed(egui::Key::C))
            {
                self.copy_components();
                return;
            }
            if i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)))
                || (command && i.key_pressed(egui::Key::V))
            {
                self.paste_components();
                return;
            }
            // Ctrl+A / Cmd+A: Select All
            if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::A) {
                if self.pending_transform.is_some() {
                    self.cancel_transform();
                }
                self.select_all();
                return;
            }

            // Plain 'A': Open the Add / Standard Library Dialog
            if !i.modifiers.command && !i.modifiers.ctrl && i.key_pressed(egui::Key::A) {
                if self.pending_transform.is_some() {
                    self.cancel_transform();
                }
                self.show_add_dialog = !self.show_add_dialog;
                return;
            }

            // Undo / Redo
            if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::Z) {
                if i.modifiers.shift {
                    self.redo();
                } else {
                    self.undo();
                }
            }

            if i.key_pressed(egui::Key::Escape) {
                if self.pending_transform.is_some() {
                    self.cancel_transform();
                    return;
                } else if self.show_add_dialog {
                    self.show_add_dialog = false;
                } else if self.active_wire_start.is_some() {
                    self.active_wire_start = None;
                    self.active_wire_waypoints.clear();
                } else if !self.active_solder_nodes.is_empty() {
                    self.active_solder_nodes.clear();
                } else if self.tool != ToolMode::Select {
                    self.tool = ToolMode::Select;
                } else {
                    self.selected.clear();
                }
            }

            if command {
                return;
            }
            if i.key_pressed(egui::Key::G) {
                self.ghost_mode = !self.ghost_mode;
            }
            if i.key_pressed(egui::Key::F) {
                self.viewport.flipped = !self.viewport.flipped;
            }
            if self.pending_transform.is_some() {
                return;
            }

            if i.key_pressed(egui::Key::R) {
                self.active_rotation = self.active_rotation.next_cw();
                if self.tool == ToolMode::PlaceBoard {
                    std::mem::swap(&mut self.new_board_cols, &mut self.new_board_rows);
                }
                if let [SelectedItem::Connector(cid)] = self.selected.as_slice() {
                    let cid = *cid;
                    if let Some(c) = self
                        .project
                        .connectors
                        .iter()
                        .find(|c| c.id == cid)
                        .cloned()
                    {
                        let mut rotated = c;
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
                self.start_move();
            }
            if i.key_pressed(egui::Key::P) {
                self.tool = ToolMode::PlaceSinglePin;
            }
            if i.key_pressed(egui::Key::H) {
                self.tool = ToolMode::PlaceHeaderLine;
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

    pub fn delete_selected(&mut self) {
        if self.pending_transform.is_some() {
            self.cancel_transform();
            return;
        }
        if self.selected.is_empty() {
            return;
        }
        self.record_undo();
        let to_remove = std::mem::take(&mut self.selected);
        for item in to_remove {
            match item {
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
        }
    }
}

impl eframe::App for PerfworkApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_keyboard_shortcuts(ctx);

        // 1. Minimal Standard Window Menu Bar (File, Edit, View)
        egui::TopBottomPanel::top("window_menu_bar")
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_rgb(24, 27, 33))
                    .inner_margin(Margin::symmetric(12, 4))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(40, 44, 54))),
            )
            .show(ctx, |ui| {
                self.render_window_menu_bar(ui, ctx);
            });

        // 2. Bottom Status Bar with KiCad-inspired Control Scheme Dropdown, Coordinates & Zoom
        egui::TopBottomPanel::bottom("bottom_status_bar")
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_rgb(24, 27, 33))
                    .inner_margin(Margin::symmetric(12, 5))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(40, 44, 54))),
            )
            .show(ctx, |ui| {
                self.render_bottom_status_bar(ui, ctx);
            });

        // 3. Full-Window Interactive 2D CAD Canvas
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(20, 22, 27)))
            .show(ctx, |ui| {
                self.render_center_canvas(ui);
            });

        // 4. Autodesk Fusion-Style Floating Icon Toolbar at Top-Center of the Canvas
        self.render_floating_fusion_toolbar(ctx);

        // 5. Compact Floating Property Inspector Card (top-right, only when an item or active tool needs options)
        if self.show_inspector {
            self.render_floating_inspector(ctx);
        }

        // 6. Modals (Unified Add / Library Dialog & JSON Import/Export)
        self.render_modals(ctx);
    }
}

impl PerfworkApp {
    /// Clean, standard application menu bar at the top of the window
    fn render_window_menu_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::menu::bar(ui, |ui| {
            ui.label(
                RichText::new("PERFWORK")
                    .font(FontId::proportional(13.5))
                    .strong()
                    .color(Color32::from_rgb(88, 202, 255)),
            );
            ui.add_space(8.0);

            ui.menu_button("File", |ui| {
                if ui.button("Load Demo Circuit").clicked() {
                    if self.pending_transform.is_some() {
                        self.cancel_transform();
                    }
                    self.record_undo();
                    self.project = Project::new_starter();
                    self.selected.clear();
                    ui.close_menu();
                }
                if ui.button("Clear All (Empty Workspace)").clicked() {
                    self.clear_all();
                    ui.close_menu();
                }
                ui.separator();
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if ui.button("Open Project...").clicked() {
                        ui.close_menu();
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Perfwork Project", &["json"])
                            .pick_file()
                        {
                            if let Ok(data) = std::fs::read_to_string(&path) {
                                if let Ok(proj) = serde_json::from_str::<Project>(&data) {
                                    if self.pending_transform.is_some() {
                                        self.cancel_transform();
                                    }
                                    self.record_undo();
                                    self.project = proj;
                                    self.selected.clear();
                                    self.status_toast =
                                        Some((format!("Loaded {}", path.display()), false));
                                }
                            }
                        }
                    }
                    if ui.button("Save Project...").clicked() {
                        ui.close_menu();
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Perfwork Project", &["perfwork.json", "json"])
                            .set_file_name("circuit.perfwork.json")
                            .save_file()
                        {
                            if let Ok(json) = serde_json::to_string_pretty(&self.project) {
                                if std::fs::write(&path, json).is_ok() {
                                    self.status_toast = Some((
                                        format!("Saved project to {}", path.display()),
                                        false,
                                    ));
                                }
                            }
                        }
                    }
                    ui.separator();
                }
                if ui.button("Import / Export JSON...").clicked() {
                    self.json_buffer =
                        serde_json::to_string_pretty(&self.project).unwrap_or_default();
                    self.show_json_modal = true;
                    ui.close_menu();
                }
            });

            ui.menu_button("Edit", |ui| {
                if ui
                    .add_enabled(
                        !self.undo_stack.is_empty(),
                        egui::Button::new("Undo (Ctrl+Z)"),
                    )
                    .clicked()
                {
                    self.undo();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        !self.redo_stack.is_empty(),
                        egui::Button::new("Redo (Ctrl+Shift+Z)"),
                    )
                    .clicked()
                {
                    self.redo();
                    ui.close_menu();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        !self.selected.is_empty(),
                        egui::Button::new("Move selection (M)"),
                    )
                    .clicked()
                {
                    self.start_move();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        !self.selected.is_empty(),
                        egui::Button::new("Copy components (Ctrl+C)"),
                    )
                    .clicked()
                {
                    self.copy_components();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        !self.clipboard.is_empty(),
                        egui::Button::new("Paste components (Ctrl+V)"),
                    )
                    .clicked()
                {
                    self.paste_components();
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Add Board / Component... (A)").clicked() {
                    self.show_add_dialog = true;
                    ui.close_menu();
                }
                if ui.button("Select All (Ctrl+A)").clicked() {
                    self.select_all();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        !self.selected.is_empty(),
                        egui::Button::new("Delete Selected (Del)"),
                    )
                    .clicked()
                {
                    self.delete_selected();
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Clear All").clicked() {
                    self.clear_all();
                    ui.close_menu();
                }
            });

            ui.menu_button("View", |ui| {
                if ui.button("Fit All Boards").clicked() {
                    let screen = ctx.screen_rect();
                    self.viewport.fit_project(screen, &self.project);
                    ui.close_menu();
                }
                ui.separator();
                ui.checkbox(&mut self.show_inspector, "Show Floating Inspector");
                ui.checkbox(
                    &mut self.show_x_ray_blocked_holes,
                    "Highlight Blocked Overhang Holes",
                );
                ui.checkbox(&mut self.show_pin_labels, "Show Pin Labels");
                ui.checkbox(&mut self.ghost_mode, "Ghost components (G)");
                ui.checkbox(&mut self.viewport.flipped, "Solder-side view (F)");
            });

            // Quick right-side actions in the window bar
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(&self.project.name)
                        .size(12.0)
                        .color(Color32::from_rgb(150, 160, 178)),
                );
            });
        });
    }

    /// Autodesk Fusion-inspired floating icon dock suspended at the top-center of the workspace
    fn render_floating_fusion_toolbar(&mut self, ctx: &egui::Context) {
        egui::Area::new(egui::Id::new("fusion_floating_toolbar"))
            .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 42.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(30, 34, 42, 242))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(10, 6))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(58, 66, 82)))
                    .shadow(egui::Shadow {
                        offset: [0, 4],
                        blur: 14,
                        spread: 0,
                        color: Color32::from_rgba_unmultiplied(0, 0, 0, 110),
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;

                            // 1. Primary "Add" button (opens the unified Add Board / Component dialog, shortcut A)
                            if toolbar_icon_button(ui, CadIcon::Add, "Add (A)", self.show_add_dialog, true)
                                .on_hover_text(
                                    "Open Library to Add Perfboards, Pins, Headers, or Components (Shortcut: A or Right-Click)",
                                )
                                .clicked()
                            {
                                if self.pending_transform.is_some() { self.cancel_transform(); }
                                self.show_add_dialog = !self.show_add_dialog;
                            }

                            ui.separator();

                            // 2. Core CAD Tool Icons
                            let tools = [
                                (
                                    ToolMode::Select,
                                    CadIcon::Select,
                                    "Select",
                                    "Select, move, or inspect items (V)\nCtrl+A selects all",
                                ),
                                (
                                    ToolMode::MoveSelection,
                                    CadIcon::Move,
                                    "Move",
                                    "Move selection (M); without a selection, drag a board",
                                ),
                                (
                                    ToolMode::PlaceSinglePin,
                                    CadIcon::Pin,
                                    "Pin",
                                    "Place Single Connector Pin (P)",
                                ),
                                (
                                    ToolMode::PlaceHeaderLine,
                                    CadIcon::Header,
                                    "Header",
                                    "Place Multi-Pin Header Line (H)",
                                ),
                                (
                                    ToolMode::DrawWire,
                                    CadIcon::Wire,
                                    "Wire",
                                    "Route Wire between holes — Straight or Spline (W)",
                                ),
                                (
                                    ToolMode::DrawSolder,
                                    CadIcon::Solder,
                                    "Solder",
                                    "Draw Solder Bridge across neighbor holes (S)",
                                ),
                                (
                                    ToolMode::CutTrack,
                                    CadIcon::CutTrack,
                                    "Cut",
                                    "Cut/Restore Stripboard Copper Track (X)",
                                ),
                                (
                                    ToolMode::Eraser,
                                    CadIcon::Erase,
                                    "Erase",
                                    "Delete clicked items (D)",
                                ),
                            ];

                            for (mode, icon, caption, tooltip) in tools {
                                let active = self.tool == mode;
                                if toolbar_icon_button(ui, icon, caption, active, false).on_hover_text(tooltip).clicked() {
                                    if self.pending_transform.is_some() { self.cancel_transform(); }
                                    if mode == ToolMode::MoveSelection { self.start_move(); } else { self.tool = mode; }
                                    self.active_wire_start = None;
                                    self.active_wire_waypoints.clear();
                                    self.active_solder_nodes.clear();
                                }
                            }

                            ui.separator();
                            if toolbar_icon_button(ui, CadIcon::Ghost, "Ghost (G)", self.components_are_ghosted(), false)
                                .on_hover_text("See through components (G). Automatic while routing wires or solder.").clicked() {
                                self.ghost_mode = !self.ghost_mode;
                            }
                            if toolbar_icon_button(ui, CadIcon::FlipView, "Flip (F)", self.viewport.flipped, false)
                                .on_hover_text("Turn the layout over: top / mirrored solder-side view (F)").clicked() {
                                self.viewport.flipped = !self.viewport.flipped;
                            }

                            // 3. Contextual mini-controls inside the floating dock when a placement/routing tool is active
                            if self.tool == ToolMode::DrawWire {
                                ui.separator();
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.selectable_value(
                                            &mut self.wire_mode,
                                            WireMode::Straight,
                                            "Straight",
                                        );
                                        ui.selectable_value(
                                            &mut self.wire_mode,
                                            WireMode::Spline,
                                            "Spline",
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        for (name, col) in WIRE_PALETTE {
                                            let sel = self.wire_color == *col;
                                            let (rect, resp) = ui.allocate_exact_size(
                                                egui::vec2(15.0, 14.0),
                                                egui::Sense::click(),
                                            );
                                            ui.painter().rect_filled(rect, 3.0, col.to_color32());
                                            if sel {
                                                ui.painter().rect_stroke(
                                                    rect,
                                                    3.0,
                                                    Stroke::new(1.8_f32, Color32::WHITE),
                                                    egui::StrokeKind::Inside,
                                                );
                                            }
                                            if resp.on_hover_text(*name).clicked() {
                                                self.wire_color = *col;
                                            }
                                        }
                                    });
                                });
                            } else if self.tool == ToolMode::PlaceHeaderLine {
                                ui.separator();
                                ui.horizontal(|ui| {
                                    ui.label("Pins:");
                                    ui.add(
                                        egui::DragValue::new(&mut self.header_cols).range(1..=40),
                                    );
                                    ui.label("×");
                                    ui.add(
                                        egui::DragValue::new(&mut self.header_rows).range(1..=4),
                                    );
                                    if ui
                                        .button(format!("Rotate {}°", self.active_rotation.degrees()))
                                        .clicked()
                                    {
                                        self.active_rotation = self.active_rotation.next_cw();
                                    }
                                });
                            } else if matches!(
                                self.tool,
                                ToolMode::PlaceSinglePin
                                    | ToolMode::PlaceLibraryFootprint
                                    | ToolMode::PlaceBoard
                            ) {
                                ui.separator();
                                if ui
                                    .button(format!("Rotate ({}°)", self.active_rotation.degrees()))
                                    .on_hover_text("Press R to rotate 90°")
                                    .clicked()
                                {
                                    self.active_rotation = self.active_rotation.next_cw();
                                    if self.tool == ToolMode::PlaceBoard {
                                        std::mem::swap(
                                            &mut self.new_board_cols,
                                            &mut self.new_board_rows,
                                        );
                                    }
                                }
                            }
                        });
                    });
            });
    }

    /// Compact floating inspector card in the top-right corner when item(s) are selected
    fn render_floating_inspector(&mut self, ctx: &egui::Context) {
        if self.selected.is_empty() {
            return;
        }

        egui::Area::new(egui::Id::new("floating_inspector_card"))
            .anchor(Align2::RIGHT_TOP, egui::vec2(-14.0, 42.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(28, 32, 40, 242))
                    .corner_radius(CornerRadius::same(9))
                    .inner_margin(Margin::same(12))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(56, 64, 80)))
                    .shadow(egui::Shadow {
                        offset: [0, 4],
                        blur: 12,
                        spread: 0,
                        color: Color32::from_rgba_unmultiplied(0, 0, 0, 100),
                    })
                    .show(ui, |ui| {
                        ui.set_width(265.0);
                        match self.selected.as_slice() {
                            [SelectedItem::Board(board_id)] => {
                                self.render_board_inspector(ui, *board_id);
                            }
                            [SelectedItem::Connector(conn_id)] => {
                                self.render_connector_inspector(ui, *conn_id);
                            }
                            [SelectedItem::Wire(wire_id)] => {
                                self.render_wire_inspector(ui, *wire_id);
                            }
                            [SelectedItem::SolderLine(solder_id)] => {
                                self.render_solder_inspector(ui, *solder_id);
                            }
                            many => {
                                ui.label(
                                    RichText::new(format!("{} ITEMS SELECTED", many.len()))
                                        .size(11.5)
                                        .strong()
                                        .color(Color32::from_rgb(110, 208, 255)),
                                );
                                ui.add_space(6.0);
                                let boards_n = many
                                    .iter()
                                    .filter(|i| matches!(i, SelectedItem::Board(_)))
                                    .count();
                                let conns_n = many
                                    .iter()
                                    .filter(|i| matches!(i, SelectedItem::Connector(_)))
                                    .count();
                                let wires_n = many
                                    .iter()
                                    .filter(|i| matches!(i, SelectedItem::Wire(_)))
                                    .count();
                                let solder_n = many
                                    .iter()
                                    .filter(|i| matches!(i, SelectedItem::SolderLine(_)))
                                    .count();
                                ui.label(format!(
                                    "• Boards: {}\n• Components: {}\n• Wires: {}\n• Solder Traces: {}",
                                    boards_n, conns_n, wires_n, solder_n
                                ));
                                ui.add_space(8.0);
                                if ui
                                    .button(
                                        RichText::new("Delete All Selected (Del)")
                                            .color(Color32::from_rgb(255, 110, 110)),
                                    )
                                    .clicked()
                                {
                                    self.delete_selected();
                                }
                            }
                        }
                    });
            });
    }

    fn render_board_inspector(&mut self, ui: &mut egui::Ui, board_id: Uuid) {
        let Some(idx) = self.project.boards.iter().position(|b| b.id == board_id) else {
            return;
        };

        let mut delete_clicked = false;
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
        ui.horizontal(|ui| {
            ui.label("Cols (X):");
            ui.add(egui::DragValue::new(&mut board.cols).range(2..=120));
            ui.label("Rows (Y):");
            ui.add(egui::DragValue::new(&mut board.rows).range(2..=120));
        });
        ui.label(
            RichText::new(format!(
                "Size: {:.1} × {:.1} mm  |  Origin: ({}, {})",
                (board.cols as f32 + 1.5) * PITCH_MM,
                (board.rows as f32 + 1.5) * PITCH_MM,
                board.origin.col,
                board.origin.row
            ))
            .size(11.0)
            .color(Color32::from_rgb(148, 158, 175)),
        );

        ui.add_space(6.0);
        egui::ComboBox::from_label("Col Labels")
            .selected_text(board.col_label_style.display_name())
            .show_ui(ui, |ui| {
                for style in AxisLabelStyle::ALL {
                    ui.selectable_value(&mut board.col_label_style, style, style.display_name());
                }
            });

        egui::ComboBox::from_label("Row Labels")
            .selected_text(board.row_label_style.display_name())
            .show_ui(ui, |ui| {
                for style in AxisLabelStyle::ALL {
                    ui.selectable_value(&mut board.row_label_style, style, style.display_name());
                }
            });

        ui.checkbox(&mut board.labels_both_sides, "Labels on All 4 Edges");

        ui.add_space(4.0);
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

        ui.add_space(6.0);
        if ui
            .button(RichText::new("Delete Board").color(Color32::from_rgb(255, 110, 110)))
            .clicked()
        {
            delete_clicked = true;
        }

        if delete_clicked {
            self.delete_selected();
        }
    }

    fn render_connector_inspector(&mut self, ui: &mut egui::Ui, conn_id: Uuid) {
        let Some(idx) = self.project.connectors.iter().position(|c| c.id == conn_id) else {
            return;
        };

        let mut delete_clicked = false;
        let mut rotate_clicked = false;

        ui.label(
            RichText::new("COMPONENT PROPERTIES")
                .size(11.5)
                .strong()
                .color(Color32::from_rgb(110, 208, 255)),
        );
        ui.add_space(6.0);

        let conn = &mut self.project.connectors[idx];
        ui.horizontal(|ui| {
            ui.label("Ref:");
            ui.text_edit_singleline(&mut conn.designator);
        });
        ui.horizontal(|ui| {
            ui.label("Label:");
            ui.text_edit_singleline(&mut conn.label);
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(format!("Rotation: {}°", conn.rotation.degrees()));
            if ui.small_button("Rotate (R)").clicked() {
                rotate_clicked = true;
            }
        });

        if let ConnectorKind::HeaderLine { cols, rows, female } = &mut conn.kind {
            ui.separator();
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
                    "Overhang blocks {} adjacent hole(s)",
                    blocked.len()
                ))
                .size(11.0)
                .color(Color32::from_rgb(255, 168, 92)),
            );
        }

        ui.add_space(6.0);
        if ui
            .button(RichText::new("Delete Component").color(Color32::from_rgb(255, 110, 110)))
            .clicked()
        {
            delete_clicked = true;
        }

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
        ui.label(
            RichText::new("WIRE PROPERTIES")
                .size(11.5)
                .strong()
                .color(Color32::from_rgb(110, 208, 255)),
        );
        ui.add_space(6.0);

        let wire = &mut self.project.wires[idx];
        ui.horizontal(|ui| {
            ui.label("Label:");
            ui.text_edit_singleline(&mut wire.label);
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Mode:");
            ui.selectable_value(&mut wire.mode, WireMode::Straight, "Straight");
            ui.selectable_value(&mut wire.mode, WireMode::Spline, "Spline");
        });

        ui.add_space(4.0);
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
            ui.label("Width:");
            ui.add(egui::Slider::new(&mut wire.thickness, 0.6..=2.2).step_by(0.1));
        });

        if !wire.waypoints.is_empty() && ui.small_button("Clear Intermediate Bends").clicked() {
            wire.waypoints.clear();
        }

        ui.add_space(6.0);
        if ui
            .button(RichText::new("Delete Wire").color(Color32::from_rgb(255, 110, 110)))
            .clicked()
        {
            delete_clicked = true;
        }

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
        ui.label(
            RichText::new("SOLDER TRACE")
                .size(11.5)
                .strong()
                .color(Color32::from_rgb(110, 208, 255)),
        );
        ui.add_space(6.0);

        let solder = &mut self.project.solder_lines[idx];
        ui.label(format!("Spans {} connected holes", solder.nodes.len()));
        ui.horizontal(|ui| {
            ui.label("Width:");
            ui.add(egui::Slider::new(&mut solder.width_scale, 0.7..=1.5).step_by(0.05));
        });

        ui.add_space(6.0);
        if ui
            .button(RichText::new("Remove Solder Trace").color(Color32::from_rgb(255, 110, 110)))
            .clicked()
        {
            delete_clicked = true;
        }

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
                            "{} ({})",
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
                egui::ComboBox::from_id_salt("status_bar_control_scheme")
                    .selected_text(
                        RichText::new(self.control_scheme.display_name())
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
                ui.label(if self.viewport.flipped { "SOLDER SIDE" } else { "TOP SIDE" });

                let zoom_pct = (self.viewport.pixels_per_pitch / 27.0 * 100.0).round() as i32;
                if ui.small_button("Fit").on_hover_text("Fit all boards in view").clicked() {
                    let screen = ctx.screen_rect();
                    self.viewport.fit_project(screen, &self.project);
                }
                if ui.small_button("+").clicked() {
                    let screen = ctx.screen_rect();
                    self.viewport
                        .zoom_at_screen_pos(screen, screen.center(), 1.2);
                }
                ui.label(
                    RichText::new(format!("{}%", zoom_pct))
                        .monospace()
                        .size(11.5),
                );
                if ui.small_button("-").clicked() {
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

        let pointer_screen = response
            .hover_pos()
            .or_else(|| ui.input(|i| i.pointer.latest_pos()));
        let pointer_world = pointer_screen.map(|p| self.viewport.screen_to_world(canvas_rect, p));
        let hovered_hole = pointer_world.map(|w| w.snap_to_grid());
        if response.hovered() && !self.show_add_dialog && !self.show_json_modal {
            if let Some(hole) = hovered_hole {
                self.last_pointer_grid = hole;
            }
        }

        // Track right-mouse press position so a quick Right-Click opens the context menu (Add...)
        // whereas a Right-Drag pans the canvas smoothly in Mouse mode
        if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary)) {
            self.right_press_pos = pointer_screen;
        }

        // Context Menu on stationary Right-Click when in Select mode
        if self.tool == ToolMode::Select
            && self.active_wire_start.is_none()
            && self.active_solder_nodes.is_empty()
        {
            response.context_menu(|ui| {
                ui.set_min_width(195.0);
                if ui.button("Add Board / Component... (A)").clicked() {
                    self.show_add_dialog = true;
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Place Single Pin (P)").clicked() {
                    self.tool = ToolMode::PlaceSinglePin;
                    ui.close_menu();
                }
                if ui.button("Place Header Line (H)").clicked() {
                    self.tool = ToolMode::PlaceHeaderLine;
                    ui.close_menu();
                }
                if ui.button("Route Wire (W)").clicked() {
                    self.tool = ToolMode::DrawWire;
                    ui.close_menu();
                }
                if ui.button("Draw Solder Trace (S)").clicked() {
                    self.tool = ToolMode::DrawSolder;
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Select All (Ctrl+A)").clicked() {
                    self.select_all();
                    ui.close_menu();
                }
                if !self.selected.is_empty() && ui.button("Delete Selected (Del)").clicked() {
                    self.delete_selected();
                    ui.close_menu();
                }
                if ui.button("Clear All").clicked() {
                    self.clear_all();
                    ui.close_menu();
                }
            });
        }

        // Handle Canvas Interactions per Tool Mode
        if !self.show_add_dialog && !self.show_json_modal {
            if let (Some(world_pos), Some(grid_pos)) = (pointer_world, hovered_hole) {
                self.handle_canvas_interaction(ui, &response, world_pos, grid_pos);
            }
        }

        // Build Ghost Previews if in a Placement tool
        let ghost_board = hovered_hole.and_then(|h| self.build_preview_board(h));
        let ghost_connectors: Vec<_> = hovered_hole
            .and_then(|h| self.build_preview_connector(h))
            .into_iter()
            .collect();

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
                ghost_board: ghost_board.as_ref(),
                ghost_connectors: &ghost_connectors,
                ghost_components: self.components_are_ghosted(),
                active_wire_in_progress: wire_preview,
                active_solder_in_progress: solder_preview,
                hovered_hole,
                show_x_ray_blocked_holes: self.show_x_ray_blocked_holes,
                show_pin_labels: self.show_pin_labels,
                show_net_highlights: self.show_net_highlights,
            },
        );

        // Draw HUD Coordinate & Hole Occupancy Pill in the bottom-left of the canvas
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

        // Right-click cancels active wire/solder drawing or exits placement tool back to Select
        if response.clicked_by(egui::PointerButton::Secondary) {
            if self.pending_transform.is_some() {
                self.cancel_transform();
                return;
            }
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
            if self.tool != ToolMode::Select {
                self.tool = ToolMode::Select;
                return;
            }
        }

        match self.tool {
            ToolMode::MoveSelection => {
                if response.hovered() {
                    self.update_transform(grid_pos);
                }
                if response.clicked_by(egui::PointerButton::Primary) {
                    self.commit_transform();
                }
            }
            ToolMode::Select => {
                if response.clicked_by(egui::PointerButton::Primary) {
                    let shift = ui.input(|i| i.modifiers.shift);
                    if let Some(hit) = self.hit_test(world_pos, grid_pos) {
                        if shift {
                            if let Some(pos) = self.selected.iter().position(|x| *x == hit) {
                                self.selected.remove(pos);
                            } else {
                                self.selected.push(hit);
                            }
                        } else {
                            self.selected = vec![hit];
                        }
                    } else if !shift {
                        self.selected.clear();
                    }
                }

                if response.drag_started_by(egui::PointerButton::Primary) {
                    // Check if user grabbed a wire waypoint handle first
                    if let [SelectedItem::Wire(wid)] = self.selected.as_slice() {
                        let wid = *wid;
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
                    if let Some(hit) = self.hit_test(world_pos, grid_pos) {
                        if !self.selected.contains(&hit) {
                            self.selected = vec![hit.clone()];
                        }
                        match hit {
                            SelectedItem::Connector(cid) => {
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
                            SelectedItem::Board(bid) => {
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
                        if let Some(c) = self
                            .project
                            .connectors
                            .iter()
                            .find(|c| c.id == cid)
                            .cloned()
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
                            b.origin =
                                GridPos::new(start_origin.col + dcol, start_origin.row + drow);
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
                        self.selected = vec![SelectedItem::Board(bid)];
                        self.record_undo();
                        self.dragging_board = Some((bid, world_pos, orig));
                    }
                }
                if response.dragged_by(egui::PointerButton::Primary) {
                    if let Some((bid, start_world, start_origin)) = self.dragging_board {
                        let dcol = (world_pos.x - start_world.x).round() as i32;
                        let drow = (world_pos.y - start_world.y).round() as i32;
                        if let Some(b) = self.project.boards.iter_mut().find(|b| b.id == bid) {
                            b.origin =
                                GridPos::new(start_origin.col + dcol, start_origin.row + drow);
                        }
                    }
                }
                if response.drag_stopped() {
                    self.dragging_board = None;
                }
            }

            ToolMode::PlaceBoard => {
                if response.clicked_by(egui::PointerButton::Primary) {
                    if let Some(board) = self.build_preview_board(grid_pos) {
                        self.record_undo();
                        let id = board.id;
                        self.project.boards.push(board);
                        self.selected = vec![SelectedItem::Board(id)];
                        self.status_toast = Some((
                            "Placed board on plane. Click again to place another, or press Esc / Right-Click to finish."
                                .to_string(),
                            false,
                        ));
                    }
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
                                self.selected = vec![SelectedItem::Connector(id)];
                                self.status_toast =
                                    Some(("Placed component on perfboard".to_string(), false));
                            }
                            Err(conflicts) => {
                                let (_, first_reason) = &conflicts[0];
                                self.status_toast =
                                    Some((format!("Cannot place here: {}", first_reason), true));
                            }
                        }
                    }
                }
            }

            ToolMode::DrawWire => {
                if response.clicked_by(egui::PointerButton::Primary) {
                    let occ = self.project.hole_occupancy(grid_pos);
                    if let Some(start_hole) = self.active_wire_start {
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
                            self.selected = vec![SelectedItem::Wire(id)];
                            self.status_toast = Some((
                                "Completed wire connection. Click another hole to start a new wire."
                                    .to_string(),
                                false,
                            ));
                        } else {
                            let wp = if self.wire_snap_waypoints_half_grid {
                                world_pos.snap_half_grid()
                            } else {
                                world_pos
                            };
                            self.active_wire_waypoints.push(wp);
                        }
                    } else if occ.board_id.is_none() {
                        self.status_toast = Some((
                            "Wires must start and end at a perfboard hole".to_string(),
                            true,
                        ));
                    } else if let Some((_, ref des)) = occ.blocked_by_housing {
                        self.status_toast =
                            Some((format!("Hole is covered by {} housing overhang", des), true));
                    } else {
                        self.active_wire_start = Some(grid_pos);
                        self.active_wire_waypoints.clear();
                        self.status_toast = Some((
                            "Wire started: click anywhere to add waypoints, click a destination hole to finish."
                                .to_string(),
                            false,
                        ));
                    }
                }
            }

            ToolMode::DrawSolder => {
                if response.drag_started_by(egui::PointerButton::Primary)
                    || response.clicked_by(egui::PointerButton::Primary)
                {
                    let occ = self.project.hole_occupancy(grid_pos);
                    if occ.can_place_solder() {
                        if let Some(&last) = self.active_solder_nodes.last() {
                            if last == grid_pos {
                                self.commit_active_solder_line();
                            } else if last.is_neighbor(grid_pos) {
                                self.active_solder_nodes.push(grid_pos);
                            } else {
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
                            self.selected = vec![hit];
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
            self.selected = vec![SelectedItem::SolderLine(id)];
            self.status_toast = Some((
                "Created solder trace (covered holes block new pins, and can join other solder lines)"
                    .to_string(),
                false,
            ));
        } else {
            self.active_solder_nodes.clear();
        }
    }

    fn hit_test(&self, world_pos: WorldPos, grid_pos: GridPos) -> Option<SelectedItem> {
        if self.components_are_ghosted() {
            // Solder-side / X-ray inspection must be able to reach copper under a housing.
            for solder in self.project.solder_lines.iter().rev() {
                if solder.distance_to_point(world_pos) < 0.38 {
                    return Some(SelectedItem::SolderLine(solder.id));
                }
            }
        }
        for wire in self.project.wires.iter().rev() {
            if wire.distance_to_point(world_pos) < 0.36 {
                return Some(SelectedItem::Wire(wire.id));
            }
        }
        for conn in self.project.connectors.iter().rev() {
            if conn.contains_world_pos(world_pos)
                || conn.pin_positions().iter().any(|(p, _)| *p == grid_pos)
            {
                return Some(SelectedItem::Connector(conn.id));
            }
        }
        for solder in self.project.solder_lines.iter().rev() {
            if solder.distance_to_point(world_pos) < 0.38 {
                return Some(SelectedItem::SolderLine(solder.id));
            }
        }
        if let Some(board) = self.project.find_board_at_world(world_pos) {
            return Some(SelectedItem::Board(board.id));
        }
        None
    }

    fn draw_canvas_hud_overlay(&self, painter: &egui::Painter, canvas_rect: Rect, hole: GridPos) {
        let occ = self.project.hole_occupancy(hole);
        let board_info = if let Some(board) = self.project.find_board_at_hole(hole) {
            if let Some(local) = board.global_to_local(hole) {
                format!("{} • Hole {}", board.name, board.format_hole_coord(local))
            } else {
                board.name.clone()
            }
        } else {
            format!("Grid ({}, {})", hole.col, hole.row)
        };

        let mut status_parts = Vec::new();
        if let Some((_, des, pin)) = &occ.pin_owner {
            status_parts.push(format!("Pin: {} #{}", des, pin));
        }
        if let Some((_, des)) = &occ.blocked_by_housing {
            status_parts.push(format!("Blocked ({} Overhang)", des));
        }
        if !occ.solder_lines.is_empty() {
            status_parts.push(format!("Soldered ({})", occ.solder_lines.len()));
        }
        if !occ.wire_endpoints.is_empty() {
            status_parts.push(format!("Wires: {}", occ.wire_endpoints.len()));
        }

        let detail = if status_parts.is_empty() {
            if occ.board_id.is_some() {
                "Free Hole".to_string()
            } else {
                "Empty Plane".to_string()
            }
        } else {
            status_parts.join("  |  ")
        };

        let text = format!("{}  —  {}", board_info, detail);
        let galley = painter.layout_no_wrap(
            text,
            FontId::monospace(11.5),
            Color32::from_rgb(218, 226, 240),
        );
        let hud_pos = egui::pos2(
            canvas_rect.min.x + 14.0,
            canvas_rect.max.y - galley.size().y - 22.0,
        );
        let bg_rect = egui::Rect::from_min_size(hud_pos, galley.size() + egui::vec2(16.0, 10.0));
        painter.rect_filled(
            bg_rect,
            6.0,
            Color32::from_rgba_unmultiplied(24, 27, 34, 210),
        );
        painter.rect_stroke(
            bg_rect,
            6.0,
            Stroke::new(1.0_f32, Color32::from_rgb(52, 60, 74)),
            egui::StrokeKind::Inside,
        );
        painter.galley(
            hud_pos + egui::vec2(8.0, 5.0),
            galley,
            Color32::from_rgb(218, 226, 240),
        );
    }

    fn render_modals(&mut self, ctx: &egui::Context) {
        if self.show_add_dialog && self.pending_transform.is_some() {
            self.cancel_transform();
        }
        // Unified "Add to Workspace" Dialog (Triggered via 'A', Right-Click -> Add, or Toolbar '+')
        if self.show_add_dialog {
            let mut open = self.show_add_dialog;
            let mut close_after_selection = false;

            egui::Window::new("Add to Workspace (Standard Library)")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_size(egui::vec2(560.0, 440.0))
                .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    // Category Tabs: Boards | Pins & Headers | Connectors & Components
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.add_dialog_tab,
                            AddDialogTab::Boards,
                            "Boards",
                        );
                        ui.selectable_value(
                            &mut self.add_dialog_tab,
                            AddDialogTab::PinsAndHeaders,
                            "Pins & Headers",
                        );
                        ui.selectable_value(
                            &mut self.add_dialog_tab,
                            AddDialogTab::Components,
                            "Components",
                        );
                    });

                    ui.separator();

                    match self.add_dialog_tab {
                        AddDialogTab::Boards => {
                            ui.label(
                                RichText::new("Select a standard board from the library or customize its dimensions, then click Place on Plane:")
                                    .size(12.0)
                                    .color(Color32::from_rgb(168, 178, 196)),
                            );
                            ui.add_space(6.0);

                            // Standard Board Presets Grid
                            let presets = standard_board_library();
                            egui::ScrollArea::vertical()
                                .max_height(175.0)
                                .show(ui, |ui| {
                                    for preset in presets {
                                        let is_sel = self.selected_board_preset_id == preset.id;
                                        egui::Frame::NONE
                                            .fill(if is_sel {
                                                Color32::from_rgb(38, 72, 112)
                                            } else {
                                                Color32::from_rgb(34, 38, 48)
                                            })
                                            .corner_radius(CornerRadius::same(6))
                                            .inner_margin(Margin::same(8))
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    let choice = ui
                                                        .selectable_label(
                                                            is_sel,
                                                            RichText::new(preset.name).strong(),
                                                        );
                                                    if choice.clicked() {
                                                        self.apply_board_preset(preset.id);
                                                    }
                                                    if choice.double_clicked() {
                                                        self.apply_board_preset(preset.id);
                                                        self.tool = ToolMode::PlaceBoard;
                                                        close_after_selection = true;
                                                    }
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            if ui
                                                                .small_button("Place Board →")
                                                                .clicked()
                                                            {
                                                                self.apply_board_preset(preset.id);
                                                                self.tool = ToolMode::PlaceBoard;
                                                                close_after_selection = true;
                                                            }
                                                        },
                                                    );
                                                });
                                                ui.label(
                                                    RichText::new(preset.description)
                                                        .size(11.5)
                                                        .color(Color32::from_rgb(150, 160, 178)),
                                                );
                                            });
                                        ui.add_space(4.0);
                                    }
                                });

                            ui.separator();
                            ui.label(
                                RichText::new("Customize Board Before Placing:")
                                    .strong()
                                    .size(12.5),
                            );
                            ui.add_space(4.0);

                            egui::Grid::new("custom_board_grid")
                                .num_columns(2)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    ui.label("Board Name:");
                                    ui.text_edit_singleline(&mut self.new_board_name);
                                    ui.end_row();

                                    ui.label("Dimensions (Cols × Rows):");
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::DragValue::new(&mut self.new_board_cols)
                                                .range(2..=120)
                                                .prefix("Cols: "),
                                        );
                                        ui.add(
                                            egui::DragValue::new(&mut self.new_board_rows)
                                                .range(2..=120)
                                                .prefix("Rows: "),
                                        );
                                    });
                                    ui.end_row();

                                    ui.label("Column / Row Identifiers:");
                                    ui.horizontal(|ui| {
                                        egui::ComboBox::from_id_salt("add_col_style")
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
                                        egui::ComboBox::from_id_salt("add_row_style")
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
                                    });
                                    ui.end_row();

                                    ui.label("Substrate & Theme:");
                                    ui.horizontal(|ui| {
                                        egui::ComboBox::from_id_salt("add_board_kind")
                                            .selected_text(self.new_board_kind.display_name())
                                            .show_ui(ui, |ui| {
                                                for k in BoardKind::ALL {
                                                    ui.selectable_value(
                                                        &mut self.new_board_kind,
                                                        k,
                                                        k.display_name(),
                                                    );
                                                }
                                            });
                                        egui::ComboBox::from_id_salt("add_board_theme")
                                            .selected_text(self.new_board_theme.display_name())
                                            .show_ui(ui, |ui| {
                                                for t in BoardTheme::ALL {
                                                    ui.selectable_value(
                                                        &mut self.new_board_theme,
                                                        t,
                                                        t.display_name(),
                                                    );
                                                }
                                            });
                                    });
                                    ui.end_row();
                                });

                            ui.add_space(8.0);
                            if ui
                                .button(
                                    RichText::new("Place Custom Board on Plane")
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .clicked()
                            {
                                self.tool = ToolMode::PlaceBoard;
                                close_after_selection = true;
                            }
                        }

                        AddDialogTab::PinsAndHeaders => {
                            ui.label(
                                RichText::new("Choose a single connector pin or parametric multi-pin header line:")
                                    .size(12.0)
                                    .color(Color32::from_rgb(168, 178, 196)),
                            );
                            ui.add_space(8.0);

                            egui::Frame::NONE
                                .fill(Color32::from_rgb(34, 38, 48))
                                .corner_radius(CornerRadius::same(6))
                                .inner_margin(Margin::same(10))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        if ui.selectable_label(false, "Single Connector Pin (1×1)").double_clicked() {
                                            self.tool = ToolMode::PlaceSinglePin;
                                            close_after_selection = true;
                                        }
                                        ui.checkbox(&mut self.single_pin_square, "Square Pad");
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui.button("Place Single Pin →").clicked() {
                                                    self.tool = ToolMode::PlaceSinglePin;
                                                    close_after_selection = true;
                                                }
                                            },
                                        );
                                    });
                                });

                            ui.add_space(8.0);

                            egui::Frame::NONE
                                .fill(Color32::from_rgb(34, 38, 48))
                                .corner_radius(CornerRadius::same(6))
                                .inner_margin(Margin::same(10))
                                .show(ui, |ui| {
                                    if ui.selectable_label(false,
                                        RichText::new("Parametric Pin Header Line (N×M)")
                                            .strong(),
                                    ).double_clicked() {
                                        self.tool = ToolMode::PlaceHeaderLine;
                                        close_after_selection = true;
                                    }
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        ui.label("Columns (Length):");
                                        ui.add(
                                            egui::DragValue::new(&mut self.header_cols)
                                                .range(1..=40),
                                        );
                                        ui.label("Rows:");
                                        ui.add(
                                            egui::DragValue::new(&mut self.header_rows)
                                                .range(1..=4),
                                        );
                                        ui.checkbox(&mut self.header_female, "Female Socket");
                                    });

                                    ui.add_space(6.0);
                                    ui.label("Quick Presets:");
                                    ui.horizontal_wrapped(|ui| {
                                        for (c, r, lbl) in [
                                            (2, 1, "1×2 Header"),
                                            (3, 1, "1×3 Header"),
                                            (4, 1, "1×4 Header"),
                                            (6, 1, "1×6 Header"),
                                            (8, 1, "1×8 Header"),
                                            (10, 1, "1×10 Header"),
                                            (4, 2, "2×4 Dual Header"),
                                            (5, 2, "2×5 Dual Header"),
                                            (8, 2, "2×8 Dual Header"),
                                        ] {
                                            if ui.button(lbl).clicked() {
                                                self.header_cols = c;
                                                self.header_rows = r;
                                                self.tool = ToolMode::PlaceHeaderLine;
                                                close_after_selection = true;
                                            }
                                        }
                                    });

                                    ui.add_space(8.0);
                                    if ui
                                        .button(
                                            RichText::new(format!(
                                                "Place {}×{} Header Line →",
                                                self.header_cols, self.header_rows
                                            ))
                                            .strong(),
                                        )
                                        .clicked()
                                    {
                                        self.tool = ToolMode::PlaceHeaderLine;
                                        close_after_selection = true;
                                    }
                                });
                        }

                        AddDialogTab::Components => {
                            ui.horizontal(|ui| {
                                ui.label("Filter:");
                                ui.text_edit_singleline(&mut self.add_dialog_search);
                                if !self.add_dialog_search.is_empty()
                                    && ui.small_button("x").clicked()
                                {
                                    self.add_dialog_search.clear();
                                }
                            });
                            ui.add_space(6.0);

                            let query = self.add_dialog_search.to_lowercase();
                            let lib = standard_library();

                            egui::ScrollArea::vertical()
                                .max_height(310.0)
                                .show(ui, |ui| {
                                    for category in FootprintCategory::ALL {
                                        let matching: Vec<_> = lib
                                            .iter()
                                            .filter(|f| f.category == category)
                                            .filter(|f| {
                                                query.is_empty()
                                                    || f.name.to_lowercase().contains(&query)
                                                    || f.short_name.to_lowercase().contains(&query)
                                                    || f.description.to_lowercase().contains(&query)
                                            })
                                            .collect();

                                        if matching.is_empty() {
                                            continue;
                                        }

                                        ui.label(
                                            RichText::new(category.display_name())
                                                .size(12.0)
                                                .strong()
                                                .color(Color32::from_rgb(120, 195, 250)),
                                        );
                                        ui.add_space(4.0);

                                        for fp in matching {
                                            egui::Frame::NONE
                                                .fill(Color32::from_rgb(34, 38, 48))
                                                .corner_radius(CornerRadius::same(6))
                                                .inner_margin(Margin::same(8))
                                                .show(ui, |ui| {
                                                    ui.horizontal(|ui| {
                                                        if ui.selectable_label(self.selected_footprint_id == fp.id,
                                                            RichText::new(fp.name).strong(),
                                                        ).double_clicked() {
                                                            self.selected_footprint_id = fp.id.to_string();
                                                            self.tool = ToolMode::PlaceLibraryFootprint;
                                                            close_after_selection = true;
                                                        }
                                                        ui.with_layout(
                                                            egui::Layout::right_to_left(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                if ui
                                                                    .button("Place →")
                                                                    .clicked()
                                                                {
                                                                    self.selected_footprint_id =
                                                                        fp.id.to_string();
                                                                    self.tool = ToolMode::PlaceLibraryFootprint;
                                                                    close_after_selection = true;
                                                                }
                                                            },
                                                        );
                                                    });
                                                    ui.label(
                                                        RichText::new(fp.description)
                                                            .size(11.5)
                                                            .color(Color32::from_rgb(150, 160, 178)),
                                                    );
                                                });
                                            ui.add_space(4.0);
                                        }
                                        ui.add_space(6.0);
                                    }
                                });
                        }
                    }
                });

            if close_after_selection {
                open = false;
            }
            self.show_add_dialog = open;
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
                                    self.selected.clear();
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

#[cfg(test)]
mod interaction_tests {
    use super::*;

    fn fixture() -> PerfworkApp {
        let mut app = PerfworkApp {
            project: Project::new_empty("Test"),
            ..Default::default()
        };
        app.project
            .boards
            .push(Perfboard::new("Board", GridPos::new(0, 0), 24, 18));
        app.tool = ToolMode::PlaceSinglePin;
        let mut pin = app.build_preview_connector(GridPos::new(2, 2)).unwrap();
        pin.id = Uuid::new_v4();
        pin.designator = "P1".into();
        app.selected = vec![SelectedItem::Connector(pin.id)];
        app.project.connectors.push(pin);
        app.tool = ToolMode::Select;
        app.last_pointer_grid = GridPos::new(2, 2);
        app
    }

    #[test]
    fn move_selection_is_atomic_undoable_and_cancellable() {
        let mut app = fixture();
        app.start_move();
        assert_eq!(app.tool, ToolMode::MoveSelection);
        app.update_transform(GridPos::new(5, 6));
        assert_eq!(app.project.connectors[0].anchor, GridPos::new(5, 6));
        app.update_transform(GridPos::new(8, 7)); // Relative to original, not accumulated.
        assert_eq!(app.project.connectors[0].anchor, GridPos::new(8, 7));
        assert!(app.commit_transform());
        assert_eq!(app.undo_stack.len(), 1);
        app.undo();
        assert_eq!(app.project.connectors[0].anchor, GridPos::new(2, 2));
        app.redo();
        assert_eq!(app.project.connectors[0].anchor, GridPos::new(8, 7));
        app.selected = vec![SelectedItem::Connector(app.project.connectors[0].id)];
        app.start_move();
        app.update_transform(GridPos::new(10, 10));
        app.cancel_transform();
        assert_eq!(app.project.connectors[0].anchor, GridPos::new(8, 7));
    }

    #[test]
    fn paste_preserves_original_and_checks_collisions() {
        let mut app = fixture();
        let original = app.project.connectors[0].clone();
        app.copy_components();
        app.paste_components();
        assert_eq!(app.project.connectors.len(), 2);
        assert_ne!(app.project.connectors[1].id, original.id);
        assert!(!app.commit_transform()); // Still overlaps original.
        app.update_transform(GridPos::new(7, 8));
        assert!(app.commit_transform());
        assert_eq!(app.project.connectors[0].anchor, original.anchor);
        assert_eq!(app.project.connectors[1].anchor, GridPos::new(7, 8));
        app.undo();
        assert_eq!(app.project.connectors.len(), 1);
        app.paste_components();
        app.cancel_transform();
        assert_eq!(app.project.connectors.len(), 1);
    }

    #[test]
    fn multi_component_paste_retains_spacing_and_rejects_offboard() {
        let mut app = fixture();
        let mut other = app.project.connectors[0].clone();
        other.id = Uuid::new_v4();
        other.anchor = GridPos::new(4, 2);
        app.selected.push(SelectedItem::Connector(other.id));
        app.project.connectors.push(other);
        app.copy_components();
        app.paste_components();
        app.update_transform(GridPos::new(-3, -3));
        assert!(!app.commit_transform());
        app.update_transform(GridPos::new(8, 8));
        assert_eq!(app.project.connectors[2].anchor, GridPos::new(8, 8));
        assert_eq!(app.project.connectors[3].anchor, GridPos::new(10, 8));
        assert!(app.commit_transform());
    }

    #[test]
    fn ghost_is_automatic_without_overwriting_manual_preference() {
        let mut app = fixture();
        assert!(!app.components_are_ghosted());
        for tool in [ToolMode::DrawWire, ToolMode::DrawSolder, ToolMode::CutTrack] {
            app.tool = tool;
            assert!(app.components_are_ghosted());
            assert!(!app.ghost_mode);
        }
        app.tool = ToolMode::Select;
        assert!(!app.components_are_ghosted());
        app.ghost_mode = true;
        assert!(app.components_are_ghosted());
        app.ghost_mode = false;
        app.viewport.flipped = true;
        assert!(app.components_are_ghosted());
    }

    fn key(app: &mut PerfworkApp, key: egui::Key, modifiers: egui::Modifiers) {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            modifiers,
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| app.handle_keyboard_shortcuts(ctx));
    }

    #[test]
    fn keyboard_move_copy_paste_ghost_and_flip() {
        let mut app = fixture();
        let ctrl = egui::Modifiers {
            ctrl: true,
            ..Default::default()
        };
        key(&mut app, egui::Key::C, ctrl);
        assert_eq!(app.clipboard.len(), 1);
        key(&mut app, egui::Key::V, ctrl);
        assert_eq!(app.tool, ToolMode::MoveSelection); // Not swallowed by plain V select shortcut.
        assert_eq!(app.project.connectors.len(), 2);
        key(&mut app, egui::Key::Escape, egui::Modifiers::NONE);
        assert_eq!(app.project.connectors.len(), 1);
        app.selected = vec![SelectedItem::Connector(app.project.connectors[0].id)];
        key(&mut app, egui::Key::M, egui::Modifiers::NONE);
        assert!(app.pending_transform.is_some());
        key(&mut app, egui::Key::G, egui::Modifiers::NONE);
        assert!(app.ghost_mode);
        key(&mut app, egui::Key::F, egui::Modifiers::NONE);
        assert!(app.viewport.flipped);
    }

    #[test]
    fn library_entries_double_click_to_place_and_keep_place_buttons() {
        for (tab, entry, expected, button) in [
            (
                AddDialogTab::Boards,
                "Standard Perfboard (24×18 — 5×7cm)",
                ToolMode::PlaceBoard,
                "Place Board →",
            ),
            (
                AddDialogTab::Components,
                "JST-XH 2-Pin Vertical (B2B-XH-A)",
                ToolMode::PlaceLibraryFootprint,
                "Place →",
            ),
            (
                AddDialogTab::PinsAndHeaders,
                "Single Connector Pin (1×1)",
                ToolMode::PlaceSinglePin,
                "Place Single Pin →",
            ),
            (
                AddDialogTab::PinsAndHeaders,
                "Parametric Pin Header Line (N×M)",
                ToolMode::PlaceHeaderLine,
                "Place 4×1 Header Line →",
            ),
        ] {
            let mut app = fixture();
            app.show_add_dialog = true;
            app.add_dialog_tab = tab;
            let ctx = egui::Context::default();
            let mut time = 0.0;
            let mut draw = |app: &mut PerfworkApp, events| {
                time += 0.05;
                ctx.run(
                    egui::RawInput {
                        time: Some(time),
                        events,
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(1400.0, 1000.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| app.render_modals(ctx),
                )
            };
            let _ = draw(&mut app, vec![]);
            let output = draw(&mut app, vec![]);
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| {
                    if let egui::Shape::Text(t) = &s.shape {
                        Some((t.galley.job.text.clone(), t.pos + t.galley.size() * 0.5))
                    } else {
                        None
                    }
                })
                .collect();
            assert!(
                texts.iter().any(|(text, _)| text == button),
                "Missing Place button: {button}"
            );
            let point = texts
                .iter()
                .find(|(text, _)| text == entry)
                .unwrap_or_else(|| panic!("Missing entry: {entry}"))
                .1;
            for click in 0..2 {
                let _ = draw(
                    &mut app,
                    vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
                let _ = draw(
                    &mut app,
                    vec![egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
                if click == 0 {
                    assert!(app.show_add_dialog, "Single click should not place {entry}");
                }
            }
            assert!(
                !app.show_add_dialog,
                "Double click should close dialog: {entry}"
            );
            assert_eq!(app.tool, expected);
        }
    }
}
