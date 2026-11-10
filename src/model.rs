use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::library::get_footprint_by_id;

/// 1 grid unit = 2.54mm (standard 0.1 inch pitch)
pub const PITCH_MM: f32 = 2.54;

/// Global or board-local grid coordinate
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GridPos {
    pub col: i32,
    pub row: i32,
}

impl GridPos {
    pub const fn new(col: i32, row: i32) -> Self {
        Self { col, row }
    }

    pub fn add(self, dcol: i32, drow: i32) -> Self {
        Self {
            col: self.col + dcol,
            row: self.row + drow,
        }
    }

    pub fn manhattan_distance(self, other: Self) -> i32 {
        (self.col - other.col).abs() + (self.row - other.row).abs()
    }

    pub fn is_neighbor(self, other: Self) -> bool {
        let dc = (self.col - other.col).abs();
        let dr = (self.row - other.row).abs();
        (dc <= 1 && dr <= 1) && !(dc == 0 && dr == 0)
    }

    pub fn to_vec2(self) -> egui::Vec2 {
        egui::vec2(self.col as f32, self.row as f32)
    }
}

/// 2D floating-point position in world pitch units (where 1.0 = 1 hole pitch = 2.54mm)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorldPos {
    pub x: f32,
    pub y: f32,
}

impl WorldPos {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn from_grid(pos: GridPos) -> Self {
        Self {
            x: pos.col as f32,
            y: pos.row as f32,
        }
    }

    pub fn snap_to_grid(self) -> GridPos {
        GridPos {
            col: self.x.round() as i32,
            row: self.y.round() as i32,
        }
    }

    pub fn snap_half_grid(self) -> Self {
        Self {
            x: (self.x * 2.0).round() / 2.0,
            y: (self.y * 2.0).round() / 2.0,
        }
    }

    pub fn distance(self, other: Self) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

/// Style of identifiers along rows or columns of a perfboard
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AxisLabelStyle {
    Numbers1Based,
    Numbers0Based,
    LettersUpper,
    LettersLower,
    Hidden,
}

impl AxisLabelStyle {
    pub const ALL: [AxisLabelStyle; 5] = [
        AxisLabelStyle::Numbers1Based,
        AxisLabelStyle::Numbers0Based,
        AxisLabelStyle::LettersUpper,
        AxisLabelStyle::LettersLower,
        AxisLabelStyle::Hidden,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            AxisLabelStyle::Numbers1Based => "Numbers (1, 2, 3...)",
            AxisLabelStyle::Numbers0Based => "Numbers (0, 1, 2...)",
            AxisLabelStyle::LettersUpper => "Letters (A, B, C...)",
            AxisLabelStyle::LettersLower => "Letters (a, b, c...)",
            AxisLabelStyle::Hidden => "Hidden",
        }
    }

    pub fn format_index(self, index: u32) -> String {
        match self {
            AxisLabelStyle::Numbers1Based => (index + 1).to_string(),
            AxisLabelStyle::Numbers0Based => index.to_string(),
            AxisLabelStyle::LettersUpper => index_to_letters(index, true),
            AxisLabelStyle::LettersLower => index_to_letters(index, false),
            AxisLabelStyle::Hidden => String::new(),
        }
    }
}

fn index_to_letters(mut index: u32, uppercase: bool) -> String {
    let base = if uppercase { b'A' } else { b'a' };
    let mut chars = Vec::new();
    loop {
        let rem = (index % 26) as u8;
        chars.push((base + rem) as char);
        if index < 26 {
            break;
        }
        index = (index / 26) - 1;
    }
    chars.into_iter().rev().collect()
}

/// Substrate type of the board
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoardKind {
    /// Standard isolated-pad perfboard
    Perfboard,
    /// Horizontal copper strips (Veroboard style) with optional track cuts
    StripboardHorizontal,
    /// Vertical copper strips with optional track cuts
    StripboardVertical,
    /// Standard breadboard (5-hole terminal strips + center trench + optional power rails)
    Breadboard,
}

impl BoardKind {
    pub const ALL: [BoardKind; 4] = [
        BoardKind::Perfboard,
        BoardKind::StripboardHorizontal,
        BoardKind::StripboardVertical,
        BoardKind::Breadboard,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            BoardKind::Perfboard => "Perfboard (Isolated Pads)",
            BoardKind::StripboardHorizontal => "Stripboard (Horizontal Tracks)",
            BoardKind::StripboardVertical => "Stripboard (Vertical Tracks)",
            BoardKind::Breadboard => "Breadboard (Terminal Strips)",
        }
    }
}

/// Visual color theme of the PCB / board substrate
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoardTheme {
    ClassicGreen,
    PhenolicAmber,
    MatteBlack,
    SolderMaskBlue,
    BreadboardWhite,
}

impl BoardTheme {
    pub const ALL: [BoardTheme; 5] = [
        BoardTheme::ClassicGreen,
        BoardTheme::PhenolicAmber,
        BoardTheme::MatteBlack,
        BoardTheme::SolderMaskBlue,
        BoardTheme::BreadboardWhite,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            BoardTheme::ClassicGreen => "FR4 Classic Green",
            BoardTheme::PhenolicAmber => "Phenolic Amber / Brown",
            BoardTheme::MatteBlack => "Matte Black",
            BoardTheme::SolderMaskBlue => "Blue PCB",
            BoardTheme::BreadboardWhite => "Breadboard White",
        }
    }

    pub fn substrate_fill(self) -> egui::Color32 {
        match self {
            BoardTheme::ClassicGreen => egui::Color32::from_rgb(24, 78, 46),
            BoardTheme::PhenolicAmber => egui::Color32::from_rgb(138, 92, 42),
            BoardTheme::MatteBlack => egui::Color32::from_rgb(32, 35, 40),
            BoardTheme::SolderMaskBlue => egui::Color32::from_rgb(24, 64, 118),
            BoardTheme::BreadboardWhite => egui::Color32::from_rgb(235, 236, 232),
        }
    }

    pub fn substrate_border(self) -> egui::Color32 {
        match self {
            BoardTheme::ClassicGreen => egui::Color32::from_rgb(46, 136, 80),
            BoardTheme::PhenolicAmber => egui::Color32::from_rgb(186, 134, 72),
            BoardTheme::MatteBlack => egui::Color32::from_rgb(76, 82, 94),
            BoardTheme::SolderMaskBlue => egui::Color32::from_rgb(56, 118, 196),
            BoardTheme::BreadboardWhite => egui::Color32::from_rgb(185, 188, 182),
        }
    }

    pub fn label_color(self) -> egui::Color32 {
        match self {
            BoardTheme::BreadboardWhite => egui::Color32::from_rgb(75, 80, 88),
            _ => egui::Color32::from_rgba_unmultiplied(235, 242, 235, 210),
        }
    }
}

/// Physical substrate extents of a board, in millimetres.
///
/// Deliberately stored as *margins beyond the outermost hole centres* rather than as an
/// overall width/height. The hole lattice stays the single source of truth for topology;
/// this only says how much FR4 surrounds it. Resizing a board by columns or rows therefore
/// keeps its real-world border intact instead of silently rescaling it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoardOutline {
    /// Substrate beyond the first/last hole centre, left and right
    pub margin_x_mm: f32,
    /// Substrate beyond the first/last hole centre, top and bottom
    pub margin_y_mm: f32,
    /// Substrate thickness (1.6mm for standard FR4)
    pub thickness_mm: f32,
    /// Rounded corner radius of the board outline
    pub corner_radius_mm: f32,
}

impl Default for BoardOutline {
    /// The generic protoboard family measured from the common 5×7 / 4×6 / 3×7 / 2×8cm kit:
    /// every one of those boards has the same 5.8mm side and 3.6mm top/bottom border.
    fn default() -> Self {
        Self {
            margin_x_mm: 5.8,
            margin_y_mm: 3.6,
            thickness_mm: 1.6,
            corner_radius_mm: 1.0,
        }
    }
}

impl BoardOutline {
    /// Overall substrate size in mm for a hole field of `cols` × `rows`.
    pub fn size_mm(&self, cols: u32, rows: u32) -> (f32, f32) {
        let span = |n: u32| (n.max(1) - 1) as f32 * PITCH_MM;
        (
            span(cols) + 2.0 * self.margin_x_mm,
            span(rows) + 2.0 * self.margin_y_mm,
        )
    }

    /// Margins expressed in pitch units, for the renderer and world-space bounds.
    pub fn margin_pitch(&self) -> (f32, f32) {
        (self.margin_x_mm / PITCH_MM, self.margin_y_mm / PITCH_MM)
    }
}

/// Corner mounting hole geometry, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MountingSpec {
    /// Hole centre inset from each board edge
    pub inset_mm: f32,
    /// Drill diameter — this is the number that has to match a real standoff
    pub drill_mm: f32,
    /// Annular copper pad diameter (cosmetic; not exported as geometry)
    pub pad_mm: f32,
}

impl Default for MountingSpec {
    /// Measured off the 5×7 board: a small ~1.5mm tooling hole about 2.2mm in from each
    /// corner, not the M3 clearance hole these boards are often assumed to have.
    ///
    /// The photograph this came from resolves roughly 5 pixels per millimetre, which is
    /// enough to place the hole but not enough to call 1.5mm against 2.0mm with
    /// confidence. **Check this one with calipers before trusting it for a fastener** --
    /// it is the single number an enclosure standoff has to match, and it is editable per
    /// board in the inspector as well as here.
    fn default() -> Self {
        Self {
            inset_mm: 2.2,
            drill_mm: 1.5,
            pad_mm: 2.8,
        }
    }
}

/// Which side of the board an oversized edge pad strip sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeSide {
    Left,
    Right,
}

impl EdgeSide {
    pub const ALL: [EdgeSide; 2] = [EdgeSide::Left, EdgeSide::Right];

    pub fn prefix(self) -> char {
        match self {
            EdgeSide::Left => 'L',
            EdgeSide::Right => 'R',
        }
    }
}

/// Oversized elongated solder pads running down the left and right board edges.
///
/// These sit about 1.5 hole pitches outboard of the matrix on the real board, which is off
/// the integer lattice everything else is built on. Rather than round to 1 pitch (which
/// lands them on top of the row labels) or 2 (which hangs half the pad off the substrate),
/// the pad keeps a *lattice identity* for topology and a separate *millimetre position*
/// for geometry. Nets, DRC and hit-testing key off the former; drawing and export use the
/// latter.
///
/// Every dimension is editable per board so a pad can be matched to a measured one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EdgePads {
    pub left: bool,
    pub right: bool,
    /// Distance from the outermost hole column to the pad centres, in mm.
    /// 3.81mm is 1.5 pitches, measured off the 5×7 board.
    #[serde(default = "default_edge_pad_offset")]
    pub offset_mm: f32,
    /// Pad size across the board, in mm
    #[serde(default = "default_edge_pad_width")]
    pub width_mm: f32,
    /// Pad size along the board, in mm
    #[serde(default = "default_edge_pad_height")]
    pub height_mm: f32,
}

fn default_edge_pad_offset() -> f32 {
    3.81
}
fn default_edge_pad_width() -> f32 {
    2.6
}
fn default_edge_pad_height() -> f32 {
    1.3
}

impl Default for EdgePads {
    fn default() -> Self {
        Self {
            left: false,
            right: false,
            offset_mm: default_edge_pad_offset(),
            width_mm: default_edge_pad_width(),
            height_mm: default_edge_pad_height(),
        }
    }
}

impl EdgePads {
    pub const NONE: EdgePads = EdgePads {
        left: false,
        right: false,
        offset_mm: 3.81,
        width_mm: 2.6,
        height_mm: 1.3,
    };
    pub const BOTH: EdgePads = EdgePads {
        left: true,
        right: true,
        offset_mm: 3.81,
        width_mm: 2.6,
        height_mm: 1.3,
    };

    /// Half-extents of a pad in pitch units, for geometry that works in world space.
    pub fn half_extents_pitch(self) -> (f32, f32) {
        (
            self.width_mm * 0.5 / PITCH_MM,
            self.height_mm * 0.5 / PITCH_MM,
        )
    }

    pub fn enabled(self, side: EdgeSide) -> bool {
        match side {
            EdgeSide::Left => self.left,
            EdgeSide::Right => self.right,
        }
    }

    pub fn any(self) -> bool {
        self.left || self.right
    }
}

/// Minimum lattice separation between an edge pad and the hole matrix. A pad nearer than
/// one whole pitch would share a lattice column with the matrix and stop being addressable.
pub const MIN_EDGE_PAD_OFFSET: i32 = 1;

/// A perfboard, stripboard, or breadboard placed on the workspace plane
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Perfboard {
    pub id: Uuid,
    pub name: String,
    /// Top-left hole (col=0, row=0 of this board) position on the global grid
    pub origin: GridPos,
    /// Number of columns (X dimension)
    pub cols: u32,
    /// Number of rows (Y dimension)
    pub rows: u32,
    /// Label style along columns (top/bottom edges)
    pub col_label_style: AxisLabelStyle,
    /// Label style along rows (left/right edges)
    pub row_label_style: AxisLabelStyle,
    /// Count columns from the right-hand edge instead of the left.
    ///
    /// The board itself cannot be rotated, so this is how a design is matched to a
    /// physical board that happens to be the other way round on the bench.
    #[serde(default)]
    pub col_labels_reversed: bool,
    /// Count rows from the bottom edge instead of the top.
    #[serde(default)]
    pub row_labels_reversed: bool,
    /// Show labels on both sides (top+bottom, left+right)
    pub labels_both_sides: bool,
    /// Board substrate kind (Perfboard, Stripboard, Breadboard)
    pub kind: BoardKind,
    /// Visual theme
    pub theme: BoardTheme,
    /// Local hole positions (0..cols, 0..rows) where stripboard tracks are cut
    pub track_cuts: Vec<GridPos>,
    /// Show corner mounting holes
    pub mounting_holes: bool,
    /// Physical substrate extents. Defaulted so projects saved before it existed still load.
    #[serde(default)]
    pub outline: BoardOutline,
    /// Corner mounting hole geometry, used when `mounting_holes` is set
    #[serde(default)]
    pub mounting: MountingSpec,
    /// Oversized solder pads along the left/right board edges
    #[serde(default)]
    pub edge_pads: EdgePads,
}

impl Perfboard {
    pub fn new(name: impl Into<String>, origin: GridPos, cols: u32, rows: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            origin,
            cols: cols.max(1),
            rows: rows.max(1),
            col_label_style: AxisLabelStyle::Numbers1Based,
            row_label_style: AxisLabelStyle::LettersUpper,
            col_labels_reversed: false,
            row_labels_reversed: false,
            labels_both_sides: true,
            kind: BoardKind::Perfboard,
            theme: BoardTheme::ClassicGreen,
            track_cuts: Vec::new(),
            mounting_holes: true,
            outline: BoardOutline::default(),
            mounting: MountingSpec::default(),
            edge_pads: EdgePads::default(),
        }
    }

    /// Convert a local board hole coordinate `(0..cols, 0..rows)` to global `GridPos`
    pub fn local_to_global(&self, local: GridPos) -> GridPos {
        GridPos {
            col: self.origin.col + local.col,
            row: self.origin.row + local.row,
        }
    }

    /// Convert a global `GridPos` to local board coordinates if inside this board
    pub fn global_to_local(&self, global: GridPos) -> Option<GridPos> {
        let lc = global.col - self.origin.col;
        let lr = global.row - self.origin.row;
        if lc >= 0 && lr >= 0 && (lc as u32) < self.cols && (lr as u32) < self.rows {
            Some(GridPos { col: lc, row: lr })
        } else {
            None
        }
    }

    /// Check if a global grid position lies on this board's hole *matrix*.
    ///
    /// Deliberately excludes edge pads: callers that mean "is there copper to solder to"
    /// want [`Perfboard::provides_hole`], while callers that mean "which row/column is
    /// this" want the matrix only.
    pub fn contains_global_hole(&self, global: GridPos) -> bool {
        self.global_to_local(global).is_some()
    }

    /// Lattice offset of the edge pad columns, in whole pitches.
    ///
    /// This is the pad's *identity*, not where it is drawn: it is what makes an edge pad a
    /// `GridPos` the netlist and DRC can key off. The physical position comes from
    /// [`EdgePads::offset_mm`] and the two deliberately disagree by up to half a pitch.
    pub fn edge_pad_lattice_offset(&self) -> i32 {
        (self.edge_pads.offset_mm / PITCH_MM)
            .round()
            .max(MIN_EDGE_PAD_OFFSET as f32) as i32
    }

    /// Column index of an edge pad strip, in board-local coordinates.
    pub fn edge_pad_col(&self, side: EdgeSide) -> i32 {
        let off = self.edge_pad_lattice_offset();
        match side {
            EdgeSide::Left => -off,
            EdgeSide::Right => self.cols as i32 - 1 + off,
        }
    }

    /// World position an edge pad is actually drawn at, in pitch units.
    fn edge_pad_world_x(&self, side: EdgeSide) -> f32 {
        let off = self.edge_pads.offset_mm / PITCH_MM;
        match side {
            EdgeSide::Left => self.origin.col as f32 - off,
            EdgeSide::Right => (self.origin.col + self.cols as i32 - 1) as f32 + off,
        }
    }

    /// Where a hole this board provides actually sits in world space.
    ///
    /// Matrix holes are on the lattice; edge pads are not. Everything that draws, exports
    /// or measures a hole must go through this rather than `WorldPos::from_grid`, or edge
    /// pads end up drawn half a pitch from where they can be clicked.
    pub fn hole_world_pos(&self, global: GridPos) -> WorldPos {
        match self.edge_pad_at(global) {
            Some((side, row)) => {
                WorldPos::new(self.edge_pad_world_x(side), (self.origin.row + row) as f32)
            }
            None => WorldPos::from_grid(global),
        }
    }

    /// The edge pad under `world`, if any. Needed because an edge pad cannot be found by
    /// snapping to the nearest lattice point: it sits between two of them.
    pub fn edge_pad_hit(&self, world: WorldPos) -> Option<GridPos> {
        if !self.edge_pads.any() {
            return None;
        }
        let (hx, hy) = self.edge_pads.half_extents_pitch();
        // A little generous, so a pad is no harder to hit than an ordinary hole.
        let (hx, hy) = (hx.max(0.3), hy.max(0.3));
        for side in EdgeSide::ALL {
            if !self.edge_pads.enabled(side) {
                continue;
            }
            if (world.x - self.edge_pad_world_x(side)).abs() > hx {
                continue;
            }
            for r in self.edge_pad_rows() {
                if (world.y - (self.origin.row + r) as f32).abs() <= hy {
                    return Some(GridPos::new(
                        self.origin.col + self.edge_pad_col(side),
                        self.origin.row + r,
                    ));
                }
            }
        }
        None
    }

    /// Rows that carry an edge pad.
    ///
    /// The first and last rows are skipped when the board has corner mounting holes,
    /// because that is where those holes are — and it is what the real board does: its
    /// edge pad strip starts one row down from the corner.
    fn edge_pad_rows(&self) -> std::ops::Range<i32> {
        if self.mounting_holes && self.rows >= 3 {
            1..self.rows as i32 - 1
        } else {
            0..self.rows as i32
        }
    }

    /// Global positions of every enabled edge pad.
    pub fn edge_pad_holes(&self) -> Vec<GridPos> {
        let mut out = Vec::new();
        for side in EdgeSide::ALL {
            if !self.edge_pads.enabled(side) {
                continue;
            }
            let c = self.edge_pad_col(side);
            for r in self.edge_pad_rows() {
                out.push(self.local_to_global(GridPos::new(c, r)));
            }
        }
        out
    }

    /// Which edge pad, if any, a global position refers to.
    pub fn edge_pad_at(&self, global: GridPos) -> Option<(EdgeSide, i32)> {
        let row = global.row - self.origin.row;
        if !self.edge_pad_rows().contains(&row) {
            return None;
        }
        EdgeSide::ALL.into_iter().find_map(|side| {
            (self.edge_pads.enabled(side)
                && global.col - self.origin.col == self.edge_pad_col(side))
            .then_some((side, row))
        })
    }

    /// Every solderable hole this board offers: the matrix plus any edge pads.
    pub fn provides_hole(&self, global: GridPos) -> bool {
        self.contains_global_hole(global) || self.edge_pad_at(global).is_some()
    }

    /// World-space bounding box of the physical substrate, derived from [`BoardOutline`].
    ///
    /// Edge pads sit inside this rectangle rather than extending it — that is what the
    /// side margin is for on a real board.
    pub fn world_bounds(&self) -> (WorldPos, WorldPos) {
        let (mx, my) = self.outline.margin_pitch();
        let min = WorldPos::new(self.origin.col as f32 - mx, self.origin.row as f32 - my);
        let max = WorldPos::new(
            (self.origin.col + self.cols as i32 - 1) as f32 + mx,
            (self.origin.row + self.rows as i32 - 1) as f32 + my,
        );
        (min, max)
    }

    /// Overall substrate size in millimetres.
    pub fn size_mm(&self) -> (f32, f32) {
        self.outline.size_mm(self.cols, self.rows)
    }

    /// Mounting hole centres in board-local millimetres, origin at the top-left substrate
    /// corner, X right and Y down. Empty when the board has no mounting holes.
    pub fn mounting_centers_mm(&self) -> Vec<(f32, f32)> {
        if !self.mounting_holes {
            return Vec::new();
        }
        let (w, h) = self.size_mm();
        let i = self.mounting.inset_mm;
        // A board too small to carry a corner hole without eating into the hole field.
        if w < 4.0 * i || h < 4.0 * i {
            return Vec::new();
        }
        vec![(i, i), (w - i, i), (i, h - i), (w - i, h - i)]
    }

    /// Board-local millimetre position of a global hole, measured from the top-left
    /// substrate corner. This is the frame STEP and image export both work in.
    pub fn hole_to_local_mm(&self, global: GridPos) -> (f32, f32) {
        // Via hole_world_pos, so an edge pad reports where it physically is rather than
        // where its lattice identity sits. Export and rendering must agree on this or the
        // PNG decal lands offset from the holes in the STEP solid.
        let w = self.hole_world_pos(global);
        (
            (w.x - self.origin.col as f32) * PITCH_MM + self.outline.margin_x_mm,
            (w.y - self.origin.row as f32) * PITCH_MM + self.outline.margin_y_mm,
        )
    }

    pub fn contains_world_pos(&self, pos: WorldPos) -> bool {
        let (min, max) = self.world_bounds();
        pos.x >= min.x && pos.x <= max.x && pos.y >= min.y && pos.y <= max.y
    }

    /// Identifier shown for column `index`, counted from the left.
    ///
    /// The single place column numbering is decided. The board edge labels and every
    /// coordinate readout go through here, so a reversed board cannot end up labelled one
    /// way on the canvas and another way in the sidebar.
    pub fn col_label(&self, index: u32) -> String {
        self.col_label_style
            .format_index(self.col_label_index(index))
    }

    /// Identifier shown for row `index`, counted from the top.
    pub fn row_label(&self, index: u32) -> String {
        self.row_label_style
            .format_index(self.row_label_index(index))
    }

    fn col_label_index(&self, index: u32) -> u32 {
        if self.col_labels_reversed {
            self.cols.saturating_sub(1).saturating_sub(index)
        } else {
            index
        }
    }

    fn row_label_index(&self, index: u32) -> u32 {
        if self.row_labels_reversed {
            self.rows.saturating_sub(1).saturating_sub(index)
        } else {
            index
        }
    }

    pub fn format_hole_coord(&self, local: GridPos) -> String {
        // A hidden axis still needs *some* identifier in a coordinate readout, so it falls
        // back to the default style for that axis — reversal included, since the hole is
        // still in the same place on the board.
        let col_str = match self.col_label_style {
            AxisLabelStyle::Hidden => (self.col_label_index(local.col as u32) + 1).to_string(),
            _ => self.col_label(local.col as u32),
        };
        let row_str = match self.row_label_style {
            AxisLabelStyle::Hidden => {
                index_to_letters(self.row_label_index(local.row as u32), true)
            }
            _ => self.row_label(local.row as u32),
        };
        format!("{}{}", row_str, col_str)
    }

    /// Human-readable name for any hole this board provides, including edge pads
    /// (`L3`, `R12`), which have no column in the matrix to name them by.
    pub fn format_global_hole(&self, global: GridPos) -> Option<String> {
        if let Some(local) = self.global_to_local(global) {
            return Some(self.format_hole_coord(local));
        }
        self.edge_pad_at(global).map(|(side, row)| {
            let row_str = match self.row_label_style {
                AxisLabelStyle::Hidden => index_to_letters(self.row_label_index(row as u32), true),
                _ => self.row_label(row as u32),
            };
            format!("{}{}", side.prefix(), row_str)
        })
    }
}

/// Rotation in 90-degree increments clockwise
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rotation {
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}

impl Rotation {
    pub fn next_cw(self) -> Self {
        match self {
            Rotation::Deg0 => Rotation::Deg90,
            Rotation::Deg90 => Rotation::Deg180,
            Rotation::Deg180 => Rotation::Deg270,
            Rotation::Deg270 => Rotation::Deg0,
        }
    }

    pub fn degrees(self) -> i32 {
        match self {
            Rotation::Deg0 => 0,
            Rotation::Deg90 => 90,
            Rotation::Deg180 => 180,
            Rotation::Deg270 => 270,
        }
    }

    pub fn rotate_offset(self, dcol: i32, drow: i32) -> (i32, i32) {
        match self {
            Rotation::Deg0 => (dcol, drow),
            Rotation::Deg90 => (-drow, dcol),
            Rotation::Deg180 => (-dcol, -drow),
            Rotation::Deg270 => (drow, -dcol),
        }
    }

    pub fn rotate_vec2(self, x: f32, y: f32) -> (f32, f32) {
        match self {
            Rotation::Deg0 => (x, y),
            Rotation::Deg90 => (-y, x),
            Rotation::Deg180 => (-x, -y),
            Rotation::Deg270 => (y, -x),
        }
    }
}

/// Visual color palette for components, pins, and wires
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn to_color32(self) -> egui::Color32 {
        egui::Color32::from_rgb(self.r, self.g, self.b)
    }

    pub fn with_alpha(self, a: u8) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(self.r, self.g, self.b, a)
    }

    pub fn from_color32(c: egui::Color32) -> Self {
        Self {
            r: c.r(),
            g: c.g(),
            b: c.b(),
        }
    }
}

/// Kind of placed connector / pin / header / library component
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConnectorKind {
    /// Single connector pin in 1 hole
    SinglePin { square_pad: bool },
    /// Parametric pin header (`cols` x `rows`, e.g., 1xN or 2xN)
    HeaderLine { cols: u32, rows: u32, female: bool },
    /// Standard library component footprint (e.g. JST-XH, Screw Terminal, DIP Socket, etc.)
    LibraryFootprint { footprint_id: String },
}

/// A placed connector / pin / component on the grid
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacedConnector {
    pub id: Uuid,
    pub designator: String,
    pub label: String,
    /// Anchor position (Pin 1) on the global grid
    pub anchor: GridPos,
    pub rotation: Rotation,
    pub kind: ConnectorKind,
    pub body_color: RgbColor,
    /// Optional custom pin names (indexed by pin 0..N)
    pub pin_labels: Vec<String>,
    /// Per-placement override for the body height in mm, for parts sold in several heights.
    /// `None` uses the library value.
    #[serde(default)]
    pub body_height_mm: Option<f32>,
}

impl PlacedConnector {
    /// Vertical extent of the body above the board surface, in mm, as
    /// `(standoff, height)`. This is what the 3D export extrudes.
    pub fn body_profile_mm(&self) -> (f32, f32) {
        let (standoff, height) = match &self.kind {
            // A 2.54mm header post stands ~11.5mm proud of the board; the plastic spacer
            // is part of that, so there is no standoff below it.
            ConnectorKind::SinglePin { .. } | ConnectorKind::HeaderLine { .. } => (0.0, 11.5),
            ConnectorKind::LibraryFootprint { footprint_id } => get_footprint_by_id(footprint_id)
                .map(|fp| (fp.standoff_mm, fp.body_height_mm))
                // An unknown footprint id already renders as a 1x1 placeholder; give it a
                // low placeholder body rather than nothing, so it is visible in CAD.
                .unwrap_or((0.0, 3.0)),
        };
        (standoff, self.body_height_mm.unwrap_or(height))
    }
    /// Returns the global grid positions of all electrical pins of this connector
    pub fn pin_positions(&self) -> Vec<(GridPos, String)> {
        match &self.kind {
            ConnectorKind::SinglePin { .. } => {
                let lbl = self
                    .pin_labels
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "1".to_string());
                vec![(self.anchor, lbl)]
            }
            ConnectorKind::HeaderLine { cols, rows, .. } => {
                let mut pins = Vec::new();
                let mut idx = 1;
                for r in 0..(*rows as i32) {
                    for c in 0..(*cols as i32) {
                        let (dc, dr) = self.rotation.rotate_offset(c, r);
                        let lbl = self
                            .pin_labels
                            .get(idx - 1)
                            .cloned()
                            .unwrap_or_else(|| idx.to_string());
                        pins.push((self.anchor.add(dc, dr), lbl));
                        idx += 1;
                    }
                }
                pins
            }
            ConnectorKind::LibraryFootprint { footprint_id } => {
                if let Some(fp) = get_footprint_by_id(footprint_id) {
                    fp.pins
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            let (dc, dr) = self.rotation.rotate_offset(p.dcol, p.drow);
                            let lbl = self
                                .pin_labels
                                .get(i)
                                .cloned()
                                .unwrap_or_else(|| p.name.to_string());
                            (self.anchor.add(dc, dr), lbl)
                        })
                        .collect()
                } else {
                    vec![(self.anchor, "1".to_string())]
                }
            }
        }
    }

    /// Returns global grid positions that are physically blocked by the connector housing
    /// (excluding the connector's own electrical pin holes, e.g. JST-XH side overhang)
    pub fn blocked_overhang_holes(&self) -> Vec<GridPos> {
        match &self.kind {
            ConnectorKind::SinglePin { .. } | ConnectorKind::HeaderLine { .. } => Vec::new(),
            ConnectorKind::LibraryFootprint { footprint_id } => {
                if let Some(fp) = get_footprint_by_id(footprint_id) {
                    fp.blocked_offsets
                        .iter()
                        .map(|&(dc, dr)| {
                            let (rc, rr) = self.rotation.rotate_offset(dc, dr);
                            self.anchor.add(rc, rr)
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            }
        }
    }

    /// World-space 2D bounding polygon (4 corners) of the component courtyard / body
    pub fn body_rect_local(&self) -> (egui::Vec2, egui::Vec2) {
        match &self.kind {
            ConnectorKind::SinglePin { .. } => (egui::vec2(-0.46, -0.46), egui::vec2(0.46, 0.46)),
            ConnectorKind::HeaderLine { cols, rows, .. } => (
                egui::vec2(-0.48, -0.48),
                egui::vec2((*cols as f32 - 1.0) + 0.48, (*rows as f32 - 1.0) + 0.48),
            ),
            ConnectorKind::LibraryFootprint { footprint_id } => {
                if let Some(fp) = get_footprint_by_id(footprint_id) {
                    (
                        egui::vec2(fp.body_min.0, fp.body_min.1),
                        egui::vec2(fp.body_max.0, fp.body_max.1),
                    )
                } else {
                    (egui::vec2(-0.5, -0.5), egui::vec2(0.5, 0.5))
                }
            }
        }
    }

    pub fn world_aabb(&self) -> (WorldPos, WorldPos) {
        let (min_l, max_l) = self.body_rect_local();
        let corners = [
            (min_l.x, min_l.y),
            (max_l.x, min_l.y),
            (max_l.x, max_l.y),
            (min_l.x, max_l.y),
        ];
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for (cx, cy) in corners {
            let (rx, ry) = self.rotation.rotate_vec2(cx, cy);
            let wx = self.anchor.col as f32 + rx;
            let wy = self.anchor.row as f32 + ry;
            min_x = min_x.min(wx);
            min_y = min_y.min(wy);
            max_x = max_x.max(wx);
            max_y = max_y.max(wy);
        }
        (WorldPos::new(min_x, min_y), WorldPos::new(max_x, max_y))
    }

    pub fn contains_world_pos(&self, pos: WorldPos) -> bool {
        let (min, max) = self.world_aabb();
        pos.x >= min.x && pos.x <= max.x && pos.y >= min.y && pos.y <= max.y
    }
}

/// Wire routing mode: Straight polyline (angled segments) or smooth Catmull-Rom Spline
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireMode {
    Straight,
    Spline,
}

impl WireMode {
    pub fn display_name(self) -> &'static str {
        match self {
            WireMode::Straight => "Straight (Angled)",
            WireMode::Spline => "Smooth Spline",
        }
    }
}

/// Board layer for wires or solder
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireLayer {
    TopComponent,
    BottomSolder,
}

impl WireLayer {
    pub fn display_name(self) -> &'static str {
        match self {
            WireLayer::TopComponent => "Top (Component Side)",
            WireLayer::BottomSolder => "Bottom (Solder Side)",
        }
    }
}

/// A wire starting and ending at board holes, with arbitrary intermediate waypoints in world space
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wire {
    pub id: Uuid,
    pub label: String,
    pub start_hole: GridPos,
    pub end_hole: GridPos,
    /// Arbitrary intermediate points in world space (pitch units)
    pub waypoints: Vec<WorldPos>,
    pub mode: WireMode,
    pub color: RgbColor,
    pub thickness: f32,
    pub layer: WireLayer,
}

impl Wire {
    /// Returns all control points in world coordinates from start_hole through waypoints to end_hole
    pub fn all_control_points(&self) -> Vec<WorldPos> {
        self.control_points_between(
            WorldPos::from_grid(self.start_hole),
            WorldPos::from_grid(self.end_hole),
        )
    }

    /// Control points with the two endpoints supplied by the caller.
    ///
    /// A wire landing on an edge pad does not end on the lattice, and a wire has no way to
    /// know that by itself — only the board does. Callers that can resolve the endpoints
    /// (see `Project::hole_world_pos`) should, or the wire stops half a pitch short of the
    /// pad it is soldered to.
    pub fn control_points_between(&self, start: WorldPos, end: WorldPos) -> Vec<WorldPos> {
        let mut pts = Vec::with_capacity(self.waypoints.len() + 2);
        pts.push(start);
        pts.extend_from_slice(&self.waypoints);
        pts.push(end);
        pts
    }

    /// Sample the wire path into a sequence of world points (either straight segments or Catmull-Rom spline)
    pub fn sampled_points(&self) -> Vec<WorldPos> {
        self.sample(self.all_control_points())
    }

    /// Sample with endpoints resolved against the project, so edge pads land correctly.
    pub fn sampled_points_in(&self, project: &Project) -> Vec<WorldPos> {
        self.sample(self.control_points_between(
            project.hole_world_pos(self.start_hole),
            project.hole_world_pos(self.end_hole),
        ))
    }

    fn sample(&self, ctrl: Vec<WorldPos>) -> Vec<WorldPos> {
        match self.mode {
            WireMode::Straight => ctrl,
            WireMode::Spline => sample_catmull_rom(&ctrl, 16),
        }
    }

    pub fn distance_to_point(&self, pos: WorldPos) -> f32 {
        let pts = self.sampled_points();
        let mut min_dist = f32::INFINITY;
        for pair in pts.windows(2) {
            let d = point_to_segment_distance(pos, pair[0], pair[1]);
            if d < min_dist {
                min_dist = d;
            }
        }
        min_dist
    }
}

/// Evaluate a centripetal/uniform Catmull-Rom spline through `pts`
pub fn sample_catmull_rom(pts: &[WorldPos], subdivisions_per_segment: usize) -> Vec<WorldPos> {
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity((pts.len() - 1) * subdivisions_per_segment + 1);
    let n = pts.len();
    for i in 0..(n - 1) {
        let p0 = if i == 0 {
            WorldPos::new(2.0 * pts[0].x - pts[1].x, 2.0 * pts[0].y - pts[1].y)
        } else {
            pts[i - 1]
        };
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = if i + 2 >= n {
            WorldPos::new(
                2.0 * pts[n - 1].x - pts[n - 2].x,
                2.0 * pts[n - 1].y - pts[n - 2].y,
            )
        } else {
            pts[i + 2]
        };

        let steps = subdivisions_per_segment.max(2);
        for s in 0..steps {
            let t = s as f32 / steps as f32;
            let t2 = t * t;
            let t3 = t2 * t;

            let x = 0.5
                * ((2.0 * p1.x)
                    + (-p0.x + p2.x) * t
                    + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
                    + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3);
            let y = 0.5
                * ((2.0 * p1.y)
                    + (-p0.y + p2.y) * t
                    + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
                    + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3);
            out.push(WorldPos::new(x, y));
        }
    }
    if let Some(&last) = pts.last() {
        out.push(last);
    }
    out
}

pub fn point_to_segment_distance(p: WorldPos, a: WorldPos, b: WorldPos) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-6 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
    let proj = WorldPos::new(a.x + t * dx, a.y + t * dy);
    p.distance(proj)
}

/// A solder bridge / solder line spanning adjacent holes on the board.
/// Holes covered by a solder line are blocked for new connector pins (unless a pin is already soldered into it),
/// and multiple solder lines can connect to each other at shared holes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolderLine {
    pub id: Uuid,
    pub nodes: Vec<GridPos>,
    pub width_scale: f32,
}

impl SolderLine {
    pub fn new(nodes: Vec<GridPos>) -> Self {
        Self {
            id: Uuid::new_v4(),
            nodes,
            width_scale: 1.0,
        }
    }

    pub fn contains_hole(&self, hole: GridPos) -> bool {
        self.nodes.contains(&hole)
    }

    pub fn distance_to_point(&self, pos: WorldPos) -> f32 {
        if self.nodes.is_empty() {
            return f32::INFINITY;
        }
        if self.nodes.len() == 1 {
            return pos.distance(WorldPos::from_grid(self.nodes[0]));
        }
        let mut min_d = f32::INFINITY;
        for pair in self.nodes.windows(2) {
            let d = point_to_segment_distance(
                pos,
                WorldPos::from_grid(pair[0]),
                WorldPos::from_grid(pair[1]),
            );
            min_d = min_d.min(d);
        }
        min_d
    }
}

/// A user-authored net name, anchored to a board hole.
///
/// Nets themselves are *derived* from geometry on every rebuild and therefore have no
/// stable identity to hang a name on. Anchoring the name to a hole instead makes both
/// naming behaviours fall out of the union-find with no special cases: wiring an unnamed
/// group onto a labelled hole absorbs it into that net, and bridging two differently
/// labelled nets produces one net holding two names — a conflict, by definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetLabel {
    pub id: Uuid,
    pub hole: GridPos,
    pub name: String,
}

impl NetLabel {
    pub fn new(hole: GridPos, name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            hole,
            name: name.into(),
        }
    }
}

/// Mechanical occupancy status of a single grid hole.
///
/// This answers "can a pin physically go here", *not* "what is this connected to" —
/// electrical connectivity lives in [`crate::netlist::Netlist`]. A hole under a JST side
/// wall is blocked here and connected to nothing there; a hole on a powered strip is the
/// reverse. The two are orthogonal and neither replaces the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoleOccupancy {
    /// Board this hole belongs to (if any)
    pub board_id: Option<Uuid>,
    /// Connector that has an electrical pin inserted in this hole
    pub pin_owner: Option<(Uuid, String, String)>, // (connector_id, designator, pin_name)
    /// Connector whose physical housing overhangs and blocks this hole
    pub blocked_by_housing: Option<(Uuid, String)>, // (connector_id, designator)
    /// Solder lines passing through this hole
    pub solder_lines: Vec<Uuid>,
    /// Stripboard track cut at this hole
    pub has_track_cut: bool,
}

impl HoleOccupancy {
    /// Returns true if a new connector pin CANNOT be placed into this hole
    /// Rules:
    /// 1. Cannot place a pin if another pin is already in this hole
    /// 2. Cannot place a pin if a connector body overhangs and blocks this hole (e.g. JST-XH side overhang)
    /// 3. Cannot place a pin if a solder line spans this hole (solder lines block holes for pins!)
    pub fn is_blocked_for_pin(&self, ignore_connector: Option<Uuid>) -> Option<String> {
        if let Some((cid, ref des, ref pin)) = self.pin_owner {
            if Some(cid) != ignore_connector {
                return Some(format!("Occupied by {} pin {}", des, pin));
            }
        }
        if let Some((cid, ref des)) = self.blocked_by_housing {
            if Some(cid) != ignore_connector {
                return Some(format!("Blocked by {} housing overhang", des));
            }
        }
        if !self.solder_lines.is_empty() {
            return Some("Blocked by solder trace".to_string());
        }
        None
    }

    /// Returns true if a solder line can use this hole
    /// Solder lines can connect across empty holes, holes with other solder lines, or solder onto pins,
    /// but cannot be placed where there is no board or where a track cut shouldn't block solder.
    pub fn can_place_solder(&self) -> bool {
        self.board_id.is_some()
    }
}

/// Complete Perfwork project document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub boards: Vec<Perfboard>,
    pub connectors: Vec<PlacedConnector>,
    pub wires: Vec<Wire>,
    pub solder_lines: Vec<SolderLine>,
    /// User-authored net names. `serde(default)` so projects saved before nets existed still load.
    #[serde(default)]
    pub net_labels: Vec<NetLabel>,
    next_designator_counter: u32,
}

impl Default for Project {
    fn default() -> Self {
        Self::new_starter()
    }
}

impl Project {
    pub fn new_empty(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            boards: Vec::new(),
            connectors: Vec::new(),
            wires: Vec::new(),
            solder_lines: Vec::new(),
            net_labels: Vec::new(),
            next_designator_counter: 1,
        }
    }

    /// Creates a rich, realistic starter circuit showcasing Perfboards, JST-XH connectors (with blocked overhang holes),
    /// pin headers, DIP IC socket, straight and spline wires, and solder traces.
    pub fn new_starter() -> Self {
        let mut main_board = Perfboard::new("Sensor Controller PCB", GridPos::new(0, 0), 26, 18);
        main_board.col_label_style = AxisLabelStyle::Numbers1Based;
        main_board.row_label_style = AxisLabelStyle::LettersUpper;
        main_board.theme = BoardTheme::ClassicGreen;

        let mut breakout_board =
            Perfboard::new("Power & Aux Stripboard", GridPos::new(30, 2), 12, 14);
        breakout_board.col_label_style = AxisLabelStyle::Numbers1Based;
        breakout_board.row_label_style = AxisLabelStyle::LettersUpper;
        breakout_board.kind = BoardKind::StripboardHorizontal;
        breakout_board.theme = BoardTheme::PhenolicAmber;
        breakout_board.track_cuts =
            vec![GridPos::new(5, 3), GridPos::new(5, 4), GridPos::new(5, 5)];

        let jst_power = PlacedConnector {
            id: Uuid::new_v4(),
            designator: "J1".to_string(),
            label: "12V_IN (JST-XH 2P)".to_string(),
            anchor: GridPos::new(2, 2),
            rotation: Rotation::Deg0,
            kind: ConnectorKind::LibraryFootprint {
                footprint_id: "jst_xh_2".to_string(),
            },
            body_color: RgbColor::new(238, 236, 224),
            pin_labels: vec!["VCC".to_string(), "GND".to_string()],
            body_height_mm: None,
        };

        let jst_sensor = PlacedConnector {
            id: Uuid::new_v4(),
            designator: "J2".to_string(),
            label: "I2C Sensor (JST-XH 4P)".to_string(),
            anchor: GridPos::new(10, 2),
            rotation: Rotation::Deg0,
            kind: ConnectorKind::LibraryFootprint {
                footprint_id: "jst_xh_4".to_string(),
            },
            body_color: RgbColor::new(238, 236, 224),
            pin_labels: vec![
                "VCC".to_string(),
                "GND".to_string(),
                "SDA".to_string(),
                "SCL".to_string(),
            ],
            body_height_mm: None,
        };

        let mcu_header_left = PlacedConnector {
            id: Uuid::new_v4(),
            designator: "H1".to_string(),
            label: "MCU Left Header".to_string(),
            anchor: GridPos::new(7, 7),
            rotation: Rotation::Deg90,
            kind: ConnectorKind::HeaderLine {
                cols: 8,
                rows: 1,
                female: true,
            },
            body_color: RgbColor::new(42, 46, 54),
            pin_labels: vec![
                "3V3".into(),
                "GND".into(),
                "D2".into(),
                "D3".into(),
                "SDA".into(),
                "SCL".into(),
                "A0".into(),
                "5V".into(),
            ],
            body_height_mm: None,
        };

        let mcu_header_right = PlacedConnector {
            id: Uuid::new_v4(),
            designator: "H2".to_string(),
            label: "MCU Right Header".to_string(),
            anchor: GridPos::new(13, 7),
            rotation: Rotation::Deg90,
            kind: ConnectorKind::HeaderLine {
                cols: 8,
                rows: 1,
                female: true,
            },
            body_color: RgbColor::new(42, 46, 54),
            pin_labels: (9..=16).map(|i| format!("P{}", i)).collect(),
            body_height_mm: None,
        };

        let dip_ic = PlacedConnector {
            id: Uuid::new_v4(),
            designator: "U1".to_string(),
            label: "NE555 / OpAmp".to_string(),
            anchor: GridPos::new(18, 8),
            rotation: Rotation::Deg0,
            kind: ConnectorKind::LibraryFootprint {
                footprint_id: "dip_8".to_string(),
            },
            body_color: RgbColor::new(36, 38, 44),
            pin_labels: vec![
                "GND".into(),
                "TRIG".into(),
                "OUT".into(),
                "RST".into(),
                "CTRL".into(),
                "THR".into(),
                "DIS".into(),
                "VCC".into(),
            ],
            body_height_mm: None,
        };

        let test_pin_1 = PlacedConnector {
            id: Uuid::new_v4(),
            designator: "TP1".to_string(),
            label: "TP_SDA".to_string(),
            anchor: GridPos::new(23, 5),
            rotation: Rotation::Deg0,
            kind: ConnectorKind::SinglePin { square_pad: true },
            body_color: RgbColor::new(225, 185, 65),
            pin_labels: vec!["SDA".into()],
            body_height_mm: None,
        };

        let test_pin_2 = PlacedConnector {
            id: Uuid::new_v4(),
            designator: "TP2".to_string(),
            label: "TP_SCL".to_string(),
            anchor: GridPos::new(23, 6),
            rotation: Rotation::Deg0,
            kind: ConnectorKind::SinglePin { square_pad: false },
            body_color: RgbColor::new(225, 185, 65),
            pin_labels: vec!["SCL".into()],
            body_height_mm: None,
        };

        let aux_terminal = PlacedConnector {
            id: Uuid::new_v4(),
            designator: "J3".to_string(),
            label: "Out Terminal".to_string(),
            anchor: GridPos::new(32, 4),
            rotation: Rotation::Deg0,
            kind: ConnectorKind::LibraryFootprint {
                footprint_id: "screw_term_2".to_string(),
            },
            body_color: RgbColor::new(38, 148, 92),
            pin_labels: vec!["OUT+".into(), "OUT-".into()],
            body_height_mm: None,
        };

        // Solder lines demonstrating multi-hole traces and T-junctions
        let solder_vcc_bus = SolderLine::new(vec![
            GridPos::new(2, 2),
            GridPos::new(2, 3),
            GridPos::new(2, 4),
            GridPos::new(3, 4),
            GridPos::new(4, 4),
            GridPos::new(5, 4),
        ]);

        let solder_gnd_bus = SolderLine::new(vec![
            GridPos::new(3, 2),
            GridPos::new(3, 3),
            GridPos::new(3, 15),
            GridPos::new(4, 15),
            GridPos::new(5, 15),
            GridPos::new(6, 15),
            GridPos::new(7, 15),
        ]);

        let solder_gnd_branch = SolderLine::new(vec![
            GridPos::new(7, 8),
            GridPos::new(6, 8),
            GridPos::new(5, 8),
        ]);

        let solder_mcu_pads = SolderLine::new(vec![
            GridPos::new(7, 11),
            GridPos::new(8, 11),
            GridPos::new(9, 11),
        ]);

        // Wires demonstrating both Straight (angled) and Spline modes
        let wire_sda = Wire {
            id: Uuid::new_v4(),
            label: "I2C_SDA".to_string(),
            start_hole: GridPos::new(12, 2),
            end_hole: GridPos::new(9, 11),
            waypoints: vec![WorldPos::new(12.0, 5.0), WorldPos::new(9.0, 5.0)],
            mode: WireMode::Straight,
            color: RgbColor::new(56, 182, 255),
            thickness: 1.0,
            layer: WireLayer::TopComponent,
        };

        let wire_scl = Wire {
            id: Uuid::new_v4(),
            label: "I2C_SCL".to_string(),
            start_hole: GridPos::new(13, 2),
            end_hole: GridPos::new(7, 12),
            waypoints: vec![
                WorldPos::new(13.0, 6.0),
                WorldPos::new(10.0, 6.0),
                WorldPos::new(10.0, 12.0),
            ],
            mode: WireMode::Straight,
            color: RgbColor::new(250, 200, 56),
            thickness: 1.0,
            layer: WireLayer::TopComponent,
        };

        let wire_vcc_spline = Wire {
            id: Uuid::new_v4(),
            label: "VCC_FEED".to_string(),
            start_hole: GridPos::new(5, 4),
            end_hole: GridPos::new(10, 2),
            waypoints: vec![WorldPos::new(6.8, 1.4)],
            mode: WireMode::Spline,
            color: RgbColor::new(238, 72, 72),
            thickness: 1.1,
            layer: WireLayer::TopComponent,
        };

        let wire_interboard = Wire {
            id: Uuid::new_v4(),
            label: "AUX_OUT".to_string(),
            start_hole: GridPos::new(18, 10),
            end_hole: GridPos::new(31, 4),
            waypoints: vec![WorldPos::new(24.5, 9.5), WorldPos::new(27.5, 4.2)],
            mode: WireMode::Spline,
            color: RgbColor::new(168, 112, 255),
            thickness: 1.1,
            layer: WireLayer::TopComponent,
        };

        Self {
            name: "Sensor Controller Prototype".to_string(),
            boards: vec![main_board, breakout_board],
            connectors: vec![
                jst_power,
                jst_sensor,
                mcu_header_left,
                mcu_header_right,
                dip_ic,
                test_pin_1,
                test_pin_2,
                aux_terminal,
            ],
            wires: vec![wire_sda, wire_scl, wire_vcc_spline, wire_interboard],
            solder_lines: vec![
                solder_vcc_bus,
                solder_gnd_bus,
                solder_gnd_branch,
                solder_mcu_pads,
            ],
            net_labels: vec![
                NetLabel::new(GridPos::new(2, 2), "VCC"),
                NetLabel::new(GridPos::new(3, 2), "GND"),
                NetLabel::new(GridPos::new(12, 2), "I2C_SDA"),
                NetLabel::new(GridPos::new(13, 2), "I2C_SCL"),
            ],
            next_designator_counter: 10,
        }
    }

    pub fn next_designator(&mut self, prefix: &str) -> String {
        let d = format!("{}{}", prefix, self.next_designator_counter);
        self.next_designator_counter += 1;
        d
    }

    /// The topmost board offering a solderable hole at `global` — matrix hole or edge pad.
    pub fn find_board_at_hole(&self, global: GridPos) -> Option<&Perfboard> {
        self.boards.iter().rev().find(|b| b.provides_hole(global))
    }

    /// Where a hole sits in world space, accounting for boards whose edge pads are drawn
    /// off the lattice. Falls back to the lattice position for holes on no board, so this
    /// is always safe to use in place of `WorldPos::from_grid`.
    pub fn hole_world_pos(&self, global: GridPos) -> WorldPos {
        self.find_board_at_hole(global)
            .map(|b| b.hole_world_pos(global))
            .unwrap_or_else(|| WorldPos::from_grid(global))
    }

    /// Resolve a pointer position to the hole it means.
    ///
    /// Snapping to the nearest lattice point is right for everything except edge pads,
    /// which sit half a pitch off it — those are hit-tested against their drawn rectangle
    /// first. This is the single entry point for "what hole is under the cursor"; going
    /// straight to `snap_to_grid` makes edge pads unclickable.
    pub fn hole_at_world(&self, world: WorldPos) -> GridPos {
        self.boards
            .iter()
            .rev()
            .find_map(|b| b.edge_pad_hit(world))
            .unwrap_or_else(|| world.snap_to_grid())
    }

    pub fn find_board_at_world(&self, pos: WorldPos) -> Option<&Perfboard> {
        self.boards.iter().rev().find(|b| b.contains_world_pos(pos))
    }

    /// Evaluate the complete hole occupancy state at `global`
    pub fn hole_occupancy(&self, global: GridPos) -> HoleOccupancy {
        let board = self.find_board_at_hole(global);
        let board_id = board.map(|b| b.id);
        let has_track_cut = board
            .and_then(|b| {
                let local = b.global_to_local(global)?;
                Some(b.track_cuts.contains(&local))
            })
            .unwrap_or(false);

        let mut pin_owner = None;
        let mut blocked_by_housing = None;

        for conn in &self.connectors {
            for (pin_pos, pin_name) in conn.pin_positions() {
                if pin_pos == global {
                    pin_owner = Some((conn.id, conn.designator.clone(), pin_name));
                    break;
                }
            }
            for overhang_pos in conn.blocked_overhang_holes() {
                if overhang_pos == global {
                    blocked_by_housing = Some((conn.id, conn.designator.clone()));
                }
            }
        }

        let solder_lines = self
            .solder_lines
            .iter()
            .filter(|s| s.contains_hole(global))
            .map(|s| s.id)
            .collect();

        HoleOccupancy {
            board_id,
            pin_owner,
            blocked_by_housing,
            solder_lines,
            has_track_cut,
        }
    }

    /// Validate whether a connector can be placed at its current anchor & rotation
    /// Returns `Ok(())` if valid, or `Err(Vec<(GridPos, String)>)` listing blocked holes and reasons.
    pub fn validate_connector_placement(
        &self,
        connector: &PlacedConnector,
        ignore_connector_id: Option<Uuid>,
    ) -> Result<(), Vec<(GridPos, String)>> {
        let mut conflicts = Vec::new();
        for (pin_pos, _pin_name) in connector.pin_positions() {
            let occ = self.hole_occupancy(pin_pos);
            if occ.board_id.is_none() {
                conflicts.push((pin_pos, "Outside any perfboard hole".to_string()));
                continue;
            }
            if let Some(reason) = occ.is_blocked_for_pin(ignore_connector_id) {
                conflicts.push((pin_pos, reason));
            }
        }

        // Also check that housing overhang holes don't collide with existing pins
        for overhang_pos in connector.blocked_overhang_holes() {
            let occ = self.hole_occupancy(overhang_pos);
            if let Some((cid, des, pin)) = occ.pin_owner {
                if Some(cid) != ignore_connector_id {
                    conflicts.push((
                        overhang_pos,
                        format!("Housing overhangs onto {} pin {}", des, pin),
                    ));
                }
            }
        }

        if conflicts.is_empty() {
            Ok(())
        } else {
            Err(conflicts)
        }
    }
}

#[cfg(test)]
mod label_tests {
    use super::*;

    fn board() -> Perfboard {
        let mut b = Perfboard::new("B", GridPos::new(5, 3), 24, 18);
        b.col_label_style = AxisLabelStyle::Numbers1Based;
        b.row_label_style = AxisLabelStyle::LettersUpper;
        b
    }

    #[test]
    fn reversing_counts_from_the_far_edge() {
        let mut b = board();
        assert_eq!(b.col_label(0), "1");
        assert_eq!(b.col_label(23), "24");
        assert_eq!(b.row_label(0), "A");
        assert_eq!(b.row_label(17), "R");

        b.col_labels_reversed = true;
        b.row_labels_reversed = true;
        assert_eq!(b.col_label(0), "24");
        assert_eq!(b.col_label(23), "1");
        assert_eq!(b.row_label(0), "R");
        assert_eq!(b.row_label(17), "A");
    }

    #[test]
    fn the_axes_reverse_independently() {
        let mut b = board();
        b.col_labels_reversed = true;
        assert_eq!(b.col_label(0), "24");
        assert_eq!(b.row_label(0), "A", "rows must be untouched");

        let mut b = board();
        b.row_labels_reversed = true;
        assert_eq!(b.col_label(0), "1", "columns must be untouched");
        assert_eq!(b.row_label(0), "R");
    }

    /// The canvas edge labels and every coordinate readout have to agree, or the sidebar
    /// names a hole the board calls something else.
    #[test]
    fn hole_coordinates_follow_the_same_reversal_as_the_edge_labels() {
        for (rc, rr) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut b = board();
            b.col_labels_reversed = rc;
            b.row_labels_reversed = rr;
            for (col, row) in [(0, 0), (23, 17), (7, 11)] {
                let coord = b.format_hole_coord(GridPos::new(col, row));
                let expected = format!("{}{}", b.row_label(row as u32), b.col_label(col as u32));
                assert_eq!(coord, expected, "at ({col},{row}) with reversal ({rc},{rr})");
            }
        }
    }

    /// Hiding an axis still has to produce an identifier for readouts, and that fallback
    /// must respect reversal too — the hole has not moved.
    #[test]
    fn hidden_axes_still_reverse_in_coordinate_readouts() {
        let mut b = board();
        b.col_label_style = AxisLabelStyle::Hidden;
        b.row_label_style = AxisLabelStyle::Hidden;
        assert_eq!(b.format_hole_coord(GridPos::new(0, 0)), "A1");

        b.col_labels_reversed = true;
        b.row_labels_reversed = true;
        assert_eq!(b.format_hole_coord(GridPos::new(0, 0)), "R24");
    }

    #[test]
    fn edge_pad_names_reverse_with_their_rows() {
        let mut b = board();
        b.edge_pads = EdgePads::BOTH;
        b.mounting_holes = false;
        let top_left = b.local_to_global(GridPos::new(b.edge_pad_col(EdgeSide::Left), 0));
        assert_eq!(b.format_global_hole(top_left).as_deref(), Some("LA"));

        b.row_labels_reversed = true;
        assert_eq!(b.format_global_hole(top_left).as_deref(), Some("LR"));
    }

    /// Reversal is presentation only. It must never move a hole, or a saved project would
    /// change shape when the labels are flipped.
    #[test]
    fn reversal_does_not_move_any_hole() {
        let plain = board();
        let mut flipped = board();
        flipped.col_labels_reversed = true;
        flipped.row_labels_reversed = true;

        assert_eq!(plain.world_bounds(), flipped.world_bounds());
        for (col, row) in [(0, 0), (23, 17), (9, 4)] {
            let g = plain.local_to_global(GridPos::new(col, row));
            assert_eq!(plain.hole_world_pos(g), flipped.hole_world_pos(g));
            assert_eq!(plain.hole_to_local_mm(g), flipped.hole_to_local_mm(g));
        }
    }
}
