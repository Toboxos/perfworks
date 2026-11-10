pub mod actions;
pub mod app;
pub mod canvas;
pub mod export;
pub mod icons;
pub mod library;
pub mod model;
pub mod netlist;
pub mod snap;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Perfwork — Perfboard, Stripboard & Breadboard CAD Designer")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "perfwork",
        native_options,
        Box::new(|cc| Ok(Box::new(app::PerfworkApp::new(cc)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id("perfwork_canvas")
            .expect("Failed to find perfwork_canvas")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("perfwork_canvas was not a HtmlCanvasElement");

        let web_options = eframe::WebOptions::default();

        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(app::PerfworkApp::new(cc)))),
            )
            .await
            .expect("failed to start eframe");
    });
}

#[cfg(test)]
mod tests {
    use crate::model::{
        AxisLabelStyle, ConnectorKind, GridPos, Perfboard, PlacedConnector, Project, RgbColor,
        Rotation, SolderLine, Wire, WireLayer, WireMode, WorldPos,
    };
    use uuid::Uuid;

    #[test]
    fn test_axis_label_formatting() {
        assert_eq!(AxisLabelStyle::Numbers1Based.format_index(0), "1");
        assert_eq!(AxisLabelStyle::Numbers1Based.format_index(9), "10");
        assert_eq!(AxisLabelStyle::Numbers0Based.format_index(0), "0");
        assert_eq!(AxisLabelStyle::LettersUpper.format_index(0), "A");
        assert_eq!(AxisLabelStyle::LettersUpper.format_index(25), "Z");
        assert_eq!(AxisLabelStyle::LettersUpper.format_index(26), "AA");
        assert_eq!(AxisLabelStyle::LettersLower.format_index(2), "c");
        assert_eq!(AxisLabelStyle::Hidden.format_index(5), "");
    }

    #[test]
    fn test_jst_xh_blocks_side_overhang_holes() {
        let mut proj = Project::new_empty("Test");
        proj.boards = vec![Perfboard::new("Board 1", GridPos::new(0, 0), 20, 15)];

        // Place a 3-pin JST-XH connector at (5, 5) with 0 deg rotation
        // Pins occupy (5,5), (6,5), (7,5)
        // Side overhang blocks (4,5) and (8,5)
        let jst = PlacedConnector {
            id: Uuid::new_v4(),
            designator: "J1".to_string(),
            label: "JST".to_string(),
            anchor: GridPos::new(5, 5),
            rotation: Rotation::Deg0,
            kind: ConnectorKind::LibraryFootprint {
                footprint_id: "jst_xh_3".to_string(),
            },
            body_color: RgbColor::new(240, 240, 230),
            pin_labels: Vec::new(),
            body_height_mm: None,
        };
        assert!(proj.validate_connector_placement(&jst, None).is_ok());
        proj.connectors.push(jst);

        // Attempting to place a single pin at (4, 5) or (8, 5) must fail because JST-XH housing blocks those holes
        for blocked_col in [4, 5, 6, 7, 8] {
            let pin = PlacedConnector {
                id: Uuid::new_v4(),
                designator: "P1".to_string(),
                label: String::new(),
                anchor: GridPos::new(blocked_col, 5),
                rotation: Rotation::Deg0,
                kind: ConnectorKind::SinglePin { square_pad: false },
                body_color: RgbColor::new(200, 180, 60),
                pin_labels: Vec::new(),
                body_height_mm: None,
            };
            assert!(
                proj.validate_connector_placement(&pin, None).is_err(),
                "Expected hole ({}, 5) to be blocked",
                blocked_col
            );
        }

        // Hole (3, 5) and (9, 5) are free
        for free_col in [3, 9] {
            let pin = PlacedConnector {
                id: Uuid::new_v4(),
                designator: "P2".to_string(),
                label: String::new(),
                anchor: GridPos::new(free_col, 5),
                rotation: Rotation::Deg0,
                kind: ConnectorKind::SinglePin { square_pad: false },
                body_color: RgbColor::new(200, 180, 60),
                pin_labels: Vec::new(),
                body_height_mm: None,
            };
            assert!(proj.validate_connector_placement(&pin, None).is_ok());
        }
    }

    #[test]
    fn test_solder_lines_block_new_pins_but_allow_other_solder_lines() {
        let mut proj = Project::new_empty("Solder Test");
        proj.boards = vec![Perfboard::new("B1", GridPos::new(0, 0), 12, 12)];

        // Create a solder line spanning (2, 3), (3, 3), (4, 3)
        let s1 = SolderLine::new(vec![
            GridPos::new(2, 3),
            GridPos::new(3, 3),
            GridPos::new(4, 3),
        ]);
        proj.solder_lines.push(s1);

        // Hole (3, 3) is blocked for placing a new connector pin
        let pin = PlacedConnector {
            id: Uuid::new_v4(),
            designator: "P1".to_string(),
            label: String::new(),
            anchor: GridPos::new(3, 3),
            rotation: Rotation::Deg0,
            kind: ConnectorKind::SinglePin { square_pad: false },
            body_color: RgbColor::new(200, 180, 60),
            pin_labels: Vec::new(),
            body_height_mm: None,
        };
        assert!(proj.validate_connector_placement(&pin, None).is_err());

        // However, another solder line can still connect onto (3, 3)!
        let occ = proj.hole_occupancy(GridPos::new(3, 3));
        assert!(occ.can_place_solder());
    }

    #[test]
    fn test_wire_straight_and_spline_sampling() {
        let mut wire = Wire {
            id: Uuid::new_v4(),
            label: "NET1".to_string(),
            start_hole: GridPos::new(1, 1),
            end_hole: GridPos::new(8, 6),
            waypoints: vec![WorldPos::new(4.0, 1.5), WorldPos::new(5.5, 5.0)],
            mode: WireMode::Straight,
            color: RgbColor::new(255, 0, 0),
            thickness: 1.0,
            layer: WireLayer::TopComponent,
        };

        let straight_pts = wire.sampled_points();
        assert_eq!(straight_pts.len(), 4);

        wire.mode = WireMode::Spline;
        let spline_pts = wire.sampled_points();
        assert!(spline_pts.len() > 20);
        assert_eq!(spline_pts.first().copied(), Some(WorldPos::new(1.0, 1.0)));
        assert_eq!(spline_pts.last().copied(), Some(WorldPos::new(8.0, 6.0)));
    }

    #[test]
    fn test_clear_all_and_select_all() {
        let mut app = crate::app::PerfworkApp::default();
        assert!(!app.project.boards.is_empty());
        assert!(!app.project.connectors.is_empty());

        let expected_total = app.project.boards.len()
            + app.project.connectors.len()
            + app.project.wires.len()
            + app.project.solder_lines.len();

        app.select_all();
        assert_eq!(app.selected.len(), expected_total);

        app.clear_all();
        assert!(app.project.boards.is_empty());
        assert!(app.project.connectors.is_empty());
        assert!(app.project.wires.is_empty());
        assert!(app.project.solder_lines.is_empty());
        assert!(app.selected.is_empty());
    }
}
