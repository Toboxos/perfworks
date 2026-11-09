//! STEP AP214 export.
//!
//! Writes an ISO 10303-21 part 21 file containing one solid per board and one per placed
//! component, so a finished perfboard can be dropped into a CAD assembly and have an
//! enclosure built around it.
//!
//! # What this does and does not model
//!
//! Boards are rectangular prisms with cylindrical through holes at the **mounting
//! positions only**. The 2.54mm hole matrix is deliberately not drilled: it would turn a
//! 6-face solid into one with hundreds of cylindrical faces, costs real time in every
//! downstream CAD kernel, and nothing fits through those holes from outside. Components
//! are their bounding prisms. That is enough for clearance, standoff alignment and
//! fastener placement, which is what the export is for.
//!
//! STEP carries per-face colour but has no texture concept, so board artwork cannot be
//! baked in here — see [`crate::export::image`] for the decal that pairs with this.
//!
//! # Structure
//!
//! Geometry is described through [`Prism`], a box with optional vertical through holes.
//! [`StepWriter`] owns entity numbering and emits the B-rep for one. Keeping those apart
//! means adding a new exported shape never touches entity bookkeeping, and vice versa.

use std::fmt::Write as _;

use crate::model::{Project, Rotation};

/// A unit direction as `(x, y, z)`.
type Dir = (f32, f32, f32);

/// One side wall of a prism: the four box edges that bound it (by index, with traversal
/// direction), the point its plane passes through, and its outward normal.
type SideWall = ([(usize, bool); 4], Vec3, Dir);

/// Millimetre position, Z up, origin at the top-left corner of the first board's top face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

/// A circular through hole in a [`Prism`], positioned in the prism's own local frame.
#[derive(Debug, Clone, Copy)]
pub struct Bore {
    /// Centre, relative to the prism's minimum corner
    pub x: f32,
    pub y: f32,
    pub diameter: f32,
}

/// An axis-aligned box, optionally drilled top-to-bottom. The only solid this exporter
/// knows how to build — every board and every component reduces to one.
#[derive(Debug, Clone)]
pub struct Prism {
    /// Name carried into the STEP product structure, e.g. a designator
    pub name: String,
    /// Minimum corner in global millimetres
    pub min: Vec3,
    /// Size along each axis, all strictly positive
    pub size: Vec3,
    pub bores: Vec<Bore>,
    /// Display colour, 0..1 per channel
    pub color: (f32, f32, f32),
}

impl Prism {
    pub fn max(&self) -> Vec3 {
        Vec3::new(
            self.min.x + self.size.x,
            self.min.y + self.size.y,
            self.min.z + self.size.z,
        )
    }

    /// Whether this prism is dimensionally valid. A zero or negative extent would produce
    /// a degenerate face that most CAD kernels reject on import, so such a solid is
    /// dropped rather than written.
    pub fn is_valid(&self) -> bool {
        self.size.x > 1e-4 && self.size.y > 1e-4 && self.size.z > 1e-4
    }
}

/// Accumulates part 21 entity lines and hands out their instance names.
///
/// Every `add` returns the `#n` of the line just written, which is the only way entity
/// ids are ever produced — so the file cannot contain a dangling forward reference.
struct StepWriter {
    lines: Vec<String>,
}

impl StepWriter {
    fn new() -> Self {
        Self { lines: Vec::new() }
    }

    fn add(&mut self, body: impl AsRef<str>) -> usize {
        let id = self.lines.len() + 1;
        self.lines.push(format!("#{} = {};", id, body.as_ref()));
        id
    }

    fn point(&mut self, p: Vec3) -> usize {
        self.add(format!(
            "CARTESIAN_POINT('',({},{},{}))",
            num(p.x),
            num(p.y),
            num(p.z)
        ))
    }

    fn direction(&mut self, x: f32, y: f32, z: f32) -> usize {
        self.add(format!("DIRECTION('',({},{},{}))", num(x), num(y), num(z)))
    }

    /// An `AXIS2_PLACEMENT_3D`: an origin plus a Z axis and an X reference direction.
    fn axis(&mut self, origin: Vec3, axis: Dir, refdir: Dir) -> usize {
        let o = self.point(origin);
        let a = self.direction(axis.0, axis.1, axis.2);
        let r = self.direction(refdir.0, refdir.1, refdir.2);
        self.add(format!("AXIS2_PLACEMENT_3D('',#{},#{},#{})", o, a, r))
    }

    fn vertex(&mut self, p: Vec3) -> usize {
        let pt = self.point(p);
        self.add(format!("VERTEX_POINT('',#{})", pt))
    }

    /// A straight edge between two existing vertices, as an unbounded line plus the
    /// topological edge that trims it.
    fn line_edge(&mut self, v0: usize, p0: Vec3, v1: usize, p1: Vec3) -> usize {
        let dx = p1.x - p0.x;
        let dy = p1.y - p0.y;
        let dz = p1.z - p0.z;
        let len = (dx * dx + dy * dy + dz * dz).sqrt().max(1e-9);
        let o = self.point(p0);
        let d = self.direction(dx / len, dy / len, dz / len);
        let vec = self.add(format!("VECTOR('',#{},{})", d, num(len)));
        let line = self.add(format!("LINE('',#{},#{})", o, vec));
        self.add(format!("EDGE_CURVE('',#{},#{},#{},.T.)", v0, v1, line))
    }

    /// A planar face bounded by `outer`, with each entry of `inners` cut out of it.
    fn planar_face(
        &mut self,
        origin: Vec3,
        normal: Dir,
        refdir: Dir,
        outer: usize,
        inners: &[usize],
        same_sense: bool,
    ) -> usize {
        let ax = self.axis(origin, normal, refdir);
        let plane = self.add(format!("PLANE('',#{})", ax));
        let ob = self.add(format!("FACE_OUTER_BOUND('',#{},.T.)", outer));
        let mut bounds = vec![format!("#{}", ob)];
        for inner in inners {
            let b = self.add(format!("FACE_BOUND('',#{},.T.)", inner));
            bounds.push(format!("#{}", b));
        }
        self.add(format!(
            "ADVANCED_FACE('',({}),#{},{})",
            bounds.join(","),
            plane,
            if same_sense { ".T." } else { ".F." }
        ))
    }

    /// The two half-circle edges forming one bore rim. A circle cannot be a single edge
    /// because its start and end vertex would coincide.
    ///
    /// Returned rather than wrapped in a loop because **both** the drilled planar face and
    /// the cylinder wall must reference these same two edges. Building a fresh circle for
    /// each would give the two faces coincident-but-distinct topology, and the shell would
    /// not sew — which is exactly how this was wrong the first time: the file imported, the
    /// cylinders arrived as four loose single-face shells, and the board silently never
    /// became a solid.
    fn circle_rim(&mut self, cx: f32, cy: f32, z: f32, r: f32) -> [usize; 2] {
        let ax = self.axis(Vec3::new(cx, cy, z), (0.0, 0.0, 1.0), (1.0, 0.0, 0.0));
        let circle = self.add(format!("CIRCLE('',#{},{})", ax, num(r)));
        let v0 = self.vertex(Vec3::new(cx + r, cy, z));
        let v1 = self.vertex(Vec3::new(cx - r, cy, z));
        let e0 = self.add(format!("EDGE_CURVE('',#{},#{},#{},.T.)", v0, v1, circle));
        let e1 = self.add(format!("EDGE_CURVE('',#{},#{},#{},.T.)", v1, v0, circle));
        [e0, e1]
    }

    /// An edge loop over a rim, traversed in the given direction. Two faces sharing a rim
    /// must pass opposite `forward` values, which is what makes the shell closed.
    fn rim_loop(&mut self, rim: [usize; 2], forward: bool) -> usize {
        let o0 = self.add(format!(
            "ORIENTED_EDGE('',*,*,#{},{})",
            rim[0],
            flag(forward)
        ));
        let o1 = self.add(format!(
            "ORIENTED_EDGE('',*,*,#{},{})",
            rim[1],
            flag(forward)
        ));
        let (a, b) = if forward { (o0, o1) } else { (o1, o0) };
        self.add(format!("EDGE_LOOP('',(#{},#{}))", a, b))
    }

    /// The cylindrical wall of one bore, bounded by rims already shared with the drilled
    /// top and bottom faces.
    fn bore_wall(
        &mut self,
        cx: f32,
        cy: f32,
        r: f32,
        z0: f32,
        bottom_rim: [usize; 2],
        top_rim: [usize; 2],
    ) -> usize {
        let ax = self.axis(Vec3::new(cx, cy, z0), (0.0, 0.0, 1.0), (1.0, 0.0, 0.0));
        let surf = self.add(format!("CYLINDRICAL_SURFACE('',#{},{})", ax, num(r)));

        // Opposite senses from the planar faces that share these rims.
        let b = self.rim_loop(bottom_rim, false);
        let t = self.rim_loop(top_rim, true);

        // Material lies outside the cylinder, so the face normal opposes the cylinder's
        // own outward normal.
        let fb0 = self.add(format!("FACE_BOUND('',#{},.T.)", b));
        let fb1 = self.add(format!("FACE_BOUND('',#{},.T.)", t));
        self.add(format!(
            "ADVANCED_FACE('',(#{},#{}),#{},.F.)",
            fb0, fb1, surf
        ))
    }

    /// Build the closed shell of one prism and return its `CLOSED_SHELL` id.
    fn prism_shell(&mut self, prism: &Prism) -> usize {
        let lo = prism.min;
        let hi = prism.max();

        // Eight corner vertices, bottom face first.
        let corners = [
            Vec3::new(lo.x, lo.y, lo.z),
            Vec3::new(hi.x, lo.y, lo.z),
            Vec3::new(hi.x, hi.y, lo.z),
            Vec3::new(lo.x, hi.y, lo.z),
            Vec3::new(lo.x, lo.y, hi.z),
            Vec3::new(hi.x, lo.y, hi.z),
            Vec3::new(hi.x, hi.y, hi.z),
            Vec3::new(lo.x, hi.y, hi.z),
        ];
        let v: Vec<usize> = corners.iter().map(|p| self.vertex(*p)).collect();

        // Twelve edges, indexed so each face can name the four it needs.
        let edge_defs = [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0), // bottom
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4), // top
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7), // verticals
        ];
        let edges: Vec<usize> = edge_defs
            .iter()
            .map(|(a, b)| self.line_edge(v[*a], corners[*a], v[*b], corners[*b]))
            .collect();

        let mut faces = Vec::new();

        // Each bore's two rims are built once and then referenced three times: by the
        // bottom face, the top face and the cylinder between them.
        let rims: Vec<([usize; 2], [usize; 2])> = prism
            .bores
            .iter()
            .map(|b| {
                let (cx, cy) = (lo.x + b.x, lo.y + b.y);
                let r = b.diameter * 0.5;
                (
                    self.circle_rim(cx, cy, lo.z, r),
                    self.circle_rim(cx, cy, hi.z, r),
                )
            })
            .collect();

        let bore_rings_bottom: Vec<usize> = rims
            .iter()
            .map(|(b, _)| *b)
            .collect::<Vec<_>>()
            .into_iter()
            .map(|rim| self.rim_loop(rim, true))
            .collect();
        let bore_rings_top: Vec<usize> = rims
            .iter()
            .map(|(_, t)| *t)
            .collect::<Vec<_>>()
            .into_iter()
            .map(|rim| self.rim_loop(rim, false))
            .collect();

        let bottom_loop = self.quad_loop(&edges, [(0, true), (1, true), (2, true), (3, true)]);
        let f = self.planar_face(
            Vec3::new(lo.x, lo.y, lo.z),
            (0.0, 0.0, -1.0),
            (1.0, 0.0, 0.0),
            bottom_loop,
            &bore_rings_bottom,
            true,
        );
        faces.push(f);

        let top_loop = self.quad_loop(&edges, [(4, true), (5, true), (6, true), (7, true)]);
        let f = self.planar_face(
            Vec3::new(lo.x, lo.y, hi.z),
            (0.0, 0.0, 1.0),
            (1.0, 0.0, 0.0),
            top_loop,
            &bore_rings_top,
            true,
        );
        faces.push(f);

        // Four side walls.
        let sides: [SideWall; 4] = [
            (
                [(0, true), (9, true), (4, false), (8, false)],
                Vec3::new(lo.x, lo.y, lo.z),
                (0.0, -1.0, 0.0),
            ),
            (
                [(1, true), (10, true), (5, false), (9, false)],
                Vec3::new(hi.x, lo.y, lo.z),
                (1.0, 0.0, 0.0),
            ),
            (
                [(2, true), (11, true), (6, false), (10, false)],
                Vec3::new(hi.x, hi.y, lo.z),
                (0.0, 1.0, 0.0),
            ),
            (
                [(3, true), (8, true), (7, false), (11, false)],
                Vec3::new(lo.x, hi.y, lo.z),
                (-1.0, 0.0, 0.0),
            ),
        ];
        for (loop_def, origin, normal) in sides {
            let l = self.quad_loop(&edges, loop_def);
            let refdir = if normal.0.abs() > 0.5 {
                (0.0, 1.0, 0.0)
            } else {
                (1.0, 0.0, 0.0)
            };
            let f = self.planar_face(origin, normal, refdir, l, &[], true);
            faces.push(f);
        }

        // Bore walls close the shell back up where the top and bottom were pierced.
        for (bore, (bottom_rim, top_rim)) in prism.bores.iter().zip(rims) {
            let f = self.bore_wall(
                lo.x + bore.x,
                lo.y + bore.y,
                bore.diameter * 0.5,
                lo.z,
                bottom_rim,
                top_rim,
            );
            faces.push(f);
        }

        let refs: Vec<String> = faces.iter().map(|f| format!("#{}", f)).collect();
        self.add(format!("CLOSED_SHELL('',({}))", refs.join(",")))
    }

    fn quad_loop(&mut self, edges: &[usize], def: [(usize, bool); 4]) -> usize {
        let oriented: Vec<usize> = def
            .iter()
            .map(|(idx, fwd)| {
                self.add(format!(
                    "ORIENTED_EDGE('',*,*,#{},{})",
                    edges[*idx],
                    flag(*fwd)
                ))
            })
            .collect();
        let refs: Vec<String> = oriented.iter().map(|o| format!("#{}", o)).collect();
        self.add(format!("EDGE_LOOP('',({}))", refs.join(",")))
    }
}

fn flag(b: bool) -> &'static str {
    if b {
        ".T."
    } else {
        ".F."
    }
}

/// Format a length for part 21. Trailing `.` is mandatory for STEP reals, and values are
/// rounded to a micron so floating point noise does not leak into the file.
fn num(v: f32) -> String {
    let v = (v as f64 * 1000.0).round() / 1000.0;
    let v = if v == 0.0 { 0.0 } else { v }; // normalise -0
    let mut s = format!("{:.3}", v);
    while s.ends_with('0') && !s.ends_with(".0") {
        s.pop();
    }
    s
}

/// Escape a string for a part 21 single-quoted literal.
fn quote(s: &str) -> String {
    s.replace('\'', "''")
}

/// Collect every solid in a project, in global millimetres with Z up.
///
/// The board plane is the XY plane, and the first board's top-left substrate corner is the
/// origin. Each board is placed by its `origin` grid position times the pitch — that
/// multiplication is the board-local to global bridge, and is the only place the two
/// frames meet.
pub fn collect_prisms(project: &Project) -> Vec<Prism> {
    let mut out = Vec::new();

    for board in &project.boards {
        let (w, h) = board.size_mm();
        let (bx, by) = board_origin_mm(board);
        let mut prism = Prism {
            name: board.name.clone(),
            // Board top face sits at z = 0, so components stack upward from zero and the
            // substrate hangs below. That puts the mating surface at a round number.
            min: Vec3::new(bx, by, -board.outline.thickness_mm),
            size: Vec3::new(w, h, board.outline.thickness_mm),
            bores: Vec::new(),
            color: theme_color(board.theme),
        };
        for (mx, my) in board.mounting_centers_mm() {
            prism.bores.push(Bore {
                x: mx,
                y: my,
                diameter: board.mounting.drill_mm,
            });
        }
        out.push(prism);
    }

    for conn in &project.connectors {
        let (standoff, height) = conn.body_profile_mm();
        let (min, max) = conn.world_aabb();
        // Components are positioned in the same global millimetre frame as the boards, so
        // a part keeps its place even when it straddles two boards.
        let x0 = min.x * crate::model::PITCH_MM;
        let y0 = min.y * crate::model::PITCH_MM;
        let x1 = max.x * crate::model::PITCH_MM;
        let y1 = max.y * crate::model::PITCH_MM;
        out.push(Prism {
            name: if conn.label.is_empty() {
                conn.designator.clone()
            } else {
                format!("{} {}", conn.designator, conn.label)
            },
            min: Vec3::new(x0, y0, standoff),
            size: Vec3::new(x1 - x0, y1 - y0, height),
            bores: Vec::new(),
            color: (
                conn.body_color.r as f32 / 255.0,
                conn.body_color.g as f32 / 255.0,
                conn.body_color.b as f32 / 255.0,
            ),
        });
    }

    out.retain(|p| p.is_valid());
    out
}

/// Global millimetre position of a board's top-left substrate corner.
fn board_origin_mm(board: &crate::model::Perfboard) -> (f32, f32) {
    let (mx, my) = (board.outline.margin_x_mm, board.outline.margin_y_mm);
    (
        board.origin.col as f32 * crate::model::PITCH_MM - mx,
        board.origin.row as f32 * crate::model::PITCH_MM - my,
    )
}

fn theme_color(theme: crate::model::BoardTheme) -> (f32, f32, f32) {
    let c = theme.substrate_fill();
    (
        c.r() as f32 / 255.0,
        c.g() as f32 / 255.0,
        c.b() as f32 / 255.0,
    )
}

/// Render `project` as an ISO 10303-21 AP214 file.
pub fn export_step(project: &Project, file_name: &str) -> String {
    let prisms = collect_prisms(project);
    let mut w = StepWriter::new();

    // Units: millimetres and radians, with the uncertainty STEP requires for a B-rep.
    let len_unit = w.add("( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) )");
    let ang_unit = w.add("( NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.) )");
    let sa_unit = w.add("( NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT() )");
    let tol = w.add(format!(
        "UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-5),#{},'distance_accuracy_value','')",
        len_unit
    ));
    let ctx = w.add(format!(
        "( GEOMETRIC_REPRESENTATION_CONTEXT(3) \
         GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{})) \
         GLOBAL_UNIT_ASSIGNED_CONTEXT((#{},#{},#{})) REPRESENTATION_CONTEXT('',''))",
        tol, len_unit, ang_unit, sa_unit
    ));

    let app_ctx = w.add("APPLICATION_CONTEXT('automotive design')");
    w.add(format!(
        "APPLICATION_PROTOCOL_DEFINITION('international standard',\
         'automotive_design',2000,#{})",
        app_ctx
    ));

    let mut shape_reps: Vec<(usize, String, (f32, f32, f32))> = Vec::new();
    for prism in &prisms {
        let shell = w.prism_shell(prism);
        let solid = w.add(format!(
            "MANIFOLD_SOLID_BREP('{}',#{})",
            quote(&prism.name),
            shell
        ));
        let placement = w.axis(Vec3::new(0.0, 0.0, 0.0), (0.0, 0.0, 1.0), (1.0, 0.0, 0.0));
        let rep = w.add(format!(
            "ADVANCED_BREP_SHAPE_REPRESENTATION('{}',(#{},#{}),#{})",
            quote(&prism.name),
            placement,
            solid,
            ctx
        ));
        shape_reps.push((rep, prism.name.clone(), prism.color));

        // Per-face colour. STEP's only appearance mechanism — there is no texture map, so
        // board artwork has to travel as a separate image.
        let (r, g, b) = prism.color;
        let colour = w.add(format!("COLOUR_RGB('',{},{},{})", num(r), num(g), num(b)));
        let fill = w.add(format!("FILL_AREA_STYLE_COLOUR('',#{})", colour));
        let fas = w.add(format!("FILL_AREA_STYLE('',(#{}))", fill));
        let surf_fill = w.add(format!("SURFACE_STYLE_FILL_AREA(#{})", fas));
        let surf_side = w.add(format!("SURFACE_SIDE_STYLE('',(#{}))", surf_fill));
        let surf_usage = w.add(format!("SURFACE_STYLE_USAGE(.BOTH.,#{})", surf_side));
        let psa = w.add(format!("PRESENTATION_STYLE_ASSIGNMENT((#{}))", surf_usage));
        w.add(format!("STYLED_ITEM('',(#{}),#{})", psa, solid));
    }

    // Product structure: one product per solid, all sharing a single pair of contexts.
    // Emitting the contexts up front is what lets every reference point backwards; there
    // are no forward references anywhere in the file.
    let prod_ctx = w.add(format!("PRODUCT_CONTEXT('',#{},'mechanical')", app_ctx));
    let pd_ctx = w.add(format!(
        "PRODUCT_DEFINITION_CONTEXT('part definition',#{},'design')",
        app_ctx
    ));
    for (rep, name, _) in &shape_reps {
        let product = w.add(format!(
            "PRODUCT('{0}','{0}','',(#{1}))",
            quote(name),
            prod_ctx
        ));
        let pdf = w.add(format!("PRODUCT_DEFINITION_FORMATION('','',#{})", product));
        let pd = w.add(format!(
            "PRODUCT_DEFINITION('design','',#{},#{})",
            pdf, pd_ctx
        ));
        let pds = w.add(format!("PRODUCT_DEFINITION_SHAPE('','',#{})", pd));
        w.add(format!(
            "SHAPE_DEFINITION_REPRESENTATION(#{},#{})",
            pds, rep
        ));
    }

    let mut out = String::new();
    let _ = write!(
        out,
        "ISO-10303-21;\n\
         HEADER;\n\
         FILE_DESCRIPTION(('Perfboard assembly'),'2;1');\n\
         FILE_NAME('{}','',(''),(''),'Perfwork','Perfwork','');\n\
         FILE_SCHEMA(('AUTOMOTIVE_DESIGN {{ 1 0 10303 214 3 1 1 }}'));\n\
         ENDSEC;\n\
         DATA;\n",
        quote(file_name)
    );
    for line in &w.lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    out
}

/// Rotation is already baked into `world_aabb`, so the exporter never needs it directly.
/// Kept as a compile-time reminder that component prisms are axis-aligned bounding boxes:
/// a 90-degree rotation changes the box, a 45-degree one could not be represented.
#[allow(dead_code)]
fn rotation_is_axis_aligned(r: Rotation) -> bool {
    matches!(
        r,
        Rotation::Deg0 | Rotation::Deg90 | Rotation::Deg180 | Rotation::Deg270
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GridPos, Perfboard, Project};

    fn starter() -> Project {
        Project::new_starter()
    }

    #[test]
    fn num_formats_valid_part21_reals() {
        assert_eq!(num(0.0), "0.0");
        assert_eq!(num(1.6), "1.6");
        assert_eq!(num(70.0), "70.0");
        assert_eq!(num(-3.25), "-3.25");
        // No exponent form, and always a decimal point: both are part 21 requirements.
        for v in [0.0001_f32, 123456.0, -0.5] {
            let s = num(v);
            assert!(s.contains('.'), "{s} has no decimal point");
            assert!(
                !s.contains('e') && !s.contains('E'),
                "{s} uses exponent form"
            );
        }
    }

    #[test]
    fn board_prism_matches_the_boards_catalogue_size_and_thickness() {
        let mut project = Project::new_empty("t");
        let preset = crate::library::get_board_preset_by_id("board_perf_24x18").unwrap();
        let mut board = Perfboard::new("B1", GridPos::new(0, 0), preset.cols, preset.rows);
        board.outline = preset.outline;
        board.mounting = preset.mounting;
        project.boards.push(board);

        let prisms = collect_prisms(&project);
        assert_eq!(prisms.len(), 1);
        let p = &prisms[0];
        assert!((p.size.x - 70.0).abs() < 0.4, "width {}", p.size.x);
        assert!((p.size.y - 50.0).abs() < 0.4, "height {}", p.size.y);
        assert!((p.size.z - 1.6).abs() < 1e-5, "thickness {}", p.size.z);
        // Top face at z=0 is the contract components are stacked against.
        assert!((p.max().z - 0.0).abs() < 1e-5, "top face at {}", p.max().z);
        assert_eq!(p.bores.len(), 4, "corner mounting holes");
        for b in &p.bores {
            assert!(
                (b.diameter - preset.mounting.drill_mm).abs() < 1e-5,
                "bore {} should match the preset drill {}",
                b.diameter,
                preset.mounting.drill_mm
            );
        }
    }

    #[test]
    fn components_sit_above_the_board_and_carry_their_library_height() {
        let project = starter();
        let prisms = collect_prisms(&project);
        let boards = project.boards.len();
        assert!(prisms.len() > boards, "components must be exported too");

        for p in &prisms[boards..] {
            assert!(
                p.min.z >= 0.0,
                "{} starts at z={}, below the board top face",
                p.name,
                p.min.z
            );
            assert!(p.size.z > 0.0, "{} has no height", p.name);
        }
    }

    #[test]
    fn degenerate_solids_are_dropped_rather_than_written() {
        let mut project = Project::new_empty("t");
        let mut board = Perfboard::new("flat", GridPos::new(0, 0), 10, 10);
        board.outline.thickness_mm = 0.0;
        project.boards.push(board);
        assert!(
            collect_prisms(&project).is_empty(),
            "a zero-thickness solid would be rejected by the importing CAD kernel"
        );
    }

    #[test]
    fn step_file_is_structurally_well_formed() {
        let project = starter();
        let text = export_step(&project, "test.step");

        assert!(text.starts_with("ISO-10303-21;\n"));
        assert!(text.trim_end().ends_with("END-ISO-10303-21;"));
        assert!(text.contains("FILE_SCHEMA(('AUTOMOTIVE_DESIGN"));
        assert!(text.contains("MANIFOLD_SOLID_BREP"));
        assert!(
            text.contains("CYLINDRICAL_SURFACE"),
            "mounting holes must be drilled"
        );
        assert!(text.contains("ADVANCED_BREP_SHAPE_REPRESENTATION"));

        // Every entity is declared exactly once and numbered consecutively, which is what
        // keeps references resolvable.
        let data = text
            .split("DATA;\n")
            .nth(1)
            .unwrap()
            .split("ENDSEC;")
            .next()
            .unwrap();
        for (i, line) in data.lines().filter(|l| !l.is_empty()).enumerate() {
            assert!(
                line.starts_with(&format!("#{} = ", i + 1)),
                "line {} is out of sequence: {line}",
                i + 1
            );
            assert!(line.ends_with(';'), "unterminated: {line}");
        }
    }

    /// Writes the starter project to disk when PERFWORK_DUMP_STEP is set, so the file can
    /// be round-tripped through a real CAD kernel outside the test suite.
    #[test]
    fn dump_step_for_external_validation() {
        let Ok(path) = std::env::var("PERFWORK_DUMP_STEP") else {
            return;
        };
        std::fs::write(&path, export_step(&starter(), "perfwork.step")).unwrap();
    }

    /// Every edge must be used by exactly two faces. This is the manifold condition, and
    /// violating it is how the bores were wrong at first: the cylinder built its own copy
    /// of each rim instead of sharing the drilled face's, so those edges were used once
    /// each, the shell never sewed, and the board imported as loose faces with no solid.
    /// Nothing about the file's syntax catches that — only this count does.
    #[test]
    fn every_edge_is_shared_by_exactly_two_faces() {
        use std::collections::HashMap;
        let project = starter();
        let text = export_step(&project, "t.step");

        let mut uses: HashMap<usize, usize> = HashMap::new();
        let mut edges: Vec<usize> = Vec::new();
        for line in text.lines() {
            let Some((lhs, body)) = line.split_once(" = ") else {
                continue;
            };
            let id: usize = lhs.trim_start_matches('#').parse().unwrap();
            if body.starts_with("EDGE_CURVE") {
                edges.push(id);
            } else if body.starts_with("ORIENTED_EDGE") {
                // ORIENTED_EDGE('',*,*,#n,.T.) — the only #ref is the edge it orients.
                let r = body.split('#').nth(1).unwrap();
                let n: usize = r
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .unwrap();
                *uses.entry(n).or_default() += 1;
            }
        }

        assert!(!edges.is_empty());
        for e in edges {
            assert_eq!(
                uses.get(&e).copied().unwrap_or(0),
                2,
                "edge #{e} is used {} time(s); a closed shell needs exactly 2",
                uses.get(&e).copied().unwrap_or(0)
            );
        }
    }

    /// A drilled board must have 6 planar faces plus one cylinder per mounting hole.
    #[test]
    fn a_drilled_board_emits_one_cylinder_per_mounting_hole() {
        let mut project = Project::new_empty("t");
        project
            .boards
            .push(Perfboard::new("B", GridPos::new(0, 0), 24, 18));
        let holes = project.boards[0].mounting_centers_mm().len();
        assert_eq!(holes, 4);

        let text = export_step(&project, "t.step");
        assert_eq!(
            text.matches("CYLINDRICAL_SURFACE").count(),
            holes,
            "one cylinder per bore"
        );
        assert_eq!(text.matches("ADVANCED_FACE").count(), 6 + holes);
    }

    #[test]
    fn every_entity_reference_resolves() {
        let project = starter();
        let text = export_step(&project, "t.step");
        let data = text
            .split("DATA;\n")
            .nth(1)
            .unwrap()
            .split("ENDSEC;")
            .next()
            .unwrap();
        let count = data.lines().filter(|l| !l.is_empty()).count();

        // A dangling #n is the one defect that makes a STEP file silently import as empty,
        // so it is worth checking exhaustively rather than spot-checking.
        for line in data.lines().filter(|l| !l.is_empty()) {
            let body = line.split_once(" = ").unwrap().1;
            let mut rest = body;
            while let Some(pos) = rest.find('#') {
                rest = &rest[pos + 1..];
                let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if digits.is_empty() {
                    continue;
                }
                let n: usize = digits.parse().unwrap();
                assert!(
                    n >= 1 && n <= count,
                    "reference #{n} in `{line}` points outside 1..={count}"
                );
            }
        }
    }
}
