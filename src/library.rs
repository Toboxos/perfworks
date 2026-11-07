use crate::model::RgbColor;

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
            FootprintCategory::Module => "Modules & Headers",
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
    /// Local 2D bounding box `(min_x, min_y)` to `(max_x, max_y)` in grid pitch units
    pub body_min: (f32, f32),
    pub body_max: (f32, f32),
}

pub fn standard_library() -> Vec<FootprintDef> {
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
            body_min: (-1.15, -1.10),
            body_max: ((pin_count - 1) as f32 + 1.15, 1.15),
        });
    }

    // 2. Screw Terminals (5.08mm / 2-pitch spacing per pole, plus 2.54mm compact)
    list.push(FootprintDef {
        id: "screw_term_2",
        name: "Screw Terminal 2-Pole (5.08mm / 2-Hole Pitch)",
        short_name: "Screw 2P (5.08mm)",
        description: "Heavy-duty 5.08mm pitch 2-pin screw terminal; blocks center hole & side overhang",
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
        body_min: (-1.05, -1.25),
        body_max: (3.05, 1.25),
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
        body_min: (-1.05, -1.25),
        body_max: (5.05, 1.25),
    });

    // 3. DIP IC Sockets (300mil / 3-hole row spacing)
    for (id, name, short_name, half_pins) in [
        ("dip_8", "DIP-8 IC Socket (300mil)", "DIP-8", 4),
        ("dip_14", "DIP-14 IC Socket (300mil)", "DIP-14", 7),
        ("dip_16", "DIP-16 IC Socket (300mil)", "DIP-16", 8),
    ] {
        let mut pins = Vec::new();
        // Left column: pins 1..=half_pins going down (dcol=0, drow=0..half_pins)
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
        // Right column: pins (half_pins+1)..=2*half_pins going up (dcol=3, drow=(half_pins-1)..=0)
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
            body_min: (-0.45, -0.55),
            body_max: (3.45, (half_pins - 1) as f32 + 0.55),
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
        body_min: (-0.4, -0.52),
        body_max: (4.4, 0.52),
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
        body_min: (-0.95, -0.85),
        body_max: (2.95, 0.65),
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
        body_min: (-0.45, -0.45),
        body_max: (2.45, 2.45),
    });

    list
}

pub fn get_footprint_by_id(id: &str) -> Option<FootprintDef> {
    standard_library().into_iter().find(|f| f.id == id)
}
