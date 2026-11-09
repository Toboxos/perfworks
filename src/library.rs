use std::collections::HashMap;
use std::sync::LazyLock;

use crate::model::{
    AxisLabelStyle, BoardKind, BoardOutline, BoardTheme, EdgePads, MountingSpec, RgbColor,
};

/// The border shared by every board in the common 5×7 / 4×6 / 3×7 / 2×8cm FR4 protoboard
/// kit: ~5.8mm either side (which is what the edge pad strips occupy) and ~3.5mm top and
/// bottom. Measured off all four boards.
///
/// These are a best fit, not an exact match, because the catalogue sizes are themselves
/// rounded — the true vertical border ranges from 3.41mm on the 5×7 to 3.65mm on the 2×8.
/// One shared constant reproduces every catalogue size to within 0.3mm, which is well
/// inside the tolerance of the boards themselves.
const PROTOBOARD_OUTLINE: BoardOutline = BoardOutline {
    margin_x_mm: 5.8,
    margin_y_mm: 3.5,
    thickness_mm: 1.6,
    corner_radius_mm: 1.0,
};

/// M3 clearance holes, 3.5mm in from each corner.
const PROTOBOARD_MOUNTING: MountingSpec = MountingSpec {
    inset_mm: 3.5,
    drill_mm: 3.2,
    pad_mm: 6.0,
};

/// Standard board template in the library (Perfboard, Stripboard, Breadboard)
#[derive(Debug, Clone)]
pub struct BoardPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub short_name: &'static str,
    pub description: &'static str,
    pub cols: u32,
    pub rows: u32,
    pub kind: BoardKind,
    pub theme: BoardTheme,
    pub col_label_style: AxisLabelStyle,
    pub row_label_style: AxisLabelStyle,
    /// Physical substrate extents of the real-world board this preset models
    pub outline: BoardOutline,
    /// Corner mounting hole geometry
    pub mounting: MountingSpec,
    /// Oversized edge solder pads present on the real board
    pub edge_pads: EdgePads,
    /// Whether the real board has corner mounting holes at all
    pub mounting_holes: bool,
}

static BOARD_PRESETS: LazyLock<Vec<BoardPreset>> = LazyLock::new(build_board_presets);

static BOARD_PRESET_INDEX: LazyLock<HashMap<&'static str, usize>> = LazyLock::new(|| {
    BOARD_PRESETS
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id, i))
        .collect()
});

/// All standard board presets. Built once on first use.
pub fn standard_board_library() -> &'static [BoardPreset] {
    &BOARD_PRESETS
}

/// The four boards of the standard double-sided FR4 protoboard kit.
///
/// Hole counts were measured from the physical boards and cross-checked against pitch
/// arithmetic: every one satisfies `cols = (width_mm - 11.6) / 2.54` and
/// `rows = (height_mm - 7.2) / 2.54` to within a tenth of a hole, which is what makes
/// [`PROTOBOARD_OUTLINE`] a single shared constant rather than four fudged ones.
fn protoboard(
    id: &'static str,
    short_name: &'static str,
    name: &'static str,
    description: &'static str,
    cols: u32,
    rows: u32,
) -> BoardPreset {
    BoardPreset {
        id,
        name,
        short_name,
        description,
        cols,
        rows,
        kind: BoardKind::Perfboard,
        theme: BoardTheme::ClassicGreen,
        col_label_style: AxisLabelStyle::Numbers1Based,
        row_label_style: AxisLabelStyle::LettersUpper,
        outline: PROTOBOARD_OUTLINE,
        mounting: PROTOBOARD_MOUNTING,
        edge_pads: EdgePads::BOTH,
        mounting_holes: true,
    }
}

fn build_board_presets() -> Vec<BoardPreset> {
    vec![
        protoboard(
            "board_perf_24x18",
            "Protoboard 5×7cm",
            "Protoboard 5×7cm (70×50mm, 24×18)",
            "Double-sided FR4 protoboard, 1.6mm, M3 corner holes, edge solder pads",
            24,
            18,
        ),
        protoboard(
            "board_perf_20x14",
            "Protoboard 4×6cm",
            "Protoboard 4×6cm (60×40mm, 20×14)",
            "Double-sided FR4 protoboard, 1.6mm, M3 corner holes, edge solder pads",
            20,
            14,
        ),
        protoboard(
            "board_perf_24x10",
            "Protoboard 3×7cm",
            "Protoboard 3×7cm (70×30mm, 24×10)",
            "Double-sided FR4 protoboard, 1.6mm, M3 corner holes, edge solder pads",
            24,
            10,
        ),
        protoboard(
            "board_perf_28x6",
            "Protoboard 2×8cm",
            "Protoboard 2×8cm (80×20mm, 28×6)",
            "Double-sided FR4 protoboard, 1.6mm, M3 corner holes, edge solder pads",
            28,
            6,
        ),
        BoardPreset {
            id: "board_perf_18x12",
            name: "Compact Perfboard (18×12 — 3×5cm)",
            short_name: "Perfboard 18×12",
            description: "Compact 3×5cm isolated-pad FR4 prototyping board",
            cols: 18,
            rows: 12,
            kind: BoardKind::Perfboard,
            theme: BoardTheme::ClassicGreen,
            col_label_style: AxisLabelStyle::Numbers1Based,
            row_label_style: AxisLabelStyle::LettersUpper,
            outline: PROTOBOARD_OUTLINE,
            mounting: PROTOBOARD_MOUNTING,
            edge_pads: EdgePads::NONE,
            mounting_holes: true,
        },
        BoardPreset {
            id: "board_perf_black_16x16",
            name: "Square Matte Black Perfboard (16×16)",
            short_name: "Black PCB 16×16",
            description: "Matte black solder-mask square module board",
            cols: 16,
            rows: 16,
            kind: BoardKind::Perfboard,
            theme: BoardTheme::MatteBlack,
            col_label_style: AxisLabelStyle::Numbers1Based,
            row_label_style: AxisLabelStyle::LettersUpper,
            outline: PROTOBOARD_OUTLINE,
            mounting: PROTOBOARD_MOUNTING,
            edge_pads: EdgePads::NONE,
            mounting_holes: true,
        },
        BoardPreset {
            id: "board_strip_h_24x14",
            name: "Horizontal Stripboard / Veroboard (24×14)",
            short_name: "Stripboard 24×14 (H)",
            description: "Phenolic amber stripboard with horizontal copper tracks",
            cols: 24,
            rows: 14,
            kind: BoardKind::StripboardHorizontal,
            theme: BoardTheme::PhenolicAmber,
            col_label_style: AxisLabelStyle::Numbers1Based,
            row_label_style: AxisLabelStyle::LettersUpper,
            // Veroboard is cut to size from a sheet, so it has a tight border and no
            // factory mounting holes.
            outline: BoardOutline {
                margin_x_mm: 1.6,
                margin_y_mm: 1.6,
                thickness_mm: 1.6,
                corner_radius_mm: 0.5,
            },
            mounting: MountingSpec::default(),
            edge_pads: EdgePads::NONE,
            mounting_holes: false,
        },
        BoardPreset {
            id: "board_strip_v_14x20",
            name: "Vertical Stripboard (14×20)",
            short_name: "Stripboard 14×20 (V)",
            description: "Phenolic amber stripboard with vertical copper tracks",
            cols: 14,
            rows: 20,
            kind: BoardKind::StripboardVertical,
            theme: BoardTheme::PhenolicAmber,
            col_label_style: AxisLabelStyle::Numbers1Based,
            row_label_style: AxisLabelStyle::LettersUpper,
            // Veroboard is cut to size from a sheet, so it has a tight border and no
            // factory mounting holes.
            outline: BoardOutline {
                margin_x_mm: 1.6,
                margin_y_mm: 1.6,
                thickness_mm: 1.6,
                corner_radius_mm: 0.5,
            },
            mounting: MountingSpec::default(),
            edge_pads: EdgePads::NONE,
            mounting_holes: false,
        },
        BoardPreset {
            id: "board_breadboard_30x10",
            name: "Half-Size Breadboard (30×10)",
            short_name: "Breadboard 30×10",
            description: "Solderless breadboard with 5-hole terminal strips and center IC trench",
            cols: 30,
            rows: 10,
            kind: BoardKind::Breadboard,
            theme: BoardTheme::BreadboardWhite,
            col_label_style: AxisLabelStyle::Numbers1Based,
            row_label_style: AxisLabelStyle::LettersUpper,
            // A solderless breadboard is a plastic block, not a PCB: much thicker, wide
            // border, no mounting holes and nothing to export as copper.
            outline: BoardOutline {
                margin_x_mm: 4.0,
                margin_y_mm: 4.0,
                thickness_mm: 8.5,
                corner_radius_mm: 2.0,
            },
            mounting: MountingSpec::default(),
            edge_pads: EdgePads::NONE,
            mounting_holes: false,
        },
    ]
}

pub fn get_board_preset_by_id(id: &str) -> Option<&'static BoardPreset> {
    BOARD_PRESET_INDEX.get(id).map(|&i| &BOARD_PRESETS[i])
}

/// Category of standard library component
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FootprintCategory {
    JstXh,
    ScrewTerminal,
    IcSocket,
    PassiveOrDiscrete,
    Module,
}

impl FootprintCategory {
    pub const ALL: [FootprintCategory; 5] = [
        FootprintCategory::JstXh,
        FootprintCategory::ScrewTerminal,
        FootprintCategory::IcSocket,
        FootprintCategory::PassiveOrDiscrete,
        FootprintCategory::Module,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            FootprintCategory::JstXh => "JST-XH Shrouded Connectors",
            FootprintCategory::ScrewTerminal => "Screw Terminals",
            FootprintCategory::IcSocket => "DIP IC Sockets",
            FootprintCategory::PassiveOrDiscrete => "Passives & Discretes",
            FootprintCategory::Module => "Modules & Switches",
        }
    }
}

/// Visual rendering style for the 2D shape of a library footprint
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FootprintShapeStyle {
    /// Shrouded JST-XH connector with thicker latch wall, side wings blocking adjacent holes, and polarity slots
    JstXhShrouded,
    /// Interlocking PCB screw terminal block with slotted screw heads and wire entry bevel
    ScrewTerminalBlock,
    /// Dual In-Line Package (DIP) IC socket with pin-1 half-moon notch and machined/stamped contacts
    DipSocket,
    /// Axial resistor / diode body between two leads
    AxialComponent,
    /// TO-220 vertical package with metal heatsink tab
    To220Vertical,
    /// Tactile push button switch (6x6mm 4-pin)
    TactileSwitch,
}

#[derive(Debug, Clone)]
pub struct PinDef {
    pub dcol: i32,
    pub drow: i32,
    pub name: &'static str,
    pub square: bool,
}

/// Definition of a standard component footprint in the built-in library
#[derive(Debug, Clone)]
pub struct FootprintDef {
    pub id: &'static str,
    pub name: &'static str,
    pub short_name: &'static str,
    pub description: &'static str,
    pub designator_prefix: &'static str,
    pub category: FootprintCategory,
    pub shape_style: FootprintShapeStyle,
    pub default_color: RgbColor,
    /// Electrical pins relative to anchor (0, 0)
    pub pins: Vec<PinDef>,
    /// Holes relative to anchor (0, 0) that are physically covered/blocked by the connector body overhang
    pub blocked_offsets: Vec<(i32, i32)>,
    /// Groups of pin indices that are electrically connected *inside* the part itself,
    /// independent of any board copper (e.g. the two leads on each side of a tactile switch).
    /// Most through-hole parts have no internal connectivity and leave this empty.
    pub internal_nets: Vec<Vec<usize>>,
    /// Local 2D bounding box `(min_x, min_y)` to `(max_x, max_y)` in grid pitch units
    pub body_min: (f32, f32),
    pub body_max: (f32, f32),
    /// Height of the body above the top of the board, in mm. Together with `standoff_mm`
    /// this is what the 3D export extrudes, so it is the number an enclosure clearance
    /// check depends on.
    ///
    /// These are nominal values for the common variant of each part. Where a part is sold
    /// in several heights (tactile switches especially) the comment on the entry says which
    /// one was assumed — and any individual placement can override it from the inspector.
    pub body_height_mm: f32,
    /// Gap between the board surface and the underside of the body, in mm.
    pub standoff_mm: f32,
}

static FOOTPRINTS: LazyLock<Vec<FootprintDef>> = LazyLock::new(build_footprints);

static FOOTPRINT_INDEX: LazyLock<HashMap<&'static str, usize>> = LazyLock::new(|| {
    FOOTPRINTS
        .iter()
        .enumerate()
        .map(|(i, f)| (f.id, i))
        .collect()
});

/// All standard component footprints. Built once on first use.
pub fn standard_library() -> &'static [FootprintDef] {
    &FOOTPRINTS
}

fn build_footprints() -> Vec<FootprintDef> {
    let mut list = Vec::new();

    // 1. JST-XH 2.50mm/2.54mm vertical shrouded connectors (2-pin through 8-pin)
    // JST-XH housings extend ~2.45mm beyond the first and last pin along the pin axis,
    // which covers and blocks 1 hole on each side `(-1, 0)` and `(N, 0)`.
    for pin_count in [2, 3, 4, 5, 6, 8] {
        let (id, name, short_name, desc): (&'static str, &'static str, &'static str, &'static str) =
            match pin_count {
                2 => (
                    "jst_xh_2",
                    "JST-XH 2-Pin Vertical (B2B-XH-A)",
                    "JST-XH 2P",
                    "2.5mm shrouded header; side walls overhang & block 1 hole on each side",
                ),
                3 => (
                    "jst_xh_3",
                    "JST-XH 3-Pin Vertical (B3B-XH-A)",
                    "JST-XH 3P",
                    "3-pin shrouded header; side walls overhang & block 1 hole on each side",
                ),
                4 => (
                    "jst_xh_4",
                    "JST-XH 4-Pin Vertical (B4B-XH-A)",
                    "JST-XH 4P",
                    "4-pin shrouded header; side walls overhang & block 1 hole on each side",
                ),
                5 => (
                    "jst_xh_5",
                    "JST-XH 5-Pin Vertical (B5B-XH-A)",
                    "JST-XH 5P",
                    "5-pin shrouded header; side walls overhang & block 1 hole on each side",
                ),
                6 => (
                    "jst_xh_6",
                    "JST-XH 6-Pin Vertical (B6B-XH-A)",
                    "JST-XH 6P",
                    "6-pin shrouded header; side walls overhang & block 1 hole on each side",
                ),
                _ => (
                    "jst_xh_8",
                    "JST-XH 8-Pin Vertical (B8B-XH-A)",
                    "JST-XH 8P",
                    "8-pin shrouded header; side walls overhang & block 1 hole on each side",
                ),
            };

        let pins = (0..pin_count)
            .map(|i| PinDef {
                dcol: i,
                drow: 0,
                name: match i {
                    0 => "1",
                    1 => "2",
                    2 => "3",
                    3 => "4",
                    4 => "5",
                    5 => "6",
                    6 => "7",
                    _ => "8",
                },
                square: i == 0,
            })
            .collect();

        // JST-XH blocks (-1, 0) on the left and (pin_count, 0) on the right
        let blocked_offsets = vec![(-1, 0), (pin_count, 0)];

        list.push(FootprintDef {
            id,
            name,
            short_name,
            description: desc,
            designator_prefix: "J",
            category: FootprintCategory::JstXh,
            shape_style: FootprintShapeStyle::JstXhShrouded,
            default_color: RgbColor::new(238, 236, 224),
            pins,
            blocked_offsets,
            internal_nets: Vec::new(),
            body_min: (-1.15, -1.10),
            body_max: ((pin_count - 1) as f32 + 1.15, 1.15),
            // B*B-XH-A housing stands 5.75mm off the board.
            body_height_mm: 5.75,
            standoff_mm: 0.0,
        });
    }

    // 2. Screw Terminals (5.08mm / 2-pitch spacing per pole)
    list.push(FootprintDef {
        id: "screw_term_2",
        name: "Screw Terminal 2-Pole (5.08mm / 2-Hole Pitch)",
        short_name: "Screw 2P (5.08mm)",
        description:
            "Heavy-duty 5.08mm pitch 2-pin screw terminal; blocks center hole & side overhang",
        designator_prefix: "J",
        category: FootprintCategory::ScrewTerminal,
        shape_style: FootprintShapeStyle::ScrewTerminalBlock,
        default_color: RgbColor::new(38, 148, 92),
        pins: vec![
            PinDef {
                dcol: 0,
                drow: 0,
                name: "1",
                square: true,
            },
            PinDef {
                dcol: 2,
                drow: 0,
                name: "2",
                square: false,
            },
        ],
        blocked_offsets: vec![
            (-1, 0),
            (1, 0),
            (3, 0),
            (0, -1),
            (1, -1),
            (2, -1),
            (0, 1),
            (1, 1),
            (2, 1),
        ],
        internal_nets: Vec::new(),
        body_min: (-1.05, -1.25),
        body_max: (3.05, 1.25),
        // KF128-style 5.08mm block; the screw head is the tallest point.
        body_height_mm: 10.0,
        standoff_mm: 0.0,
    });

    list.push(FootprintDef {
        id: "screw_term_3",
        name: "Screw Terminal 3-Pole (5.08mm / 2-Hole Pitch)",
        short_name: "Screw 3P (5.08mm)",
        description: "Heavy-duty 5.08mm pitch 3-pin screw terminal block",
        designator_prefix: "J",
        category: FootprintCategory::ScrewTerminal,
        shape_style: FootprintShapeStyle::ScrewTerminalBlock,
        default_color: RgbColor::new(38, 148, 92),
        pins: vec![
            PinDef {
                dcol: 0,
                drow: 0,
                name: "1",
                square: true,
            },
            PinDef {
                dcol: 2,
                drow: 0,
                name: "2",
                square: false,
            },
            PinDef {
                dcol: 4,
                drow: 0,
                name: "3",
                square: false,
            },
        ],
        blocked_offsets: vec![
            (-1, 0),
            (1, 0),
            (3, 0),
            (5, 0),
            (0, -1),
            (1, -1),
            (2, -1),
            (3, -1),
            (4, -1),
            (0, 1),
            (1, 1),
            (2, 1),
            (3, 1),
            (4, 1),
        ],
        internal_nets: Vec::new(),
        body_min: (-1.05, -1.25),
        body_max: (5.05, 1.25),
        body_height_mm: 10.0,
        standoff_mm: 0.0,
    });

    // 3. DIP IC Sockets (300mil / 3-hole row spacing)
    for (id, name, short_name, half_pins) in [
        ("dip_8", "DIP-8 IC Socket (300mil)", "DIP-8", 4),
        ("dip_14", "DIP-14 IC Socket (300mil)", "DIP-14", 7),
        ("dip_16", "DIP-16 IC Socket (300mil)", "DIP-16", 8),
    ] {
        let mut pins = Vec::new();
        let static_names = [
            "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
        ];
        for i in 0..half_pins {
            pins.push(PinDef {
                dcol: 0,
                drow: i,
                name: static_names[i as usize],
                square: i == 0,
            });
        }
        for i in 0..half_pins {
            let pin_idx = (half_pins + i) as usize;
            pins.push(PinDef {
                dcol: 3,
                drow: half_pins - 1 - i,
                name: static_names[pin_idx],
                square: false,
            });
        }

        list.push(FootprintDef {
            id,
            name,
            short_name,
            description: "Standard 300mil (3-grid pitch) Dual In-Line IC socket",
            designator_prefix: "U",
            category: FootprintCategory::IcSocket,
            shape_style: FootprintShapeStyle::DipSocket,
            default_color: RgbColor::new(38, 40, 46),
            pins,
            blocked_offsets: Vec::new(),
            internal_nets: Vec::new(),
            body_min: (-0.45, -0.55),
            body_max: (3.45, (half_pins - 1) as f32 + 0.55),
            // Socket body ~3.3mm with an IC seated in it; the IC is the top surface.
            body_height_mm: 5.0,
            standoff_mm: 0.0,
        });
    }

    // 4. Passives & Discretes
    list.push(FootprintDef {
        id: "resistor_axial_4",
        name: "Axial Resistor / Diode (4-Hole Span)",
        short_name: "Resistor (4P)",
        description: "Standard 1/4W through-hole axial resistor spanning 4 pitches",
        designator_prefix: "R",
        category: FootprintCategory::PassiveOrDiscrete,
        shape_style: FootprintShapeStyle::AxialComponent,
        default_color: RgbColor::new(212, 182, 132),
        pins: vec![
            PinDef {
                dcol: 0,
                drow: 0,
                name: "1",
                square: false,
            },
            PinDef {
                dcol: 4,
                drow: 0,
                name: "2",
                square: false,
            },
        ],
        blocked_offsets: vec![(1, 0), (2, 0), (3, 0)],
        internal_nets: Vec::new(),
        body_min: (-0.4, -0.52),
        body_max: (4.4, 0.52),
        // 1/4W axial body lying flat: ~2.3mm diameter, lifted slightly off the board.
        body_height_mm: 2.3,
        standoff_mm: 0.5,
    });

    list.push(FootprintDef {
        id: "to220_3",
        name: "TO-220-3 Regulator / MOSFET (Vertical)",
        short_name: "TO-220",
        description: "3-pin vertical TO-220 package with metal heatsink tab",
        designator_prefix: "Q",
        category: FootprintCategory::PassiveOrDiscrete,
        shape_style: FootprintShapeStyle::To220Vertical,
        default_color: RgbColor::new(45, 48, 54),
        pins: vec![
            PinDef {
                dcol: 0,
                drow: 0,
                name: "1",
                square: true,
            },
            PinDef {
                dcol: 1,
                drow: 0,
                name: "2",
                square: false,
            },
            PinDef {
                dcol: 2,
                drow: 0,
                name: "3",
                square: false,
            },
        ],
        blocked_offsets: vec![(-1, 0), (3, 0)],
        internal_nets: Vec::new(),
        body_min: (-0.95, -0.85),
        body_max: (2.95, 0.65),
        // Standing upright: 15.6mm package including the heatsink tab and its M3 hole.
        body_height_mm: 15.6,
        standoff_mm: 0.0,
    });

    list.push(FootprintDef {
        id: "tact_switch_6mm",
        name: "Tactile Pushbutton Switch (6x6mm)",
        short_name: "Tact Switch",
        description: "4-pin momentary tactile switch spanning 3x2 grid pitches",
        designator_prefix: "SW",
        category: FootprintCategory::Module,
        shape_style: FootprintShapeStyle::TactileSwitch,
        default_color: RgbColor::new(52, 56, 64),
        pins: vec![
            PinDef {
                dcol: 0,
                drow: 0,
                name: "1",
                square: true,
            },
            PinDef {
                dcol: 2,
                drow: 0,
                name: "2",
                square: false,
            },
            PinDef {
                dcol: 0,
                drow: 2,
                name: "3",
                square: false,
            },
            PinDef {
                dcol: 2,
                drow: 2,
                name: "4",
                square: false,
            },
        ],
        blocked_offsets: vec![(1, 0), (0, 1), (1, 1), (2, 1), (1, 2)],
        // The two leads on each side of the body come off one metal terminal, so they are
        // permanently shorted; pressing the button joins the two terminals. On the real part
        // the shorted pairs sit at the narrow 4.5mm spacing (the 6.5mm axis separates the
        // terminals); this footprint squares both axes off to 2 pitches, so the pairing is
        // recorded by column: pins 1+3 (dcol 0) and pins 2+4 (dcol 2).
        internal_nets: vec![vec![0, 2], vec![1, 3]],
        body_min: (-0.45, -0.45),
        body_max: (2.45, 2.45),
        // The 6x6x5mm variant, measured to the top of the plunger. These are sold from
        // 4.3mm to 13mm tall, so this is the one most worth overriding per placement.
        body_height_mm: 5.0,
        standoff_mm: 0.0,
    });

    list
}

pub fn get_footprint_by_id(id: &str) -> Option<&'static FootprintDef> {
    FOOTPRINT_INDEX.get(id).map(|&i| &FOOTPRINTS[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The catalogue sizes printed on the boards themselves. If a hole count or the shared
    /// margin constant is ever wrong, the board stops being the size it claims to be — and
    /// that is exactly the number a CAD enclosure is built around.
    #[test]
    fn protoboard_presets_match_their_catalogue_millimetre_sizes() {
        let expected = [
            ("board_perf_24x18", 70.0_f32, 50.0_f32),
            ("board_perf_20x14", 60.0, 40.0),
            ("board_perf_24x10", 70.0, 30.0),
            ("board_perf_28x6", 80.0, 20.0),
        ];
        for (id, w, h) in expected {
            let p = get_board_preset_by_id(id).expect("preset must exist");
            let (gw, gh) = p.outline.size_mm(p.cols, p.rows);
            // 0.4mm: the catalogue sizes are rounded, so one shared margin constant cannot
            // hit all four exactly. See PROTOBOARD_OUTLINE.
            assert!(
                (gw - w).abs() < 0.4,
                "{id}: width {gw:.2}mm, catalogue says {w}mm"
            );
            assert!(
                (gh - h).abs() < 0.4,
                "{id}: height {gh:.2}mm, catalogue says {h}mm"
            );
        }
    }

    #[test]
    fn every_board_preset_has_a_usable_physical_outline() {
        for p in standard_board_library() {
            assert!(
                p.outline.thickness_mm > 0.0,
                "{}: zero thickness would export as a degenerate solid",
                p.id
            );
            let (w, h) = p.outline.size_mm(p.cols, p.rows);
            let (field_w, field_h) = (
                (p.cols - 1) as f32 * crate::model::PITCH_MM,
                (p.rows - 1) as f32 * crate::model::PITCH_MM,
            );
            assert!(
                w >= field_w && h >= field_h,
                "{}: substrate {w:.1}×{h:.1} is smaller than its own hole field",
                p.id
            );
        }
    }

    /// Mounting holes have to clear the outermost holes, or a standoff fouls the copper.
    #[test]
    fn protoboard_mounting_holes_sit_clear_of_the_hole_field() {
        let p = get_board_preset_by_id("board_perf_24x18").unwrap();
        let mut board =
            crate::model::Perfboard::new("t", crate::model::GridPos::new(0, 0), p.cols, p.rows);
        board.outline = p.outline;
        board.mounting = p.mounting;
        board.edge_pads = p.edge_pads;
        let centers = board.mounting_centers_mm();
        assert_eq!(centers.len(), 4);

        // Compare against every hole the board actually provides, edge pads included —
        // those are the holes nearest a corner, so checking only the matrix would miss the
        // real collision.
        let holes: Vec<(f32, f32)> = board
            .edge_pad_holes()
            .into_iter()
            .chain((0..board.rows as i32).flat_map(|r| {
                (0..board.cols as i32)
                    .map(move |c| crate::model::GridPos::new(c, r))
                    .collect::<Vec<_>>()
            }))
            .map(|g| board.hole_to_local_mm(g))
            .collect();

        for (cx, cy) in centers {
            let nearest = holes
                .iter()
                .map(|(hx, hy)| (cx - hx).hypot(cy - hy))
                .fold(f32::INFINITY, f32::min);
            assert!(
                nearest > p.mounting.drill_mm * 0.5,
                "mounting hole at ({cx:.1},{cy:.1}) is {nearest:.2}mm from a pad centre, \
                 closer than its own {:.1}mm radius",
                p.mounting.drill_mm * 0.5
            );
        }
    }

    #[test]
    fn edge_pads_are_solderable_holes_outside_the_matrix() {
        use crate::model::{EdgeSide, GridPos, Perfboard};
        let mut board = Perfboard::new("t", GridPos::new(10, 4), 8, 5);
        assert!(board.edge_pad_holes().is_empty(), "off by default");

        board.edge_pads = EdgePads::BOTH;
        let pads = board.edge_pad_holes();
        assert_eq!(pads.len(), 10, "one pad per row, both sides");

        let left = GridPos::new(8, 4);
        let right = GridPos::new(19, 8);
        for p in [left, right] {
            assert!(
                !board.contains_global_hole(p),
                "edge pads must stay out of the matrix so row/column labels keep meaning"
            );
            assert!(board.provides_hole(p), "but must still be solderable");
        }
        assert_eq!(board.edge_pad_at(left), Some((EdgeSide::Left, 0)));
        assert_eq!(board.edge_pad_at(right), Some((EdgeSide::Right, 4)));
        assert_eq!(board.format_global_hole(left).as_deref(), Some("LA"));

        // One row past the end is off the board entirely.
        assert!(board.edge_pad_at(GridPos::new(8, 9)).is_none());
    }
}
