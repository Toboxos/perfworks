use crate::icons::{toolbar_icon_button, CadIcon};
use egui::{Align2, Color32, CornerRadius, FontId, Margin, Pos2, Rect, RichText, Stroke};
use uuid::Uuid;

use crate::actions::{
    context_actions, cycle_width, ActionGroup, ContextIntent, ContextScope, SOLDER_WIDTH_STEPS,
    WIRE_WIDTH_STEPS,
};
use crate::canvas::{
    handle_viewport_navigation, render_workspace, CanvasRenderPreview, ControlScheme, SelectedItem,
    Viewport,
};
use crate::library::{
    get_board_preset_by_id, get_footprint_by_id, standard_board_library, standard_library,
    FootprintCategory,
};
use crate::model::{
    AxisLabelStyle, BoardKind, BoardOutline, BoardTheme, ConnectorKind, EdgePads, GridPos,
    MountingSpec, NetLabel, Perfboard, PlacedConnector, Project, RgbColor, Rotation, SolderLine,
    Wire, WireLayer, WireMode, WorldPos, PITCH_MM,
};
use crate::netlist::{NetName, Netlist};

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

    /// Terse caption for the icon toolbar, where only ~60px is available. Distinct from
    /// `short_label`, which is the status-bar wording and has room to be descriptive.
    pub fn toolbar_label(&self) -> &'static str {
        match self {
            ToolMode::Select => "Select",
            ToolMode::MoveSelection | ToolMode::MoveBoard => "Move",
            ToolMode::PlaceBoard => "Board",
            ToolMode::PlaceSinglePin => "Pin",
            ToolMode::PlaceHeaderLine => "Header",
            ToolMode::PlaceLibraryFootprint => "Part",
            ToolMode::DrawWire => "Wire",
            ToolMode::DrawSolder => "Solder",
            ToolMode::CutTrack => "Cut",
            ToolMode::Eraser => "Erase",
        }
    }

    /// The single key that activates this tool, for toolbar captions. `shortcut_hint`
    /// stays the verbose form (`V / Esc`) for the status bar, which has room for it;
    /// deriving the caption from here keeps the toolbar from drifting off the keymap.
    pub fn primary_key(&self) -> Option<&'static str> {
        match self {
            ToolMode::Select => Some("V"),
            ToolMode::MoveSelection | ToolMode::MoveBoard => Some("M"),
            ToolMode::PlaceSinglePin => Some("P"),
            ToolMode::PlaceHeaderLine => Some("H"),
            ToolMode::DrawWire => Some("W"),
            ToolMode::DrawSolder => Some("S"),
            ToolMode::CutTrack => Some("X"),
            ToolMode::Eraser => Some("D"),
            // Reached through the Add dialog rather than a key of their own.
            ToolMode::PlaceBoard | ToolMode::PlaceLibraryFootprint => None,
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

/// One level of the thing the user clicked.
///
/// Clicking J2's SDA pin lands on a connector, a specific pin of it, the wire that ends
/// in that hole, and a net — all at once. `SelectedItem` deliberately stays single-valued
/// because it defines delete/move semantics (a pin cannot be deleted away from its
/// connector); this is the *view* of that click, outermost to innermost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextLayer {
    Connector(Uuid),
    Pin {
        connector: Uuid,
        index: usize,
    },
    Wire(Uuid),
    Solder(Uuid),
    /// Net anchor hole.
    Net(GridPos),
    Board(Uuid),
}

impl ContextLayer {
    fn heading(&self) -> &'static str {
        match self {
            ContextLayer::Connector(_) => "Component",
            ContextLayer::Pin { .. } => "Pin",
            ContextLayer::Wire(_) => "Wire",
            ContextLayer::Solder(_) => "Solder trace",
            ContextLayer::Net(_) => "Net",
            ContextLayer::Board(_) => "Board",
        }
    }
}

/// The core tools shown in the main toolbar row, in order. Captions are derived from
/// [`toolbar_caption`] so they cannot drift away from [`ToolMode::primary_key`].
pub const TOOLBAR_TOOLS: [(ToolMode, CadIcon, &str); 8] = [
    (
        ToolMode::Select,
        CadIcon::Select,
        "Select, move, or inspect items (V)\nCtrl+A selects all",
    ),
    (
        ToolMode::MoveSelection,
        CadIcon::Move,
        "Move selection (M); without a selection, drag a board",
    ),
    (
        ToolMode::PlaceSinglePin,
        CadIcon::Pin,
        "Place Single Connector Pin (P)",
    ),
    (
        ToolMode::PlaceHeaderLine,
        CadIcon::Header,
        "Place Multi-Pin Header Line (H)",
    ),
    (
        ToolMode::DrawWire,
        CadIcon::Wire,
        "Route Wire between holes — Straight or Spline (W)",
    ),
    (
        ToolMode::DrawSolder,
        CadIcon::Solder,
        "Draw Solder Bridge across neighbor holes (S)",
    ),
    (
        ToolMode::CutTrack,
        CadIcon::CutTrack,
        "Cut/Restore Stripboard Copper Track (X)",
    ),
    (ToolMode::Eraser, CadIcon::Erase, "Delete clicked items (D)"),
];

/// `Select` + `V` -> `"Select (V)"`. Tools without a key of their own keep a bare label.
pub fn toolbar_caption(mode: &ToolMode) -> String {
    match mode.primary_key() {
        Some(key) => format!("{} ({})", mode.toolbar_label(), key),
        None => mode.toolbar_label().to_string(),
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
    /// Supports single and multi-selection (e.g. `Ctrl+A` / `Cmd+A` Select All, `Shift+Click` toggle)
    pub selected: Vec<SelectedItem>,
    clipboard: Vec<PlacedConnector>,
    pending_transform: Option<(Project, Project, GridPos)>,
    last_pointer_grid: GridPos,
    pub ghost_mode: bool,

    /// Derived connectivity, rebuilt once per frame from `project`. Never authored.
    pub netlist: Netlist,
    netlist_generation: u64,
    netlist_build_ms: f32,
    /// Net to highlight, stored as a stable **anchor hole** rather than a `NetId` so it
    /// survives rebuilds, edits and undo. Resolved through the netlist each frame.
    pub highlighted_net: Option<GridPos>,
    /// Set by sidebar hover or canvas hover; lives for one frame only.
    hovered_net: Option<GridPos>,
    /// A net pinned on the canvas that the tree should scroll into view next frame.
    scroll_to_net: Option<GridPos>,
    /// A hole touched by a just-created conductor, whose net should be highlighted once
    /// the netlist has caught up. Needed because the netlist is rebuilt at the top of the
    /// *next* frame: at creation time the new wire or trace is not in it yet, so
    /// resolving its net right there would always return `None`.
    pending_highlight_hole: Option<GridPos>,
    /// World bounds to frame on the next canvas pass. Deferred because the toolbar runs
    /// after the central panel, so the canvas rect is no longer in hand when the
    /// "zoom to fit" action fires.
    zoom_request: Option<(WorldPos, WorldPos)>,
    /// Where the user last clicked in Select mode. The sidebar derives its layered panes
    /// from this, since a single `SelectedItem` cannot express "the pin inside this
    /// connector, with that wire landing on it".
    focus_point: Option<(WorldPos, GridPos)>,
    /// Tab presses stolen from egui's focus traversal by `raw_input_hook`, to be consumed
    /// by the keyboard handler. See that hook for why this detour is necessary.
    stolen_tab: bool,
    pub show_net_panel: bool,
    /// KiCad-style focus: dim everything that is not on the highlighted net.
    pub focus_net_mode: bool,
    net_panel_width: f32,
    net_rename_buffer: Option<(GridPos, String)>,

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
    /// Physical properties carried from the chosen preset onto the next placed board
    pub new_board_outline: BoardOutline,
    pub new_board_mounting: MountingSpec,
    pub new_board_edge_pads: EdgePads,
    pub new_board_mounting_holes: bool,
    /// GL context for offscreen PNG rendering; `None` in headless tests.
    #[cfg(not(target_arch = "wasm32"))]
    pub gl: Option<std::sync::Arc<glow::Context>>,

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

            netlist: Netlist::default(),
            netlist_generation: 0,
            netlist_build_ms: 0.0,
            highlighted_net: None,
            hovered_net: None,
            scroll_to_net: None,
            pending_highlight_hole: None,
            zoom_request: None,
            focus_point: None,
            stolen_tab: false,
            show_net_panel: true,
            focus_net_mode: false,
            net_panel_width: 0.0,
            net_rename_buffer: None,

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
            new_board_outline: BoardOutline::default(),
            new_board_mounting: MountingSpec::default(),
            new_board_edge_pads: EdgePads::default(),
            new_board_mounting_holes: true,
            #[cfg(not(target_arch = "wasm32"))]
            gl: None,

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
        let mut app = Self::default();
        // Kept so the PNG export can render into its own framebuffer. `Default` leaves it
        // `None`, which is what every headless test constructs — image export is the one
        // feature that genuinely needs a GPU, and it reports that rather than panicking.
        #[cfg(not(target_arch = "wasm32"))]
        {
            app.gl = cc.gl.clone();
        }
        app
    }

    /// Render both sides of every board to PNG, writing `<stem>-<board>-front.png` and
    /// `-back.png` next to the chosen path.
    #[cfg(not(target_arch = "wasm32"))]
    fn export_board_images(&mut self, dir: &std::path::Path, stem: &str, px_per_mm: f32) {
        use crate::export::image::{BoardSide, ImagePlan};

        let Some(gl) = self.gl.clone() else {
            self.status_toast = Some((
                "Image export needs a GPU context; none is available".to_string(),
                true,
            ));
            return;
        };

        let mut written = 0usize;
        for (i, board) in self.project.boards.iter().enumerate() {
            for side in BoardSide::ALL {
                let plan = ImagePlan::new(board, side, px_per_mm);
                let bytes = match crate::export::image::render_png(&gl, &plan, &self.project) {
                    Ok(b) => b,
                    Err(e) => {
                        self.status_toast = Some((format!("Image export failed: {e}"), true));
                        return;
                    }
                };
                let slug: String = board
                    .name
                    .chars()
                    .map(|c| if c.is_alphanumeric() { c } else { '_' })
                    .collect();
                let path = dir.join(format!("{stem}-{i}-{slug}-{}.png", side.suffix()));
                if let Err(e) = std::fs::write(&path, bytes) {
                    self.status_toast =
                        Some((format!("Could not write {}: {e}", path.display()), true));
                    return;
                }
                written += 1;
            }
        }

        self.status_toast = Some(if written == 0 {
            ("No boards to export".to_string(), true)
        } else {
            (
                format!(
                    "Exported {written} image(s) at {px_per_mm:.0} px/mm to {}",
                    dir.display()
                ),
                false,
            )
        });
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

    /// Rebuild the derived netlist. Called unconditionally once per frame: a revision
    /// counter is not trustworthy yet because the inspector mutates the document straight
    /// through `&mut` egui widgets, so there is no single mutation chokepoint. The cost is
    /// surfaced in the status bar so it stays honest.
    fn rebuild_netlist(&mut self) {
        let started = std::time::Instant::now();
        self.netlist_generation += 1;
        self.netlist = Netlist::build(&self.project, self.netlist_generation);
        self.netlist_build_ms = started.elapsed().as_secs_f32() * 1000.0;

        // A conductor created last frame is only now visible to the netlist, so this is
        // the first point at which its net can be resolved.
        if let Some(hole) = self.pending_highlight_hole.take() {
            if let Some(net) = self.netlist.net_at(hole) {
                let anchor = net.anchor();
                self.highlighted_net = Some(anchor);
                self.scroll_to_net = Some(anchor);
            }
        }

        // Drop a highlight whose anchor no longer belongs to any net (the wire holding it
        // together was deleted, the board was moved out from under it, ...).
        if let Some(anchor) = self.highlighted_net {
            if self.netlist.net_at(anchor).is_none() {
                self.highlighted_net = None;
            }
        }
    }

    /// The net to draw as highlighted this frame, and whether it was *pinned* by a click
    /// rather than merely hovered. A hover preview wins over a pin so that moving the
    /// pointer always shows what is under it, but it draws fainter.
    fn active_highlight(&self) -> Option<(GridPos, bool)> {
        match (self.hovered_net, self.highlighted_net) {
            // Hovering the net that is already pinned should not dim it.
            (Some(hover), pinned) => Some((hover, pinned == Some(hover))),
            (None, Some(pinned)) => Some((pinned, true)),
            (None, None) => None,
        }
    }

    /// Resolve a pointer position to the net under it.
    ///
    /// The snapped hole is tried first (pins, solder nodes and strip copper all live on
    /// holes), then the hit entity. The fallback is what makes clicking a wire mid-span
    /// work: wire waypoints are off-grid, so the nearest hole is often unrelated bare
    /// board, and only `hit_test` knows a wire was actually under the cursor.
    fn net_anchor_at(&self, world_pos: WorldPos, grid_pos: GridPos) -> Option<GridPos> {
        if let Some(net) = self.netlist.net_at(grid_pos) {
            return Some(net.anchor());
        }
        match self.hit_test(world_pos, grid_pos)? {
            SelectedItem::Wire(id) => self.netlist.net_of_wire(id).map(|n| n.anchor()),
            SelectedItem::SolderLine(id) => self.netlist.net_of_solder_line(id).map(|n| n.anchor()),
            // A multi-pin part spans several nets; picking one arbitrarily would be a
            // coin flip, so the body only resolves when the part has exactly one net.
            SelectedItem::Connector(id) => {
                let nets = self.netlist.nets_of_connector(id);
                match nets.as_slice() {
                    [only] => Some(only.anchor()),
                    _ => None,
                }
            }
            SelectedItem::Board(_) => None,
        }
    }

    /// Everything the last click landed on, outermost to innermost, so the sidebar can
    /// show a pane per level rather than only for the one entity that won the hit test.
    pub fn context_stack(&self) -> Vec<ContextLayer> {
        // Tied to the selection rather than cleared at each of the many places that drop
        // it: a stale focus point with nothing selected must not show panes.
        if self.selected.is_empty() {
            return Vec::new();
        }
        let Some((world, hole)) = self.focus_point else {
            return Vec::new();
        };
        let mut stack = Vec::new();

        // Component, then the specific pin of it that was clicked.
        for conn in self.project.connectors.iter().rev() {
            let pin = conn
                .pin_positions()
                .into_iter()
                .position(|(p, _)| p == hole);
            if pin.is_some() || conn.contains_world_pos(world) {
                stack.push(ContextLayer::Connector(conn.id));
                if let Some(index) = pin {
                    stack.push(ContextLayer::Pin {
                        connector: conn.id,
                        index,
                    });
                }
                break;
            }
        }

        // Conductors touching that hole, plus any the pointer is directly over.
        for wire in &self.project.wires {
            if wire.start_hole == hole
                || wire.end_hole == hole
                || wire.distance_to_point(world) < 0.36
            {
                stack.push(ContextLayer::Wire(wire.id));
            }
        }
        for solder in &self.project.solder_lines {
            if solder.contains_hole(hole) || solder.distance_to_point(world) < 0.38 {
                stack.push(ContextLayer::Solder(solder.id));
            }
        }

        if let Some(net) = self.netlist.net_at(hole) {
            stack.push(ContextLayer::Net(net.anchor()));
        }
        if let Some(board) = self.project.find_board_at_world(world) {
            stack.push(ContextLayer::Board(board.id));
        }
        stack
    }

    /// Whether Tab currently means "toggle wire routing mode" rather than "move focus".
    /// Kept narrow on purpose: stealing Tab globally would break keyboard navigation of
    /// the menus and the sidebar's text fields.
    fn tab_toggles_wire_mode(&self) -> bool {
        if self.show_add_dialog || self.show_json_modal {
            return false;
        }
        self.tool == ToolMode::DrawWire
            || matches!(self.selected.as_slice(), [SelectedItem::Wire(_)])
    }

    /// Flip routing mode on the selected wire, or the tool default when drawing.
    fn toggle_wire_mode(&mut self) {
        let next = match self.selected.as_slice() {
            [SelectedItem::Wire(id)] => {
                self.project
                    .wires
                    .iter()
                    .find(|w| w.id == *id)
                    .map(|w| match w.mode {
                        WireMode::Straight => WireMode::Spline,
                        WireMode::Spline => WireMode::Straight,
                    })
            }
            _ => Some(match self.wire_mode {
                WireMode::Straight => WireMode::Spline,
                WireMode::Spline => WireMode::Straight,
            }),
        };
        if let Some(next) = next {
            self.apply_context_intent(ContextIntent::SetWireMode(next));
        }
    }

    /// Read-only snapshot handed to context actions.
    fn context_scope(&self) -> ContextScope<'_> {
        ContextScope {
            selected: &self.selected,
            project: &self.project,
            netlist: &self.netlist,
            tool: self.tool.clone(),
            focus_net_mode: self.focus_net_mode,
            highlighted_net: self.highlighted_net,
            wire_mode: self.wire_mode,
            wire_color: self.wire_color,
            header_cols: self.header_cols,
            header_rows: self.header_rows,
            active_rotation: self.active_rotation,
        }
    }

    /// The only place a context action's result is applied. Each arm delegates; no arm
    /// contains logic of its own.
    fn apply_context_intent(&mut self, intent: ContextIntent) {
        match intent {
            ContextIntent::ToggleFocusNet => self.focus_net_mode = !self.focus_net_mode,
            ContextIntent::PinNet(anchor) => {
                self.highlighted_net = Some(anchor);
                self.scroll_to_net = Some(anchor);
            }
            ContextIntent::UnpinNet => self.highlighted_net = None,
            ContextIntent::NameNet(anchor) => {
                let existing = self
                    .netlist
                    .net_at(anchor)
                    .and_then(|n| n.labels.first().copied())
                    .and_then(|id| self.project.net_labels.iter().find(|l| l.id == id))
                    .map(|l| l.name.clone())
                    .unwrap_or_default();
                self.net_rename_buffer = Some((anchor, existing));
                self.show_net_panel = true;
            }
            ContextIntent::ZoomToSelection => self.zoom_to_selection(),
            ContextIntent::SetWireMode(mode) => match self.selected.as_slice() {
                [SelectedItem::Wire(id)] => {
                    let id = *id;
                    self.record_undo();
                    if let Some(w) = self.project.wires.iter_mut().find(|w| w.id == id) {
                        w.mode = mode;
                    }
                }
                _ => self.wire_mode = mode,
            },
            ContextIntent::SetWireColor(color) => match self.selected.as_slice() {
                [SelectedItem::Wire(id)] => {
                    let id = *id;
                    self.record_undo();
                    if let Some(w) = self.project.wires.iter_mut().find(|w| w.id == id) {
                        w.color = color;
                    }
                }
                _ => self.wire_color = color,
            },
            ContextIntent::CycleConductorWidth => self.cycle_conductor_width(),
            ContextIntent::SetHeaderSize { cols, rows } => {
                self.header_cols = cols;
                self.header_rows = rows;
            }
            ContextIntent::RotatePlacement => {
                self.active_rotation = self.active_rotation.next_cw();
                if self.tool == ToolMode::PlaceBoard {
                    std::mem::swap(&mut self.new_board_cols, &mut self.new_board_rows);
                }
            }
        }
    }

    /// Step the selected conductor to the next preset width. With nothing selected this
    /// moves the tool default instead, so the next wire drawn picks it up.
    fn cycle_conductor_width(&mut self) {
        match self.selected.as_slice() {
            [SelectedItem::Wire(id)] => {
                let id = *id;
                self.record_undo();
                if let Some(w) = self.project.wires.iter_mut().find(|w| w.id == id) {
                    w.thickness = cycle_width(w.thickness, &WIRE_WIDTH_STEPS);
                    let t = w.thickness;
                    self.status_toast = Some((format!("Wire width {:.1}", t), false));
                }
            }
            [SelectedItem::SolderLine(id)] => {
                let id = *id;
                self.record_undo();
                if let Some(s) = self.project.solder_lines.iter_mut().find(|s| s.id == id) {
                    s.width_scale = cycle_width(s.width_scale, &SOLDER_WIDTH_STEPS);
                    let t = s.width_scale;
                    self.status_toast = Some((format!("Solder width {:.2}", t), false));
                }
            }
            _ => {}
        }
    }

    /// Frame the viewport on the current selection, reusing the same span-to-zoom maths
    /// as `Viewport::fit_project`.
    fn zoom_to_selection(&mut self) {
        let mut min = WorldPos::new(f32::INFINITY, f32::INFINITY);
        let mut max = WorldPos::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut grow = |a: WorldPos, b: WorldPos| {
            min.x = min.x.min(a.x);
            min.y = min.y.min(a.y);
            max.x = max.x.max(b.x);
            max.y = max.y.max(b.y);
        };

        for item in &self.selected {
            match item {
                SelectedItem::Board(id) => {
                    if let Some(b) = self.project.boards.iter().find(|b| b.id == *id) {
                        let (lo, hi) = b.world_bounds();
                        grow(lo, hi);
                    }
                }
                SelectedItem::Connector(id) => {
                    if let Some(c) = self.project.connectors.iter().find(|c| c.id == *id) {
                        let (lo, hi) = c.world_aabb();
                        grow(lo, hi);
                    }
                }
                SelectedItem::Wire(id) => {
                    if let Some(w) = self.project.wires.iter().find(|w| w.id == *id) {
                        for p in w.sampled_points() {
                            grow(p, p);
                        }
                    }
                }
                SelectedItem::SolderLine(id) => {
                    if let Some(s) = self.project.solder_lines.iter().find(|s| s.id == *id) {
                        for n in &s.nodes {
                            let p = WorldPos::from_grid(*n);
                            grow(p, p);
                        }
                    }
                }
            }
        }

        if !min.x.is_finite() || !max.x.is_finite() {
            return;
        }
        self.zoom_request = Some((min, max));
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
            self.new_board_outline = preset.outline;
            self.new_board_mounting = preset.mounting;
            self.new_board_edge_pads = preset.edge_pads;
            self.new_board_mounting_holes = preset.mounting_holes;
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
        board.outline = self.new_board_outline;
        board.mounting = self.new_board_mounting;
        board.edge_pads = self.new_board_edge_pads;
        board.mounting_holes = self.new_board_mounting_holes;
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
            body_height_mm: None,
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
                } else if self.highlighted_net.is_some() {
                    self.highlighted_net = None;
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
            if i.key_pressed(egui::Key::N) {
                self.show_net_panel = !self.show_net_panel;
            }
            if i.key_pressed(egui::Key::T) {
                self.cycle_conductor_width();
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
            if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) {
                self.delete_selected();
            }
        });

        // Tab arrives via `raw_input_hook`, not the event stream, so it is handled
        // outside the input closure.
        if std::mem::take(&mut self.stolen_tab) {
            self.toggle_wire_mode();
        }
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
    /// Take Tab away from egui's focus traversal when it means "switch wire routing mode".
    ///
    /// egui latches the focus direction from a Tab press inside `Memory::begin_pass`,
    /// which runs *before* `update`. Handling Tab in the keyboard handler therefore can
    /// never win the race — the focus has already moved to the next widget (File, Edit,
    /// …). The only place to intervene is before egui reads the input at all.
    ///
    /// Tab is only stolen while it is actually meaningful, so ordinary focus traversal
    /// keeps working everywhere else — including tabbing between sidebar text fields.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        // A focused text field keeps Tab for itself — editing a wire label and tabbing to
        // the next field must not silently change the wire's routing mode.
        if ctx.wants_keyboard_input() || !self.tab_toggles_wire_mode() {
            return;
        }
        let mut stole = false;
        raw_input.events.retain(|event| {
            let is_plain_tab = matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::Tab,
                    modifiers: egui::Modifiers { shift: false, .. },
                    ..
                }
            );
            if is_plain_tab {
                // Both press and release are dropped; leaving the release behind
                // confuses egui's key-state bookkeeping.
                if matches!(event, egui::Event::Key { pressed: true, .. }) {
                    stole = true;
                }
                return false;
            }
            true
        });
        self.stolen_tab |= stole;
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_keyboard_shortcuts(ctx);
        self.rebuild_netlist();

        // Offscreen PNG rendering is the one path the test suite cannot reach: it needs a
        // real GL context, which only exists inside a running window. This hook exports
        // every board and quits, so the feature can still be exercised end to end.
        // See tools/validate_png.sh.
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(path) = std::env::var("PERFWORK_LOAD") {
            if let Ok(data) = std::fs::read_to_string(&path) {
                if let Ok(p) = serde_json::from_str::<Project>(&data) {
                    self.project = p;
                    self.selected.clear();
                }
            }
            std::env::remove_var("PERFWORK_LOAD");
        }

        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(dir) = std::env::var("PERFWORK_DUMP_PNG") {
            self.export_board_images(
                std::path::Path::new(&dir),
                "dump",
                crate::export::image::DEFAULT_PX_PER_MM,
            );
            if let Some((msg, err)) = &self.status_toast {
                println!(
                    "PERFWORK_DUMP_PNG: {}{}",
                    if *err { "ERROR: " } else { "" },
                    msg
                );
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

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

        // 3. Sidebar — properties + nets. Must be shown *before* the CentralPanel or the
        //    canvas claims the full width and the panel never gets any.
        self.net_panel_width = 0.0;
        if self.show_net_panel {
            let response = egui::SidePanel::right("net_panel")
                .resizable(true)
                .default_width(290.0)
                .width_range(230.0..=460.0)
                .frame(
                    egui::Frame::NONE
                        .fill(Color32::from_rgb(24, 27, 33))
                        .inner_margin(Margin::symmetric(10, 8))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(40, 44, 54))),
                )
                .show(ctx, |ui| {
                    self.render_sidebar(ui);
                });
            self.net_panel_width = response.response.rect.width();
        }

        // 4. Full-Window Interactive 2D CAD Canvas
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(20, 22, 27)))
            .show(ctx, |ui| {
                self.render_center_canvas(ui);
            });

        // 5. Autodesk Fusion-Style Floating Icon Toolbar at Top-Center of the Canvas
        self.render_floating_fusion_toolbar(ctx);

        // 6. Modals (Unified Add / Library Dialog & JSON Import/Export)
        self.render_modals(ctx);

        // The hover highlight lasts exactly one frame, wherever it came from.
        self.hovered_net = None;
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
                    if ui.button("Export STEP (3D)...").clicked() {
                        ui.close_menu();
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("STEP AP214", &["step", "stp"])
                            .set_file_name("circuit.step")
                            .save_file()
                        {
                            let name = path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "circuit.step".to_string());
                            let text = crate::export::step::export_step(&self.project, &name);
                            let solids = crate::export::step::collect_prisms(&self.project).len();
                            self.status_toast = Some(match std::fs::write(&path, text) {
                                Ok(()) => (
                                    format!("Exported {} solid(s) to {}", solids, path.display()),
                                    false,
                                ),
                                Err(e) => (format!("STEP export failed: {e}"), true),
                            });
                        }
                    }
                    if ui.button("Export Board Images (PNG)...").clicked() {
                        ui.close_menu();
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name("board.png")
                            .save_file()
                        {
                            let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                            let stem = path
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| "board".to_string());
                            self.export_board_images(
                                &dir,
                                &stem,
                                crate::export::image::DEFAULT_PX_PER_MM,
                            );
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
                ui.checkbox(&mut self.show_net_panel, "Show Sidebar (N)");
                ui.checkbox(&mut self.show_net_highlights, "Highlight Nets");
                ui.checkbox(&mut self.focus_net_mode, "Focus Net (dim others)");
                ui.separator();
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
        // `Area::anchor` resolves against the screen, not the central panel, so the dock
        // has to step aside for the net sidebar by half its width to stay centred.
        egui::Area::new(egui::Id::new("fusion_floating_toolbar"))
            .anchor(Align2::CENTER_TOP, egui::vec2(-self.net_panel_width * 0.5, 42.0))
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
                            for (mode, icon, tooltip) in TOOLBAR_TOOLS {
                                let active = self.tool == mode;
                                let caption = toolbar_caption(&mode);
                                if toolbar_icon_button(ui, icon, &caption, active, false).on_hover_text(tooltip).clicked() {
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

                        });

                        // 3. Attached context row. Tool options and selection/net actions
                        //    now live here rather than being appended to the tool row, so
                        //    the main row never changes width as you switch tools.
                        self.render_context_row(ui);
                    });
            });
    }

    /// The second toolbar row: whatever the registry says applies right now.
    ///
    /// This method only lays out and dispatches. What to offer is each
    /// `ContextAction`'s business, and applying it is `apply_context_intent`'s.
    fn render_context_row(&mut self, ui: &mut egui::Ui) {
        let intent = {
            let scope = self.context_scope();
            let live: Vec<_> = context_actions()
                .iter()
                .filter(|a| a.applies(&scope))
                .collect();
            if live.is_empty() {
                return;
            }

            ui.add_space(5.0);
            ui.separator();
            ui.add_space(2.0);

            let mut intent = None;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                let mut first_group = true;
                for group in ActionGroup::ORDER {
                    let in_group: Vec<_> = live.iter().filter(|a| a.group() == group).collect();
                    if in_group.is_empty() {
                        continue;
                    }
                    if !first_group {
                        ui.separator();
                    }
                    first_group = false;
                    for action in in_group {
                        if let Some(produced) = action.ui(ui, &scope) {
                            intent = Some(produced);
                        }
                    }
                }
            });
            intent
        };

        if let Some(intent) = intent {
            self.apply_context_intent(intent);
        }
    }

    /// Compact floating inspector card in the top-right corner when item(s) are selected
    /// Property editors for the current selection. Lives in the sidebar rather than a
    /// floating card so that editing and net inspection read as one surface.
    fn render_selection_properties(&mut self, ui: &mut egui::Ui) {
        match self.selected.as_slice() {
            [] => {
                ui.label(
                    RichText::new("Nothing selected")
                        .size(11.5)
                        .strong()
                        .color(Color32::from_rgb(120, 132, 152)),
                );
                ui.label(
                    RichText::new(
                        "Click a board, component, wire or solder trace to edit it. \
                         Hover anything to preview its net.",
                    )
                    .size(11.0)
                    .color(Color32::from_rgb(120, 132, 152)),
                );
            }
            [SelectedItem::Board(board_id)] => {
                let id = *board_id;
                self.render_board_inspector(ui, id);
            }
            [SelectedItem::Connector(conn_id)] => {
                let id = *conn_id;
                self.render_connector_inspector(ui, id);
            }
            [SelectedItem::Wire(wire_id)] => {
                let id = *wire_id;
                self.render_wire_inspector(ui, id);
            }
            [SelectedItem::SolderLine(solder_id)] => {
                let id = *solder_id;
                self.render_solder_inspector(ui, id);
            }
            many => {
                ui.label(
                    RichText::new(format!("{} ITEMS SELECTED", many.len()))
                        .size(11.5)
                        .strong()
                        .color(Color32::from_rgb(110, 208, 255)),
                );
                ui.add_space(6.0);
                let count = |f: fn(&SelectedItem) -> bool| many.iter().filter(|i| f(i)).count();
                ui.label(format!(
                    "• Boards: {}\n• Components: {}\n• Wires: {}\n• Solder Traces: {}",
                    count(|i| matches!(i, SelectedItem::Board(_))),
                    count(|i| matches!(i, SelectedItem::Connector(_))),
                    count(|i| matches!(i, SelectedItem::Wire(_))),
                    count(|i| matches!(i, SelectedItem::SolderLine(_))),
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
    }

    /// One collapsible pane per level of the click, innermost two open.
    ///
    /// Pure layout and dispatch: every pane body is an existing inspector, so adding a
    /// layer means adding a variant and one match arm.
    fn render_context_stack(&mut self, ui: &mut egui::Ui, stack: &[ContextLayer]) {
        // Multi-select has no single click to layer; keep the summary instead.
        if self.selected.len() > 1 {
            self.render_selection_properties(ui);
            return;
        }

        for (depth, layer) in stack.iter().enumerate() {
            let salt = ("ctx_layer", depth, layer.heading());
            // The stack runs most-specific-first, so everything the click actually landed
            // on opens. Only the board folds away: it is the broadest context and by far
            // the tallest pane, and it would otherwise bury the levels above it.
            let open = !matches!(layer, ContextLayer::Board(_)) || stack.len() == 1;
            egui::CollapsingHeader::new(
                RichText::new(layer.heading())
                    .size(11.5)
                    .strong()
                    .color(Color32::from_rgb(110, 208, 255)),
            )
            .id_salt(salt)
            .default_open(open)
            .show(ui, |ui| match layer {
                ContextLayer::Connector(id) => self.render_connector_inspector(ui, *id),
                ContextLayer::Pin { connector, index } => {
                    self.render_pin_pane(ui, *connector, *index)
                }
                ContextLayer::Wire(id) => self.render_wire_inspector(ui, *id),
                ContextLayer::Solder(id) => self.render_solder_inspector(ui, *id),
                ContextLayer::Net(anchor) => {
                    if let Some(net) = self.netlist.net_at(*anchor) {
                        let (id, conflict) = (net.id, net.name.is_conflict());
                        let tint = if conflict {
                            Color32::from_rgb(255, 130, 110)
                        } else {
                            Color32::from_rgb(225, 232, 245)
                        };
                        self.render_net_row(ui, id, tint);
                    }
                }
                ContextLayer::Board(id) => self.render_board_inspector(ui, *id),
            });
        }
    }

    /// Editor for one pin of a placed component.
    ///
    /// `pin_labels` is a per-instance *override* vector that may be shorter than the pin
    /// count — `pin_positions` falls back to the footprint's own name when an entry is
    /// missing. Editing pin `index` therefore has to grow the vector with the real
    /// defaults first, or the other pins would silently inherit blanks.
    fn render_pin_pane(&mut self, ui: &mut egui::Ui, conn_id: Uuid, index: usize) {
        let Some(conn_idx) = self.project.connectors.iter().position(|c| c.id == conn_id) else {
            return;
        };
        let pins = self.project.connectors[conn_idx].pin_positions();
        let Some((hole, current_name)) = pins.get(index).cloned() else {
            return;
        };

        let location = self
            .project
            .find_board_at_hole(hole)
            .and_then(|b| b.format_global_hole(hole))
            .unwrap_or_else(|| format!("({}, {})", hole.col, hole.row));

        ui.label(
            RichText::new(format!("Pin {} of {}", index + 1, pins.len()))
                .size(11.0)
                .color(Color32::from_rgb(148, 158, 175)),
        );
        ui.label(
            RichText::new(format!("Hole {}", location))
                .size(11.0)
                .color(Color32::from_rgb(148, 158, 175)),
        );

        ui.add_space(4.0);
        let mut name = current_name.clone();
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(&mut name);
        });
        if name != current_name {
            self.record_undo();
            let defaults: Vec<String> = pins.iter().map(|(_, n)| n.clone()).collect();
            let labels = &mut self.project.connectors[conn_idx].pin_labels;
            if labels.len() <= index {
                labels.resize(index + 1, String::new());
                // Backfill with the footprint's own names rather than blanks.
                for (i, slot) in labels.iter_mut().enumerate() {
                    if slot.is_empty() {
                        if let Some(default) = defaults.get(i) {
                            *slot = default.clone();
                        }
                    }
                }
            }
            labels[index] = name;
        }

        ui.add_space(4.0);
        if let Some(net) = self
            .netlist
            .net_of_pin(conn_id, index)
            .and_then(|id| self.netlist.net(id))
        {
            let anchor = net.anchor();
            let named = net.name.is_named();
            ui.label(
                RichText::new(format!("On net {}", net.name.display()))
                    .size(11.0)
                    .color(Color32::from_rgb(148, 158, 175)),
            );
            if !named
                && !current_name.trim().is_empty()
                && ui
                    .small_button(format!("Name net '{}'", current_name))
                    .on_hover_text("Use this pin's name as the net name")
                    .clicked()
            {
                self.apply_net_name(anchor, current_name.clone());
            }
        } else {
            ui.label(
                RichText::new("Not connected")
                    .size(11.0)
                    .color(Color32::from_rgb(148, 158, 175)),
            );
        }
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

        ui.add_space(6.0);
        ui.label(RichText::new("Physical").size(11.0).color(Color32::GRAY));
        let (w_mm, h_mm) = board.size_mm();
        ui.label(
            RichText::new(format!(
                "{:.1} × {:.1} × {:.1} mm",
                w_mm, h_mm, board.outline.thickness_mm
            ))
            .size(11.5)
            .monospace(),
        );
        ui.add(
            egui::DragValue::new(&mut board.outline.thickness_mm)
                .speed(0.1)
                .range(0.4..=12.0)
                .suffix(" mm thick"),
        );
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut board.outline.margin_x_mm)
                    .speed(0.1)
                    .range(0.0..=25.0)
                    .prefix("border X "),
            );
            ui.add(
                egui::DragValue::new(&mut board.outline.margin_y_mm)
                    .speed(0.1)
                    .range(0.0..=25.0)
                    .prefix("Y "),
            );
        });

        ui.checkbox(&mut board.mounting_holes, "Corner Mounting Holes");
        if board.mounting_holes {
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut board.mounting.drill_mm)
                        .speed(0.1)
                        .range(0.5..=10.0)
                        .prefix("⌀ ")
                        .suffix(" mm"),
                );
                ui.add(
                    egui::DragValue::new(&mut board.mounting.inset_mm)
                        .speed(0.1)
                        .range(0.5..=20.0)
                        .prefix("inset ")
                        .suffix(" mm"),
                );
            });
        }

        ui.add_space(4.0);
        ui.label(
            RichText::new("Edge Solder Pads")
                .size(11.0)
                .color(Color32::GRAY),
        );
        ui.horizontal(|ui| {
            ui.checkbox(&mut board.edge_pads.left, "Left");
            ui.checkbox(&mut board.edge_pads.right, "Right");
        });

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

        // Body height drives the 3D export only, so it lives next to the physical facts
        // rather than the electrical ones. Parts sold in several heights (tactile switches
        // especially) are why this is overridable per placement at all.
        ui.add_space(4.0);
        let (standoff, height) = conn.body_profile_mm();
        let mut overridden = conn.body_height_mm.is_some();
        if ui
            .checkbox(&mut overridden, "Custom body height (3D)")
            .changed()
        {
            conn.body_height_mm = overridden.then_some(height);
        }
        if let Some(h) = conn.body_height_mm.as_mut() {
            ui.add(
                egui::DragValue::new(h)
                    .speed(0.1)
                    .range(0.1..=120.0)
                    .suffix(" mm tall"),
            );
        } else {
            ui.label(
                RichText::new(format!("{:.1} mm tall (library)", height))
                    .size(11.0)
                    .color(Color32::GRAY),
            );
        }
        if standoff > 0.0 {
            ui.label(
                RichText::new(format!("{:.1} mm standoff", standoff))
                    .size(11.0)
                    .color(Color32::GRAY),
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

    /// Net tree: conflicts first, then named nets, then unnamed. Hovering a row
    /// highlights its net on the canvas; clicking pins the highlight.
    /// The one editing surface: selection properties, then the net(s) that selection
    /// belongs to, then the full net tree for browsing.
    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("PROPERTIES")
                    .size(11.5)
                    .strong()
                    .color(Color32::from_rgb(110, 208, 255)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("✕")
                    .on_hover_text("Hide sidebar (N)")
                    .clicked()
                {
                    self.show_net_panel = false;
                }
            });
        });
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            let stack = self.context_stack();
            if stack.is_empty() {
                self.render_selection_properties(ui);
            } else {
                self.render_context_stack(ui, &stack);
            }

            ui.add_space(12.0);
            egui::CollapsingHeader::new(
                RichText::new("All nets")
                    .size(11.5)
                    .strong()
                    .color(Color32::from_rgb(110, 208, 255)),
            )
            .id_salt("all_nets_tree")
            .default_open(true)
            .show(ui, |ui| self.render_net_tree(ui));
        });
    }

    fn render_net_tree(&mut self, ui: &mut egui::Ui) {
        // Focus and Pin moved to the toolbar's context row; see `crate::actions`.
        let conflicts = self.netlist.conflicts().count();
        ui.label(
            RichText::new(format!(
                "{} nets • {} conflicts",
                self.netlist.nets.len(),
                conflicts
            ))
            .size(11.0)
            .color(if conflicts > 0 {
                Color32::from_rgb(255, 130, 110)
            } else {
                Color32::from_rgb(148, 158, 175)
            }),
        );
        ui.separator();

        if self.netlist.nets.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "No nets yet. Wires, solder traces and stripboard copper create them.",
                )
                .size(11.5)
                .color(Color32::from_rgb(132, 142, 158)),
            );
            return;
        }

        // Partition by name kind. Indices stay valid for this frame only.
        let mut conflict_ids = Vec::new();
        let mut named_ids = Vec::new();
        let mut unnamed_ids = Vec::new();
        for net in &self.netlist.nets {
            match net.name {
                NetName::Conflict(_) => conflict_ids.push(net.id),
                NetName::Named(_) => named_ids.push(net.id),
                NetName::Unnamed(_) => unnamed_ids.push(net.id),
            }
        }
        named_ids.sort_by_key(|&id| self.netlist.nets[id].name.display());

        // No ScrollArea here: the sidebar owns scrolling, and nesting one inside it
        // would produce two competing scrollbars.
        for (title, ids, tint) in [
            ("Conflicts", conflict_ids, Color32::from_rgb(255, 130, 110)),
            ("Named", named_ids, Color32::from_rgb(225, 232, 245)),
            ("Unnamed", unnamed_ids, Color32::from_rgb(150, 160, 178)),
        ] {
            if ids.is_empty() {
                continue;
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("{} ({})", title, ids.len()))
                    .size(10.5)
                    .strong()
                    .color(Color32::from_rgb(120, 132, 152)),
            );
            for id in ids {
                self.render_net_row(ui, id, tint);
            }
        }
    }

    fn render_net_row(&mut self, ui: &mut egui::Ui, id: usize, tint: Color32) {
        let Some(net) = self.netlist.net(id) else {
            return;
        };
        let anchor = net.anchor();
        let title = net.name.display();
        let pin_count = net.pins.len();
        let hole_count = net.holes.len();
        let is_conflict = net.name.is_conflict();
        let is_named = net.name.is_named();
        let pins: Vec<String> = net.pins.iter().map(|p| p.display()).collect();
        let label_ids = net.labels.clone();
        let selected = self.highlighted_net == Some(anchor);

        let header = egui::CollapsingHeader::new(
            RichText::new(format!("{}  ·  {} pins", title, pin_count))
                .size(12.0)
                .color(if selected { Color32::WHITE } else { tint })
                .strong(),
        )
        .id_salt(("net_row", anchor.col, anchor.row))
        .show(ui, |ui| {
            if is_conflict {
                ui.label(
                    RichText::new(
                        "Two differently named labels landed in one net — these holes are \
                         electrically the same point.",
                    )
                    .size(10.5)
                    .color(Color32::from_rgb(255, 150, 130)),
                );
                ui.add_space(4.0);
            }
            ui.label(
                RichText::new(format!("{} holes", hole_count))
                    .size(10.5)
                    .color(Color32::from_rgb(130, 140, 156)),
            );
            for pin in &pins {
                ui.label(RichText::new(format!("  • {}", pin)).size(11.0).monospace());
            }
            if pins.is_empty() {
                ui.label(
                    RichText::new("  (no component pins)")
                        .size(10.5)
                        .italics()
                        .color(Color32::from_rgb(130, 140, 156)),
                );
            }

            ui.add_space(6.0);
            self.render_net_naming_controls(ui, anchor, is_named, &label_ids);
        });

        let row = header.header_response;
        if row.hovered() {
            self.hovered_net = Some(anchor);
        }
        if row.clicked() {
            self.highlighted_net = if selected { None } else { Some(anchor) };
        }
        // Reveal the row for a net that was just pinned from the canvas, so the tree
        // follows the drawing instead of having to be searched.
        if self.scroll_to_net == Some(anchor) {
            row.scroll_to_me(Some(egui::Align::Center));
            self.scroll_to_net = None;
        }
    }

    /// Naming a net means creating a `NetLabel` anchored to one of its holes; renaming
    /// edits that label, and clearing the name deletes it. All three are document edits
    /// and go through the undo stack.
    fn render_net_naming_controls(
        &mut self,
        ui: &mut egui::Ui,
        anchor: GridPos,
        is_named: bool,
        label_ids: &[Uuid],
    ) {
        let editing = matches!(&self.net_rename_buffer, Some((h, _)) if *h == anchor);

        if editing {
            let mut commit = false;
            let mut cancel = false;
            if let Some((_, buffer)) = &mut self.net_rename_buffer {
                let response = ui.text_edit_singleline(buffer);
                response.request_focus();
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    commit = true;
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
            }
            ui.horizontal(|ui| {
                if ui.small_button("Apply").clicked() {
                    commit = true;
                }
                if ui.small_button("Cancel").clicked() {
                    cancel = true;
                }
            });
            if commit {
                if let Some((hole, name)) = self.net_rename_buffer.take() {
                    self.apply_net_name(hole, name);
                }
            } else if cancel {
                self.net_rename_buffer = None;
            }
            return;
        }

        ui.horizontal(|ui| {
            let verb = if is_named {
                "Rename…"
            } else {
                "Name this net…"
            };
            if ui.small_button(verb).clicked() {
                let existing = label_ids
                    .first()
                    .and_then(|id| self.project.net_labels.iter().find(|l| l.id == *id))
                    .map(|l| l.name.clone())
                    .unwrap_or_default();
                self.net_rename_buffer = Some((anchor, existing));
            }
            if !label_ids.is_empty() && ui.small_button("Clear name").clicked() {
                self.record_undo();
                let drop: Vec<Uuid> = label_ids.to_vec();
                self.project.net_labels.retain(|l| !drop.contains(&l.id));
                self.status_toast = Some(("Removed net name".into(), false));
            }
        });
    }

    /// Point every label in this net at one name. An unnamed net gets a fresh label on
    /// its anchor hole; a conflicted net collapses to the single chosen name.
    fn apply_net_name(&mut self, anchor: GridPos, name: String) {
        let name = name.trim().to_string();
        let Some(net) = self.netlist.net_at(anchor) else {
            return;
        };
        let existing = net.labels.clone();
        self.record_undo();

        if name.is_empty() {
            self.project
                .net_labels
                .retain(|l| !existing.contains(&l.id));
            self.status_toast = Some(("Removed net name".into(), false));
            return;
        }

        match existing.split_first() {
            Some((keep, drop)) => {
                let keep = *keep;
                let drop: Vec<Uuid> = drop.to_vec();
                self.project.net_labels.retain(|l| !drop.contains(&l.id));
                if let Some(label) = self.project.net_labels.iter_mut().find(|l| l.id == keep) {
                    label.name = name.clone();
                }
            }
            None => self.project.net_labels.push(NetLabel::new(anchor, &name)),
        }
        self.highlighted_net = Some(anchor);
        self.status_toast = Some((format!("Named net '{}'", name), false));
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
                let conflicts = self.netlist.conflicts().count();
                ui.label(
                    RichText::new(format!(
                        "{} nets{} • {:.2}ms",
                        self.netlist.nets.len(),
                        if conflicts > 0 {
                            format!(" • {} ⚡", conflicts)
                        } else {
                            String::new()
                        },
                        self.netlist_build_ms
                    ))
                    .size(11.0)
                    .color(if conflicts > 0 {
                        Color32::from_rgb(255, 130, 110)
                    } else {
                        Color32::from_rgb(148, 158, 175)
                    }),
                )
                .on_hover_text("Derived netlist: net count, name conflicts, and rebuild cost");

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

        // Consume a deferred "zoom to selection" now that the canvas rect is known.
        if let Some((min, max)) = self.zoom_request.take() {
            self.viewport.fit_bounds(canvas_rect, min, max);
        }

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
        // Not `snap_to_grid`: edge pads sit half a pitch off the lattice, so resolving the
        // pointer has to ask the project, or they cannot be clicked at all.
        let hovered_hole = pointer_world.map(|w| self.project.hole_at_world(w));
        if response.hovered() && !self.show_add_dialog && !self.show_json_modal {
            if let Some(hole) = hovered_hole {
                self.last_pointer_grid = hole;
            }
            // Always-on faint preview of the net under the pointer. Only in Select mode:
            // while routing a wire or dragging solder the pointer means "place here",
            // and a net following the cursor would just be noise.
            if self.tool == ToolMode::Select && self.pending_transform.is_none() {
                if let (Some(world), Some(hole)) = (pointer_world, hovered_hole) {
                    self.hovered_net = self.net_anchor_at(world, hole);
                }
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

        let active = self.active_highlight();
        let highlighted_net = active.and_then(|(anchor, _)| self.netlist.net_at(anchor));
        let highlight_pinned = active.is_some_and(|(_, pinned)| pinned);
        let conflict_nets: Vec<_> = self.netlist.conflicts().collect();

        let painter = ui.painter_at(canvas_rect);
        render_workspace(
            &painter,
            canvas_rect,
            &self.viewport,
            &self.project,
            &self.selected,
            &CanvasRenderPreview {
                highlighted_net,
                highlight_pinned,
                focus_net: self.focus_net_mode,
                conflict_nets: &conflict_nets,
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
                    // Clicking a pin, wire or trace pins its net; clicking bare board
                    // clears it. The tree then scrolls to match.
                    self.highlighted_net = self.net_anchor_at(world_pos, grid_pos);
                    self.scroll_to_net = self.highlighted_net;
                    // Remember *where* so the sidebar can show every level of what was
                    // clicked, not just the entity that won the hit test.
                    self.focus_point = Some((world_pos, grid_pos));

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
                            self.pending_highlight_hole = Some(wire.start_hole);
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
            // Capture before `nodes` is moved into the SolderLine.
            self.pending_highlight_hole = nodes.first().copied();
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
        if let Some(net) = self.netlist.net_at(hole) {
            status_parts.push(format!(
                "Net: {} ({} pins)",
                net.name.display(),
                net.pins.len()
            ));
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
    // For `raw_input_hook`, which is a trait method.
    use eframe::App as _;

    fn pin_fixture(project: &mut Project, anchor: GridPos) -> PlacedConnector {
        PlacedConnector {
            id: Uuid::new_v4(),
            designator: project.next_designator("P"),
            label: String::new(),
            anchor,
            rotation: Rotation::Deg0,
            kind: ConnectorKind::SinglePin { square_pad: false },
            body_color: RgbColor::new(225, 185, 65),
            pin_labels: Vec::new(),
            body_height_mm: None,
        }
    }

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
    fn naming_a_net_is_one_undoable_edit_and_renaming_reuses_the_label() {
        let mut app = fixture();
        let a = app.project.connectors[0].anchor;
        let b = GridPos::new(6, 2);
        let extra = pin_fixture(&mut app.project, b);
        app.project.connectors.push(extra);
        app.project.wires.push(Wire {
            id: Uuid::new_v4(),
            label: String::new(),
            start_hole: a,
            end_hole: b,
            waypoints: Vec::new(),
            mode: WireMode::Straight,
            color: RgbColor::new(255, 0, 0),
            thickness: 1.0,
            layer: WireLayer::TopComponent,
        });
        app.rebuild_netlist();

        let anchor = app.netlist.net_at(a).unwrap().anchor();
        assert!(app.project.net_labels.is_empty());

        app.apply_net_name(anchor, "VCC".into());
        assert_eq!(app.project.net_labels.len(), 1);
        app.rebuild_netlist();
        assert_eq!(app.netlist.net_at(a).unwrap().name.display(), "VCC");

        // Renaming edits the existing label rather than stacking a second one.
        app.apply_net_name(anchor, "VBUS".into());
        assert_eq!(app.project.net_labels.len(), 1);
        app.rebuild_netlist();
        assert_eq!(app.netlist.net_at(a).unwrap().name.display(), "VBUS");

        // Each rename was a single undo step.
        app.undo();
        app.rebuild_netlist();
        assert_eq!(app.netlist.net_at(a).unwrap().name.display(), "VCC");
        app.undo();
        app.rebuild_netlist();
        assert!(app.project.net_labels.is_empty());

        // An empty name clears it. (`apply_net_name` reads the current netlist to find
        // the net's labels, so the rebuild the app does every frame is required here.)
        app.apply_net_name(anchor, "GND".into());
        app.rebuild_netlist();
        app.apply_net_name(anchor, "   ".into());
        assert!(app.project.net_labels.is_empty());
    }

    #[test]
    fn highlight_anchor_survives_edits_that_renumber_nets() {
        let mut app = fixture();
        let a = app.project.connectors[0].anchor;
        let far = GridPos::new(15, 15);
        let extra = pin_fixture(&mut app.project, far);
        app.project.connectors.push(extra);
        app.project
            .solder_lines
            .push(SolderLine::new(vec![a, GridPos::new(a.col + 1, a.row)]));
        app.rebuild_netlist();

        let anchor = app.netlist.net_at(a).unwrap().anchor();
        app.highlighted_net = Some(anchor);
        let before = app.netlist.net_at(anchor).unwrap().id;

        // Insert a net that sorts *before* the highlighted one, shifting every NetId.
        app.project.solder_lines.push(SolderLine::new(vec![
            GridPos::new(0, 0),
            GridPos::new(1, 0),
        ]));
        app.rebuild_netlist();

        assert_eq!(app.highlighted_net, Some(anchor), "anchor must be stable");
        let after = app.netlist.net_at(anchor).unwrap().id;
        assert_ne!(before, after, "the NetId should indeed have shifted");

        // Removing the connectivity drops the highlight instead of dangling.
        app.project.solder_lines.clear();
        app.project.connectors.clear();
        app.rebuild_netlist();
        assert_eq!(app.highlighted_net, None);
    }

    /// Drives the real net panel with synthetic pointer events: find the VCC row, hover
    /// it, click it, and confirm the highlight anchor lands on that net.
    #[test]
    fn net_panel_rows_hover_and_click_to_highlight() {
        let mut app = PerfworkApp {
            project: Project::new_starter(),
            show_net_panel: true,
            ..Default::default()
        };
        app.rebuild_netlist();
        let vcc_anchor = app
            .netlist
            .nets
            .iter()
            .find(|n| n.name.display() == "VCC")
            .expect("starter project has a VCC net")
            .anchor();

        let ctx = egui::Context::default();
        let mut time = 0.0;
        let mut draw = |app: &mut PerfworkApp, events| {
            time += 0.05;
            ctx.run(
                egui::RawInput {
                    time: Some(time),
                    events,
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 1000.0))),
                    ..Default::default()
                },
                |ctx| {
                    app.rebuild_netlist();
                    egui::SidePanel::right("net_panel").show(ctx, |ui| app.render_sidebar(ui));
                },
            )
        };

        let _ = draw(&mut app, vec![]);
        let output = draw(&mut app, vec![]);

        let vcc_pos = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text.starts_with("VCC") => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("VCC row should be listed in the net panel");

        // Hover alone highlights without pinning.
        let _ = draw(&mut app, vec![egui::Event::PointerMoved(vcc_pos)]);
        assert_eq!(app.hovered_net, Some(vcc_anchor));
        assert_eq!(app.highlighted_net, None);

        // Clicking pins it.
        let _ = draw(
            &mut app,
            vec![
                egui::Event::PointerMoved(vcc_pos),
                egui::Event::PointerButton {
                    pos: vcc_pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let _ = draw(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: vcc_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(app.highlighted_net, Some(vcc_anchor));

        // Clicking the same row again releases it.
        for pressed in [true, false] {
            let _ = draw(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: vcc_pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert_eq!(app.highlighted_net, None);
    }

    #[test]
    fn clicking_a_pin_highlights_its_net_and_bare_board_clears_it() {
        let mut app = PerfworkApp {
            project: Project::new_starter(),
            ..Default::default()
        };
        app.rebuild_netlist();

        // J1 pin 1 sits on the VCC bus.
        let j1 = app
            .project
            .connectors
            .iter()
            .find(|c| c.designator == "J1")
            .unwrap();
        let pin = j1.pin_positions()[0].0;
        let vcc = app.netlist.net_at(pin).unwrap().anchor();
        assert_eq!(app.netlist.net_at(pin).unwrap().name.display(), "VCC");

        app.highlighted_net = app.netlist.net_at(pin).map(|n| n.anchor());
        assert_eq!(app.highlighted_net, Some(vcc));

        // A hole with no connectivity clears it.
        let bare = GridPos::new(20, 14);
        assert!(app.netlist.net_at(bare).is_none());
        app.highlighted_net = app.netlist.net_at(bare).map(|n| n.anchor());
        assert_eq!(app.highlighted_net, None);
    }

    /// The bug this fixes: wire waypoints are off-grid, so clicking a wire mid-span
    /// snaps to unrelated bare board and used to clear the highlight.
    /// The toolbar button is a fixed size, so a caption that outgrows it would silently
    /// overflow or clip. This environment cannot screenshot the window, so the invariant
    /// is measured with the font that actually renders it.
    /// Reported bug: drawing a solder line onto an existing net highlighted nothing until
    /// you deselected and reselected it. The conductor cannot resolve its own net at
    /// creation time because the netlist has not been rebuilt yet.
    #[test]
    fn a_new_conductor_highlights_its_net_without_reselecting() {
        for use_wire in [false, true] {
            let mut app = PerfworkApp {
                project: Project::new_starter(),
                ..Default::default()
            };
            app.rebuild_netlist();
            let vcc = app
                .netlist
                .nets
                .iter()
                .find(|n| n.name.display() == "VCC")
                .unwrap()
                .anchor();
            app.highlighted_net = None;

            // The VCC solder bus runs through (2,4); extend the net from there.
            let join = GridPos::new(2, 4);
            assert_eq!(app.netlist.net_at(join).unwrap().anchor(), vcc);

            if use_wire {
                let free = GridPos::new(6, 9);
                assert!(app.netlist.net_at(free).is_none());
                app.project.wires.push(Wire {
                    id: Uuid::new_v4(),
                    label: String::new(),
                    start_hole: join,
                    end_hole: free,
                    waypoints: Vec::new(),
                    mode: WireMode::Straight,
                    color: RgbColor::new(255, 0, 0),
                    thickness: 1.0,
                    layer: WireLayer::TopComponent,
                });
                app.pending_highlight_hole = Some(join);
            } else {
                app.active_solder_nodes = vec![join, GridPos::new(2, 5), GridPos::new(2, 6)];
                app.commit_active_solder_line();
                assert_eq!(
                    app.pending_highlight_hole,
                    Some(join),
                    "commit should defer the highlight"
                );
                // Nothing can be highlighted yet: the netlist has not seen the trace.
                assert_eq!(app.highlighted_net, None);
            }

            // The next frame's rebuild resolves it.
            app.rebuild_netlist();
            assert_eq!(
                app.highlighted_net,
                Some(vcc),
                "{} should highlight VCC immediately",
                if use_wire {
                    "a new wire"
                } else {
                    "a new solder line"
                }
            );
            assert_eq!(app.scroll_to_net, Some(vcc), "and the tree should follow");
            assert_eq!(app.pending_highlight_hole, None, "request is consumed once");
        }
    }

    /// Table-driven over the whole registry, so a newly added action with no coverage
    /// here fails loudly rather than being silently untested.
    #[test]
    fn every_context_action_applies_exactly_where_expected() {
        use crate::actions::context_actions;

        let mut app = PerfworkApp {
            project: Project::new_starter(),
            ..Default::default()
        };
        app.rebuild_netlist();

        let wire = app.project.wires[0].id;
        let solder = app.project.solder_lines[0].id;
        let connector = app
            .project
            .connectors
            .iter()
            .find(|c| c.designator == "TP1")
            .unwrap()
            .id;
        let board = app.project.boards[0].id;

        // (label, tool, selection) -> the exact set of action ids that should show up.
        let cases: Vec<(&str, ToolMode, Vec<SelectedItem>, Vec<&str>)> = vec![
            (
                "nothing selected, select tool",
                ToolMode::Select,
                vec![],
                vec![],
            ),
            (
                "wire selected",
                ToolMode::Select,
                vec![SelectedItem::Wire(wire)],
                vec![
                    "wire.mode",
                    "wire.color",
                    "conductor.width",
                    "selection.zoom",
                    "net.focus",
                    "net.pin",
                    "net.name",
                ],
            ),
            (
                "solder trace selected",
                ToolMode::Select,
                vec![SelectedItem::SolderLine(solder)],
                vec![
                    "conductor.width",
                    "selection.zoom",
                    "net.focus",
                    "net.pin",
                    "net.name",
                ],
            ),
            (
                "single-net connector selected",
                ToolMode::Select,
                vec![SelectedItem::Connector(connector)],
                vec!["selection.zoom", "net.focus", "net.pin", "net.name"],
            ),
            (
                "board selected (no net)",
                ToolMode::Select,
                vec![SelectedItem::Board(board)],
                vec!["selection.zoom"],
            ),
            (
                "wire tool, nothing selected",
                ToolMode::DrawWire,
                vec![],
                vec!["wire.mode", "wire.color"],
            ),
            (
                "header tool",
                ToolMode::PlaceHeaderLine,
                vec![],
                vec!["header.size", "placement.rotate"],
            ),
            (
                "pin placement tool",
                ToolMode::PlaceSinglePin,
                vec![],
                vec!["placement.rotate"],
            ),
        ];

        for (label, tool, selection, mut expected) in cases {
            app.tool = tool;
            app.selected = selection;
            app.highlighted_net = None;
            let scope = app.context_scope();
            let mut got: Vec<&str> = context_actions()
                .iter()
                .filter(|a| a.applies(&scope))
                .map(|a| a.id())
                .collect();
            got.sort_unstable();
            expected.sort_unstable();
            assert_eq!(got, expected, "mismatch for case: {label}");
        }
    }

    #[test]
    fn context_intents_delegate_to_real_state_changes() {
        let mut app = PerfworkApp {
            project: Project::new_starter(),
            ..Default::default()
        };
        app.rebuild_netlist();
        let wire = app.project.wires[0].id;
        app.selected = vec![SelectedItem::Wire(wire)];
        let anchor = app.netlist.net_of_wire(wire).unwrap().anchor();

        app.apply_context_intent(ContextIntent::ToggleFocusNet);
        assert!(app.focus_net_mode);
        app.apply_context_intent(ContextIntent::ToggleFocusNet);
        assert!(!app.focus_net_mode);

        app.apply_context_intent(ContextIntent::PinNet(anchor));
        assert_eq!(app.highlighted_net, Some(anchor));
        assert_eq!(app.scroll_to_net, Some(anchor));
        app.apply_context_intent(ContextIntent::UnpinNet);
        assert_eq!(app.highlighted_net, None);

        // Wire mode / colour retarget the selected wire, not the tool default.
        let before_default = app.wire_mode;
        app.apply_context_intent(ContextIntent::SetWireMode(WireMode::Spline));
        assert_eq!(
            app.project
                .wires
                .iter()
                .find(|w| w.id == wire)
                .unwrap()
                .mode,
            WireMode::Spline
        );
        assert_eq!(app.wire_mode, before_default, "tool default untouched");

        // With nothing selected the same intent moves the tool default instead.
        app.selected.clear();
        app.apply_context_intent(ContextIntent::SetWireMode(WireMode::Spline));
        assert_eq!(app.wire_mode, WireMode::Spline);

        // Width cycling walks the presets on the selected conductor.
        app.selected = vec![SelectedItem::Wire(wire)];
        let start = app
            .project
            .wires
            .iter()
            .find(|w| w.id == wire)
            .unwrap()
            .thickness;
        app.apply_context_intent(ContextIntent::CycleConductorWidth);
        let next = app
            .project
            .wires
            .iter()
            .find(|w| w.id == wire)
            .unwrap()
            .thickness;
        assert_ne!(start, next);
        assert!(WIRE_WIDTH_STEPS.contains(&next));

        // Naming opens the sidebar with the rename buffer primed.
        app.show_net_panel = false;
        app.apply_context_intent(ContextIntent::NameNet(anchor));
        assert!(app.show_net_panel);
        assert_eq!(
            app.net_rename_buffer.as_ref().map(|(h, _)| *h),
            Some(anchor)
        );

        // Zoom defers until the canvas rect is known.
        app.selected = vec![SelectedItem::Wire(wire)];
        app.apply_context_intent(ContextIntent::ZoomToSelection);
        let (min, max) = app.zoom_request.expect("zoom should be deferred");
        assert!(min.x < max.x && min.y <= max.y);
    }

    #[test]
    fn t_cycles_the_selected_conductor_width() {
        let mut app = PerfworkApp {
            project: Project::new_starter(),
            ..Default::default()
        };
        app.rebuild_netlist();
        let solder = app.project.solder_lines[0].id;
        app.selected = vec![SelectedItem::SolderLine(solder)];

        let mut seen = vec![];
        for _ in 0..SOLDER_WIDTH_STEPS.len() + 1 {
            key(&mut app, egui::Key::T, egui::Modifiers::NONE);
            let w = app
                .project
                .solder_lines
                .iter()
                .find(|s| s.id == solder)
                .unwrap()
                .width_scale;
            assert!(SOLDER_WIDTH_STEPS.contains(&w), "{w} is not a preset");
            seen.push(w);
        }
        // Cycling must visit more than one value and come back around.
        assert!(
            seen.iter().any(|w| *w != seen[0]),
            "T should actually change the width"
        );
        assert_eq!(seen.first(), seen.last(), "should wrap to the start");
    }

    /// Reported bug: Tab moved focus through the menu bar instead of switching routing
    /// mode, because egui latches focus direction before `update` ever runs.
    #[test]
    fn tab_switches_routing_mode_instead_of_moving_focus() {
        let tab_event = |pressed| egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };

        // While the wire tool is active, Tab is taken from egui and toggles the mode.
        let mut app = PerfworkApp {
            project: Project::new_starter(),
            tool: ToolMode::DrawWire,
            ..Default::default()
        };
        app.rebuild_netlist();
        let ctx = egui::Context::default();
        let mut raw = egui::RawInput {
            events: vec![tab_event(true), tab_event(false)],
            ..Default::default()
        };
        app.raw_input_hook(&ctx, &mut raw);
        assert!(
            raw.events.is_empty(),
            "both press and release must be removed so egui never sees the Tab"
        );
        assert!(app.stolen_tab);

        let before = app.wire_mode;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            app.handle_keyboard_shortcuts(ctx)
        });
        assert_ne!(app.wire_mode, before, "Tab should flip the routing mode");
        assert!(!app.stolen_tab, "the steal is consumed once");

        // With a wire selected, Tab retargets that wire rather than the tool default.
        app.tool = ToolMode::Select;
        let wire = app.project.wires[0].id;
        app.selected = vec![SelectedItem::Wire(wire)];
        let wire_mode_before = app
            .project
            .wires
            .iter()
            .find(|w| w.id == wire)
            .unwrap()
            .mode;
        let tool_default = app.wire_mode;
        let mut raw = egui::RawInput {
            events: vec![tab_event(true)],
            ..Default::default()
        };
        app.raw_input_hook(&ctx, &mut raw);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            app.handle_keyboard_shortcuts(ctx)
        });
        assert_ne!(
            app.project
                .wires
                .iter()
                .find(|w| w.id == wire)
                .unwrap()
                .mode,
            wire_mode_before
        );
        assert_eq!(app.wire_mode, tool_default, "tool default untouched");
    }

    #[test]
    fn tab_is_left_alone_when_it_means_focus_navigation() {
        let tab_event = egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let ctx = egui::Context::default();

        // No wire context: Tab belongs to egui so the menus stay keyboard-navigable.
        let mut app = PerfworkApp {
            project: Project::new_starter(),
            tool: ToolMode::Select,
            ..Default::default()
        };
        let mut raw = egui::RawInput {
            events: vec![tab_event.clone()],
            ..Default::default()
        };
        app.raw_input_hook(&ctx, &mut raw);
        assert_eq!(raw.events.len(), 1, "Tab must pass through to egui");
        assert!(!app.stolen_tab);

        // Nor is it stolen while a modal is open.
        app.tool = ToolMode::DrawWire;
        app.show_add_dialog = true;
        let mut raw = egui::RawInput {
            events: vec![tab_event.clone()],
            ..Default::default()
        };
        app.raw_input_hook(&ctx, &mut raw);
        assert_eq!(raw.events.len(), 1);

        // Shift+Tab is never stolen: reverse focus traversal keeps working.
        app.show_add_dialog = false;
        let mut raw = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    shift: true,
                    ..Default::default()
                },
            }],
            ..Default::default()
        };
        app.raw_input_hook(&ctx, &mut raw);
        assert_eq!(raw.events.len(), 1, "Shift+Tab is focus navigation");
        assert!(!app.stolen_tab);
    }

    #[test]
    fn every_toolbar_caption_fits_its_button() {
        use crate::icons::{TOOLBAR_BUTTON_SIZE, TOOLBAR_CAPTION_PADDING, TOOLBAR_CAPTION_POINTS};

        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let budget = TOOLBAR_BUTTON_SIZE.x - TOOLBAR_CAPTION_PADDING * 2.0;

        let mut captions: Vec<String> = TOOLBAR_TOOLS
            .iter()
            .map(|(mode, _, _)| toolbar_caption(mode))
            .collect();
        // The fixed captions on the same row share the same budget.
        captions.extend(["Add (A)".to_string(), "Ghost (G)".into(), "Flip (F)".into()]);

        for caption in captions {
            let width = ctx.fonts(|f| {
                f.layout_no_wrap(
                    caption.clone(),
                    egui::FontId::proportional(TOOLBAR_CAPTION_POINTS),
                    Color32::WHITE,
                )
                .size()
                .x
            });
            assert!(
                width <= budget,
                "caption {caption:?} is {width:.1}px, over the {budget:.1}px budget"
            );
        }
    }

    #[test]
    fn toolbar_captions_advertise_the_real_shortcut() {
        for (mode, _, _) in TOOLBAR_TOOLS {
            let key = mode
                .primary_key()
                .unwrap_or_else(|| panic!("{:?} is on the toolbar but has no key", mode));
            let caption = toolbar_caption(&mode);
            assert!(
                caption.ends_with(&format!("({})", key)),
                "{caption:?} should advertise {key}"
            );
            // And the key must be the one the handler actually listens for.
            assert!(
                mode.shortcut_hint().contains(key),
                "{:?}: caption says {key} but hint says {:?}",
                mode,
                mode.shortcut_hint()
            );
        }
    }

    #[test]
    fn clicking_a_wire_mid_span_resolves_its_net() {
        let mut app = PerfworkApp {
            project: Project::new_empty("t"),
            ..Default::default()
        };
        app.project
            .boards
            .push(Perfboard::new("B", GridPos::new(0, 0), 20, 20));
        let a = GridPos::new(1, 1);
        let b = GridPos::new(1, 10);
        let wire = Wire {
            id: Uuid::new_v4(),
            label: String::new(),
            start_hole: a,
            end_hole: b,
            // Bows far out to the right, nowhere near its endpoint holes.
            waypoints: vec![WorldPos::new(9.0, 5.0)],
            mode: WireMode::Straight,
            color: RgbColor::new(255, 0, 0),
            thickness: 1.0,
            layer: WireLayer::TopComponent,
        };
        app.project.wires.push(wire);
        let p1 = pin_fixture(&mut app.project, a);
        let p2 = pin_fixture(&mut app.project, b);
        app.project.connectors.push(p1);
        app.project.connectors.push(p2);
        app.rebuild_netlist();

        let expected = app.netlist.net_at(a).unwrap().anchor();

        // The bow's apex: the nearest hole is (9, 5), which carries nothing.
        let mid = WorldPos::new(9.0, 5.0);
        assert!(
            app.netlist.net_at(mid.snap_to_grid()).is_none(),
            "the hole under the wire must be bare for this test to mean anything"
        );
        assert_eq!(
            app.net_anchor_at(mid, mid.snap_to_grid()),
            Some(expected),
            "clicking the wire itself must resolve its net"
        );

        // Genuinely empty board still clears.
        let empty = WorldPos::new(16.0, 16.0);
        assert_eq!(app.net_anchor_at(empty, empty.snap_to_grid()), None);
    }

    #[test]
    fn a_multi_net_component_body_does_not_guess_a_net() {
        let mut app = PerfworkApp {
            project: Project::new_starter(),
            ..Default::default()
        };
        app.rebuild_netlist();

        // J2 is a 4-pin JST whose pins sit on several different nets.
        let j2 = app
            .project
            .connectors
            .iter()
            .find(|c| c.designator == "J2")
            .unwrap();
        let id = j2.id;
        let body = j2.world_aabb();
        assert!(
            app.netlist.nets_of_connector(id).len() > 1,
            "J2 should span several nets"
        );

        // A point inside the housing but off any pin hole: resolving it would be a coin
        // flip between the part's nets, so it must resolve to nothing.
        let off_pin = WorldPos::new((body.0.x + body.1.x) * 0.5, body.0.y + 0.02);
        assert_eq!(app.net_anchor_at(off_pin, off_pin.snap_to_grid()), None);

        // A single-net part *does* resolve from its body. TP1 is an unconnected test
        // point, which still forms a singleton net — one hole, one pin, no conductors.
        // That is deliberate: it is what a future "unconnected pin" check would key on.
        let tp = app
            .project
            .connectors
            .iter()
            .find(|c| c.designator == "TP1")
            .unwrap();
        let (tp_id, tp_hole) = (tp.id, tp.anchor);
        let nets = app.netlist.nets_of_connector(tp_id);
        assert_eq!(nets.len(), 1, "an unconnected pin is its own net");
        assert_eq!(nets[0].holes.len(), 1);
        assert!(nets[0].wires.is_empty() && nets[0].solder_lines.is_empty());

        let anchor = nets[0].anchor();
        let body_pt = WorldPos::new(tp_hole.col as f32, tp_hole.row as f32);
        assert_eq!(
            app.net_anchor_at(body_pt, tp_hole),
            Some(anchor),
            "a part with exactly one net resolves from anywhere on it"
        );
    }

    #[test]
    fn hover_preview_is_transient_and_pinning_is_not() {
        let mut app = PerfworkApp {
            project: Project::new_starter(),
            ..Default::default()
        };
        app.rebuild_netlist();
        let vcc = app
            .netlist
            .nets
            .iter()
            .find(|n| n.name.display() == "VCC")
            .unwrap()
            .anchor();

        // Hover alone highlights, but reports itself as unpinned so it draws faint.
        app.hovered_net = Some(vcc);
        assert_eq!(app.active_highlight(), Some((vcc, false)));

        // Pinned, no hover: solid.
        app.hovered_net = None;
        app.highlighted_net = Some(vcc);
        assert_eq!(app.active_highlight(), Some((vcc, true)));

        // Hovering the already-pinned net must not demote it to faint.
        app.hovered_net = Some(vcc);
        assert_eq!(app.active_highlight(), Some((vcc, true)));

        // Hovering a *different* net previews that one instead, faintly.
        let gnd = app
            .netlist
            .nets
            .iter()
            .find(|n| n.name.display() == "GND")
            .unwrap()
            .anchor();
        app.hovered_net = Some(gnd);
        assert_eq!(app.active_highlight(), Some((gnd, false)));
    }

    /// The case from the report: clicking J2's SDA pin lands on the connector, that pin,
    /// the wire ending in the same hole, the net, and the board — five panes, not one.
    #[test]
    fn clicking_a_pin_with_a_wire_on_it_yields_every_layer() {
        let mut app = PerfworkApp {
            project: Project::new_starter(),
            ..Default::default()
        };
        app.rebuild_netlist();

        let j2 = app
            .project
            .connectors
            .iter()
            .find(|c| c.designator == "J2")
            .unwrap();
        let (j2_id, pins) = (j2.id, j2.pin_positions());
        let (sda_index, (sda_hole, sda_name)) = pins
            .iter()
            .enumerate()
            .find(|(_, (_, name))| name == "SDA")
            .map(|(i, p)| (i, p.clone()))
            .expect("J2 has an SDA pin");

        let sda_wire = app
            .project
            .wires
            .iter()
            .find(|w| w.start_hole == sda_hole || w.end_hole == sda_hole)
            .expect("a wire lands on the SDA pin")
            .id;
        let net = app.netlist.net_at(sda_hole).unwrap();
        let (net_anchor, board) = (net.anchor(), app.project.boards[0].id);

        app.selected = vec![SelectedItem::Connector(j2_id)];
        app.focus_point = Some((WorldPos::from_grid(sda_hole), sda_hole));

        assert_eq!(
            app.context_stack(),
            vec![
                ContextLayer::Connector(j2_id),
                ContextLayer::Pin {
                    connector: j2_id,
                    index: sda_index
                },
                ContextLayer::Wire(sda_wire),
                ContextLayer::Net(net_anchor),
                ContextLayer::Board(board),
            ],
            "every level of the click should be present, outermost first"
        );
        assert_eq!(sda_name, "SDA");

        // A bare hole on the board layers down to just the board.
        let bare = GridPos::new(20, 15);
        assert!(app.netlist.net_at(bare).is_none());
        app.focus_point = Some((WorldPos::from_grid(bare), bare));
        assert_eq!(app.context_stack(), vec![ContextLayer::Board(board)]);

        // And with nothing selected there are no panes at all, however stale the point.
        app.selected.clear();
        assert!(app.context_stack().is_empty());
    }

    /// `pin_labels` is a sparse override vector that can be shorter than the pin count:
    /// `pin_positions` falls back to the footprint name for missing entries. Editing a
    /// high-index pin must backfill the real defaults, not blanks.
    #[test]
    fn editing_a_pin_label_backfills_shorter_override_vectors() {
        let mut app = PerfworkApp {
            project: Project::new_empty("t"),
            ..Default::default()
        };
        app.project
            .boards
            .push(Perfboard::new("B", GridPos::new(0, 0), 20, 20));
        // A DIP-8 with no overrides at all: pin_labels is empty, all 8 names come from
        // the footprint.
        let dip = PlacedConnector {
            id: Uuid::new_v4(),
            designator: "U1".into(),
            label: String::new(),
            anchor: GridPos::new(3, 3),
            rotation: Rotation::Deg0,
            kind: ConnectorKind::LibraryFootprint {
                footprint_id: "dip_8".into(),
            },
            body_color: RgbColor::new(36, 38, 44),
            pin_labels: Vec::new(),
            body_height_mm: None,
        };
        let id = dip.id;
        app.project.connectors.push(dip);
        app.rebuild_netlist();

        let before: Vec<String> = app.project.connectors[0]
            .pin_positions()
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        assert_eq!(before.len(), 8);
        assert!(app.project.connectors[0].pin_labels.is_empty());

        // Drive the pane and rename pin 6 (index 5).
        let pins = app.project.connectors[0].pin_positions();
        let target = 5;
        app.selected = vec![SelectedItem::Connector(id)];
        app.focus_point = Some((WorldPos::from_grid(pins[target].0), pins[target].0));

        let ctx = egui::Context::default();
        let mut time = 0.0;
        let mut draw = |app: &mut PerfworkApp, events| {
            time += 0.05;
            ctx.run(
                egui::RawInput {
                    time: Some(time),
                    events,
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 1200.0))),
                    ..Default::default()
                },
                |ctx| {
                    app.rebuild_netlist();
                    egui::SidePanel::right("net_panel").show(ctx, |ui| app.render_sidebar(ui));
                },
            )
        };
        let _ = draw(&mut app, vec![]);
        let output = draw(&mut app, vec![]);

        // Find the pin-name text field by its current value and click into it.
        let field = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == before[target] => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("the pin pane should show the pin's current name");

        let _ = draw(
            &mut app,
            vec![
                egui::Event::PointerMoved(field),
                egui::Event::PointerButton {
                    pos: field,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: field,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let _ = draw(&mut app, vec![egui::Event::Text("Z".into())]);

        let labels = &app.project.connectors[0].pin_labels;
        assert!(
            labels.len() > target,
            "the override vector must have grown to reach pin {target}"
        );
        assert!(
            labels.iter().take(target).all(|l| !l.is_empty()),
            "earlier pins must be backfilled with real names, not blanks: {labels:?}"
        );

        // Every other pin keeps the name it had; only the edited one changed.
        let after: Vec<String> = app.project.connectors[0]
            .pin_positions()
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        for i in 0..8 {
            if i != target {
                assert_eq!(after[i], before[i], "pin {i} should be untouched");
            }
        }
        assert_ne!(
            after[target], before[target],
            "the edited pin should change"
        );
    }

    #[test]
    fn selecting_a_wire_surfaces_its_net_in_the_sidebar() {
        let mut app = PerfworkApp {
            project: Project::new_starter(),
            ..Default::default()
        };
        app.rebuild_netlist();

        let wire = app
            .project
            .wires
            .iter()
            .find(|w| w.label == "I2C_SDA")
            .unwrap();
        let (wire_id, start) = (wire.id, wire.start_hole);
        app.selected = vec![SelectedItem::Wire(wire_id)];
        app.focus_point = Some((WorldPos::from_grid(start), start));

        let net = app.netlist.net_of_wire(wire_id).expect("wire has a net");
        assert_eq!(net.name.display(), "I2C_SDA");

        // Render the sidebar and confirm both the wire editor and its net appear.
        let ctx = egui::Context::default();
        let mut time = 0.0;
        let mut draw = |app: &mut PerfworkApp| {
            time += 0.05;
            ctx.run(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 1000.0))),
                    ..Default::default()
                },
                |ctx| {
                    app.rebuild_netlist();
                    egui::SidePanel::right("net_panel").show(ctx, |ui| app.render_sidebar(ui));
                },
            )
        };
        let _ = draw(&mut app);
        let output = draw(&mut app);
        let texts: Vec<String> = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.job.text.clone()),
                _ => None,
            })
            .collect();

        assert!(
            texts.iter().any(|t| t == "WIRE PROPERTIES"),
            "wire editor should be in the sidebar, got {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == "Net"),
            "the wire's net pane should sit directly beneath it, got {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.starts_with("I2C_SDA")),
            "and it should name the net"
        );
    }

    #[test]
    fn n_toggles_the_net_panel_and_esc_releases_the_highlight() {
        let mut app = fixture();
        // Open by default: all editing lives in the sidebar now, so starting hidden
        // would leave the user with no way to edit anything.
        assert!(app.show_net_panel);
        key(&mut app, egui::Key::N, egui::Modifiers::NONE);
        assert!(!app.show_net_panel);
        key(&mut app, egui::Key::N, egui::Modifiers::NONE);
        assert!(app.show_net_panel);

        // Esc drops the net highlight before it clears the selection.
        app.highlighted_net = Some(GridPos::new(2, 2));
        let keep = app.selected.clone();
        key(&mut app, egui::Key::Escape, egui::Modifiers::NONE);
        assert_eq!(app.highlighted_net, None);
        assert_eq!(app.selected, keep, "Esc should not also drop the selection");
        key(&mut app, egui::Key::Escape, egui::Modifiers::NONE);
        assert!(app.selected.is_empty());
    }

    #[test]
    fn projects_saved_before_nets_existed_still_load() {
        let mut project = Project::new_starter();
        project.net_labels.clear();
        let json = serde_json::to_string(&project).unwrap();

        // Strip the key entirely to mimic a file written by the previous version.
        let legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mut map = legacy.as_object().unwrap().clone();
        map.remove("net_labels");
        let legacy = serde_json::to_string(&serde_json::Value::Object(map)).unwrap();
        assert!(!legacy.contains("net_labels"));

        let loaded: Project = serde_json::from_str(&legacy).expect("legacy project must load");
        assert!(loaded.net_labels.is_empty());
        assert_eq!(loaded.boards.len(), project.boards.len());
        assert_eq!(loaded.connectors.len(), project.connectors.len());
    }

    #[test]
    fn library_entries_double_click_to_place_and_keep_place_buttons() {
        for (tab, entry, expected, button) in [
            (
                AddDialogTab::Boards,
                "Protoboard 5×7cm (70×50mm, 24×18)",
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
