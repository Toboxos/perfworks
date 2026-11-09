//! Derived electrical connectivity.
//!
//! The netlist is **computed from geometry on every rebuild and never authored**.
//! Connectivity already exists in the document as wires, solder traces, stripboard
//! copper and breadboard terminal strips; storing a parallel net list would desync on
//! the first edit.
//!
//! Net *names*, by contrast, are authored — as [`crate::model::NetLabel`]s anchored to a
//! hole. See that type for why the name cannot live on the net itself.
//!
//! This module answers *electrical* questions only. The *mechanical* question "can a pin
//! physically go in this hole" belongs to [`crate::model::HoleOccupancy`]; the two are
//! orthogonal and both are needed.

use std::collections::HashMap;

use uuid::Uuid;

use crate::library::get_footprint_by_id;
use crate::model::{BoardKind, GridPos, Project};

/// Index into [`Netlist::nets`]. Valid only for the `generation` that produced it —
/// never store one in a selection or a saved file.
pub type NetId = usize;

/// How a net got (or did not get) its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetName {
    /// No label landed in this net. The ordinal is display-only and never persisted.
    Unnamed(u32),
    Named(String),
    /// Two or more differently-named labels ended up in one net — e.g. GND accidentally
    /// bridged to VCC. Names are sorted and deduplicated.
    Conflict(Vec<String>),
}

impl NetName {
    pub fn display(&self) -> String {
        match self {
            NetName::Unnamed(n) => format!("N${}", n),
            NetName::Named(name) => name.clone(),
            NetName::Conflict(names) => names.join(" ⚡ "),
        }
    }

    pub fn is_conflict(&self) -> bool {
        matches!(self, NetName::Conflict(_))
    }

    pub fn is_named(&self) -> bool {
        matches!(self, NetName::Named(_))
    }
}

/// A connector pin that is a member of a net.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinRef {
    pub connector: Uuid,
    pub index: usize,
    pub designator: String,
    pub pin_name: String,
    pub hole: GridPos,
}

impl PinRef {
    /// Human-readable `U1.3 (TRIG)` form used in the net tree.
    pub fn display(&self) -> String {
        if self.pin_name.is_empty() {
            format!("{}.{}", self.designator, self.index + 1)
        } else {
            format!("{}.{} ({})", self.designator, self.index + 1, self.pin_name)
        }
    }
}

/// One electrically connected group.
#[derive(Debug, Clone)]
pub struct Net {
    pub id: NetId,
    pub name: NetName,
    /// Every hole in the group, sorted by (row, col). Includes bare strip holes, so the
    /// highlight overlay can light up a whole copper track.
    pub holes: Vec<GridPos>,
    /// The subset of `holes` where something actually lands — a pin, a wire end, a solder
    /// node or a net label — as opposed to holes that merely sit on the same copper strip.
    /// The overlay marks these strongly and the rest with a faint wash, which is what
    /// keeps a stripboard rail from turning into a wall of circles.
    pub contacts: Vec<GridPos>,
    pub pins: Vec<PinRef>,
    pub wires: Vec<Uuid>,
    pub solder_lines: Vec<Uuid>,
    pub labels: Vec<Uuid>,
}

impl Net {
    /// The anchor used to refer to this net across rebuilds: its lowest hole in
    /// (row, col) order.
    pub fn anchor(&self) -> GridPos {
        self.holes[0]
    }
}

/// Disjoint-set over hole indices, with union by size and path halving.
struct Dsu {
    parent: Vec<usize>,
    size: Vec<usize>,
}

impl Dsu {
    fn with_capacity(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            size: vec![1; n],
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) {
        let (mut ra, mut rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        if self.size[ra] < self.size[rb] {
            std::mem::swap(&mut ra, &mut rb);
        }
        self.parent[rb] = ra;
        self.size[ra] += self.size[rb];
    }
}

/// Bidirectional lookup between holes/pins and the nets they belong to.
#[derive(Debug, Clone, Default)]
pub struct Netlist {
    pub generation: u64,
    pub nets: Vec<Net>,
    hole_to_net: HashMap<GridPos, NetId>,
    pin_to_net: HashMap<(Uuid, usize), NetId>,
}

impl Netlist {
    pub fn net_of_hole(&self, hole: GridPos) -> Option<NetId> {
        self.hole_to_net.get(&hole).copied()
    }

    pub fn net_of_pin(&self, connector: Uuid, pin_index: usize) -> Option<NetId> {
        self.pin_to_net.get(&(connector, pin_index)).copied()
    }

    pub fn net(&self, id: NetId) -> Option<&Net> {
        self.nets.get(id)
    }

    /// Resolve a stable anchor hole back to a net. Used by the highlight, which stores a
    /// `GridPos` rather than a `NetId` so it survives rebuilds, edits and undo.
    pub fn net_at(&self, anchor: GridPos) -> Option<&Net> {
        self.net_of_hole(anchor).and_then(|id| self.net(id))
    }

    pub fn conflicts(&self) -> impl Iterator<Item = &Net> {
        self.nets.iter().filter(|n| n.name.is_conflict())
    }

    /// The net a wire belongs to. A wire joins its two endpoints, so it is always
    /// exactly one net — unlike a connector, whose pins may span several.
    pub fn net_of_wire(&self, wire: Uuid) -> Option<&Net> {
        self.nets.iter().find(|n| n.wires.contains(&wire))
    }

    pub fn net_of_solder_line(&self, solder: Uuid) -> Option<&Net> {
        self.nets.iter().find(|n| n.solder_lines.contains(&solder))
    }

    /// Every net touched by a connector's pins, in net order and deduplicated. A
    /// multi-pin part normally spans one net per pin.
    pub fn nets_of_connector(&self, connector: Uuid) -> Vec<&Net> {
        let mut ids: Vec<NetId> = self
            .nets
            .iter()
            .filter(|n| n.pins.iter().any(|p| p.connector == connector))
            .map(|n| n.id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids.into_iter().filter_map(|id| self.net(id)).collect()
    }

    /// Rebuild from scratch. O(holes) with near-linear union-find.
    pub fn build(project: &Project, generation: u64) -> Self {
        // 1. Materialise every board hole as a DSU node. A few thousand holes is cheap,
        //    and it lets the highlight light up whole strips rather than only the
        //    occupied holes on them. If this ever shows up in a profile, the fix is
        //    virtual nodes — one per cut-delimited strip segment.
        let mut index: HashMap<GridPos, usize> = HashMap::new();
        let mut holes: Vec<GridPos> = Vec::new();
        for board in &project.boards {
            for r in 0..board.rows as i32 {
                for c in 0..board.cols as i32 {
                    // Overlapping boards share holes; first board to claim one wins,
                    // matching how `find_board_at_hole` resolves ambiguity.
                    let g = board.local_to_global(GridPos::new(c, r));
                    index.entry(g).or_insert_with(|| {
                        holes.push(g);
                        holes.len() - 1
                    });
                }
            }
            // Edge pads are isolated copper, so they join no strip below — but they are
            // solderable, so they must exist as nodes or a wire landing on one would have
            // no net at all.
            for g in board.edge_pad_holes() {
                index.entry(g).or_insert_with(|| {
                    holes.push(g);
                    holes.len() - 1
                });
            }
        }

        let mut dsu = Dsu::with_capacity(holes.len());
        let node = |g: GridPos| index.get(&g).copied();

        // 2. Board copper.
        for board in &project.boards {
            match board.kind {
                BoardKind::Perfboard => {} // isolated pads, nothing to join
                BoardKind::StripboardHorizontal => {
                    for r in 0..board.rows as i32 {
                        let mut prev: Option<usize> = None;
                        for c in 0..board.cols as i32 {
                            let local = GridPos::new(c, r);
                            // A cut severs the copper *at* this hole: the pad stays, but
                            // the track does not continue through it.
                            if board.track_cuts.contains(&local) {
                                prev = None;
                                continue;
                            }
                            let Some(cur) = node(board.local_to_global(local)) else {
                                continue;
                            };
                            if let Some(p) = prev {
                                dsu.union(p, cur);
                            }
                            prev = Some(cur);
                        }
                    }
                }
                BoardKind::StripboardVertical => {
                    for c in 0..board.cols as i32 {
                        let mut prev: Option<usize> = None;
                        for r in 0..board.rows as i32 {
                            let local = GridPos::new(c, r);
                            if board.track_cuts.contains(&local) {
                                prev = None;
                                continue;
                            }
                            let Some(cur) = node(board.local_to_global(local)) else {
                                continue;
                            };
                            if let Some(p) = prev {
                                dsu.union(p, cur);
                            }
                            prev = Some(cur);
                        }
                    }
                }
                BoardKind::Breadboard => {
                    // Matches what `canvas::draw_board` actually paints: two half-column
                    // groups split by the centre trench. Note this disagrees with the
                    // "5-hole terminal strips" wording in the docs and with real
                    // breadboards; the renderer and the netlist must not disagree with
                    // each other, so both follow this rule until it is changed together.
                    let mid = board.rows as i32 / 2;
                    for c in 0..board.cols as i32 {
                        for (from, to) in [(0, mid - 1), (mid, board.rows as i32 - 1)] {
                            let mut prev: Option<usize> = None;
                            for r in from..=to {
                                let local = GridPos::new(c, r);
                                if board.track_cuts.contains(&local) {
                                    prev = None;
                                    continue;
                                }
                                let Some(cur) = node(board.local_to_global(local)) else {
                                    continue;
                                };
                                if let Some(p) = prev {
                                    dsu.union(p, cur);
                                }
                                prev = Some(cur);
                            }
                        }
                    }
                }
            }
        }

        // 3. Solder traces join consecutive nodes.
        for solder in &project.solder_lines {
            for pair in solder.nodes.windows(2) {
                if let (Some(a), Some(b)) = (node(pair[0]), node(pair[1])) {
                    dsu.union(a, b);
                }
            }
        }

        // 4. Wires join their two endpoint holes. Endpoints are global, so a wire
        //    spanning two boards produces a single net for free.
        for wire in &project.wires {
            if let (Some(a), Some(b)) = (node(wire.start_hole), node(wire.end_hole)) {
                dsu.union(a, b);
            }
        }

        // 5. Connectivity inside a part (e.g. the shorted lead pairs of a tact switch).
        for conn in &project.connectors {
            let ConnectorKindPins { pins, internal } = connector_pins(conn);
            for group in internal {
                let mut members = group
                    .iter()
                    .filter_map(|&i| pins.get(i))
                    .filter_map(|(hole, _): &(GridPos, String)| node(*hole));
                if let Some(first) = members.next() {
                    for m in members {
                        dsu.union(first, m);
                    }
                }
            }
        }

        // 6. Collect members per root.
        let mut groups: HashMap<usize, Vec<GridPos>> = HashMap::new();
        for (i, &g) in holes.iter().enumerate() {
            groups.entry(dsu.find(i)).or_default().push(g);
        }

        let mut pins_by_root: HashMap<usize, Vec<PinRef>> = HashMap::new();
        for conn in &project.connectors {
            let ConnectorKindPins { pins, .. } = connector_pins(conn);
            for (i, (hole, pin_name)) in pins.into_iter().enumerate() {
                let Some(n) = node(hole) else { continue };
                pins_by_root.entry(dsu.find(n)).or_default().push(PinRef {
                    connector: conn.id,
                    index: i,
                    designator: conn.designator.clone(),
                    pin_name,
                    hole,
                });
            }
        }

        let mut wires_by_root: HashMap<usize, Vec<Uuid>> = HashMap::new();
        for wire in &project.wires {
            if let Some(n) = node(wire.start_hole) {
                wires_by_root.entry(dsu.find(n)).or_default().push(wire.id);
            }
        }

        let mut solder_by_root: HashMap<usize, Vec<Uuid>> = HashMap::new();
        for solder in &project.solder_lines {
            if let Some(n) = solder.nodes.first().and_then(|&g| node(g)) {
                solder_by_root
                    .entry(dsu.find(n))
                    .or_default()
                    .push(solder.id);
            }
        }

        let mut labels_by_root: HashMap<usize, Vec<(Uuid, String)>> = HashMap::new();
        for label in &project.net_labels {
            if let Some(n) = node(label.hole) {
                labels_by_root
                    .entry(dsu.find(n))
                    .or_default()
                    .push((label.id, label.name.clone()));
            }
        }

        // 7. Keep only nets that carry something. Without this every bare hole on the
        //    board becomes a singleton net and the tree shows thousands of rows.
        let mut roots: Vec<usize> = groups
            .keys()
            .copied()
            .filter(|r| {
                pins_by_root.contains_key(r)
                    || wires_by_root.contains_key(r)
                    || solder_by_root.contains_key(r)
                    || labels_by_root.contains_key(r)
            })
            .collect();

        // Order nets by their lowest hole so that `Unnamed` ordinals and tree order stay
        // stable across rebuilds instead of following HashMap iteration order.
        let root_anchor = |r: &usize| {
            groups[r]
                .iter()
                .map(|g| (g.row, g.col))
                .min()
                .unwrap_or((i32::MAX, i32::MAX))
        };
        roots.sort_by_key(root_anchor);

        let mut nets = Vec::with_capacity(roots.len());
        let mut hole_to_net = HashMap::new();
        let mut pin_to_net = HashMap::new();
        let mut unnamed_counter = 0_u32;

        for (id, root) in roots.into_iter().enumerate() {
            let mut net_holes = groups.remove(&root).unwrap_or_default();
            net_holes.sort_by_key(|g| (g.row, g.col));

            let label_entries = labels_by_root.remove(&root).unwrap_or_default();
            let mut distinct: Vec<String> = label_entries
                .iter()
                .map(|(_, name)| name.clone())
                .filter(|n| !n.trim().is_empty())
                .collect();
            distinct.sort();
            distinct.dedup();

            let name = match distinct.len() {
                0 => {
                    unnamed_counter += 1;
                    NetName::Unnamed(unnamed_counter)
                }
                1 => NetName::Named(distinct.remove(0)),
                _ => NetName::Conflict(distinct),
            };

            let pins = pins_by_root.remove(&root).unwrap_or_default();
            for &hole in &net_holes {
                hole_to_net.insert(hole, id);
            }
            for p in &pins {
                pin_to_net.insert((p.connector, p.index), id);
            }

            let wires = wires_by_root.remove(&root).unwrap_or_default();
            let solder_lines = solder_by_root.remove(&root).unwrap_or_default();

            // Holes where something lands, as opposed to bare strip copper.
            let mut contacts: Vec<GridPos> = pins.iter().map(|p| p.hole).collect();
            for wire in project.wires.iter().filter(|w| wires.contains(&w.id)) {
                contacts.push(wire.start_hole);
                contacts.push(wire.end_hole);
            }
            for line in project
                .solder_lines
                .iter()
                .filter(|s| solder_lines.contains(&s.id))
            {
                contacts.extend(line.nodes.iter().copied());
            }
            for label in project
                .net_labels
                .iter()
                .filter(|l| label_entries.iter().any(|(id, _)| *id == l.id))
            {
                contacts.push(label.hole);
            }
            contacts.sort_by_key(|g| (g.row, g.col));
            contacts.dedup();

            nets.push(Net {
                id,
                name,
                holes: net_holes,
                contacts,
                pins,
                wires,
                solder_lines,
                labels: label_entries.into_iter().map(|(id, _)| id).collect(),
            });
        }

        Self {
            generation,
            nets,
            hole_to_net,
            pin_to_net,
        }
    }
}

struct ConnectorKindPins {
    pins: Vec<(GridPos, String)>,
    internal: Vec<Vec<usize>>,
}

/// Pin holes plus any part-internal connectivity, in one pass so the footprint is looked
/// up once per connector rather than once per query.
fn connector_pins(conn: &crate::model::PlacedConnector) -> ConnectorKindPins {
    let internal = match &conn.kind {
        crate::model::ConnectorKind::LibraryFootprint { footprint_id } => {
            get_footprint_by_id(footprint_id)
                .map(|fp| fp.internal_nets.clone())
                .unwrap_or_default()
        }
        _ => Vec::new(),
    };
    ConnectorKindPins {
        pins: conn.pin_positions(),
        internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        BoardTheme, ConnectorKind, NetLabel, Perfboard, PlacedConnector, RgbColor, Rotation,
        SolderLine, Wire, WireLayer, WireMode, WorldPos,
    };

    fn board(origin: GridPos, cols: u32, rows: u32, kind: BoardKind) -> Perfboard {
        let mut b = Perfboard::new("B", origin, cols, rows);
        b.kind = kind;
        b.theme = BoardTheme::ClassicGreen;
        b
    }

    fn pin_at(designator: &str, hole: GridPos) -> PlacedConnector {
        PlacedConnector {
            id: Uuid::new_v4(),
            designator: designator.to_string(),
            label: String::new(),
            anchor: hole,
            rotation: Rotation::Deg0,
            kind: ConnectorKind::SinglePin { square_pad: false },
            body_color: RgbColor::new(200, 180, 60),
            pin_labels: Vec::new(),
        }
    }

    fn wire(a: GridPos, b: GridPos) -> Wire {
        Wire {
            id: Uuid::new_v4(),
            label: String::new(),
            start_hole: a,
            end_hole: b,
            waypoints: Vec::new(),
            mode: WireMode::Straight,
            color: RgbColor::new(255, 0, 0),
            thickness: 1.0,
            layer: WireLayer::TopComponent,
        }
    }

    /// Both pins resolve to the same net id.
    fn same_net(nl: &Netlist, a: &PlacedConnector, b: &PlacedConnector) -> bool {
        match (nl.net_of_pin(a.id, 0), nl.net_of_pin(b.id, 0)) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
    }

    #[test]
    fn wire_joins_its_two_endpoints() {
        let mut p = Project::new_empty("t");
        p.boards = vec![board(GridPos::new(0, 0), 10, 10, BoardKind::Perfboard)];
        let a = pin_at("P1", GridPos::new(1, 1));
        let b = pin_at("P2", GridPos::new(8, 8));
        let c = pin_at("P3", GridPos::new(4, 4));
        p.wires = vec![wire(a.anchor, b.anchor)];
        p.connectors = vec![a.clone(), b.clone(), c.clone()];

        let nl = Netlist::build(&p, 1);
        assert!(same_net(&nl, &a, &b));
        assert!(!same_net(&nl, &a, &c));
    }

    #[test]
    fn solder_chain_joins_its_whole_run() {
        let mut p = Project::new_empty("t");
        p.boards = vec![board(GridPos::new(0, 0), 10, 10, BoardKind::Perfboard)];
        let a = pin_at("P1", GridPos::new(2, 3));
        let b = pin_at("P2", GridPos::new(5, 3));
        p.solder_lines = vec![SolderLine::new(vec![
            GridPos::new(2, 3),
            GridPos::new(3, 3),
            GridPos::new(4, 3),
            GridPos::new(5, 3),
        ])];
        p.connectors = vec![a.clone(), b.clone()];

        let nl = Netlist::build(&p, 1);
        assert!(same_net(&nl, &a, &b));
        // The intermediate bare holes belong to the net too, so the overlay can light
        // up the whole trace.
        let net = nl.net_at(GridPos::new(2, 3)).unwrap();
        assert!(net.holes.contains(&GridPos::new(4, 3)));
    }

    #[test]
    fn stripboard_row_joins_and_a_track_cut_splits_it() {
        let mut b = board(GridPos::new(0, 0), 10, 4, BoardKind::StripboardHorizontal);
        let left = pin_at("P1", GridPos::new(1, 2));
        let right = pin_at("P2", GridPos::new(8, 2));

        let mut p = Project::new_empty("t");
        p.boards = vec![b.clone()];
        p.connectors = vec![left.clone(), right.clone()];
        assert!(
            same_net(&Netlist::build(&p, 1), &left, &right),
            "an uncut stripboard row is one net"
        );

        b.track_cuts = vec![GridPos::new(5, 2)];
        p.boards = vec![b];
        assert!(
            !same_net(&Netlist::build(&p, 2), &left, &right),
            "a track cut must sever the row"
        );
    }

    #[test]
    fn vertical_stripboard_joins_columns_not_rows() {
        let mut p = Project::new_empty("t");
        p.boards = vec![board(
            GridPos::new(0, 0),
            6,
            6,
            BoardKind::StripboardVertical,
        )];
        let top = pin_at("P1", GridPos::new(3, 0));
        let bottom = pin_at("P2", GridPos::new(3, 5));
        let across = pin_at("P3", GridPos::new(4, 0));
        p.connectors = vec![top.clone(), bottom.clone(), across.clone()];

        let nl = Netlist::build(&p, 1);
        assert!(same_net(&nl, &top, &bottom));
        assert!(!same_net(&nl, &top, &across));
    }

    #[test]
    fn breadboard_groups_split_at_the_centre_trench() {
        let mut p = Project::new_empty("t");
        p.boards = vec![board(GridPos::new(0, 0), 10, 10, BoardKind::Breadboard)];
        let upper_a = pin_at("P1", GridPos::new(3, 0));
        let upper_b = pin_at("P2", GridPos::new(3, 4));
        let lower = pin_at("P3", GridPos::new(3, 5));
        p.connectors = vec![upper_a.clone(), upper_b.clone(), lower.clone()];

        let nl = Netlist::build(&p, 1);
        assert!(same_net(&nl, &upper_a, &upper_b));
        assert!(
            !same_net(&nl, &upper_b, &lower),
            "the trench separates the two halves"
        );
    }

    #[test]
    fn internal_nets_short_the_tact_switch_lead_pairs() {
        let mut p = Project::new_empty("t");
        p.boards = vec![board(GridPos::new(0, 0), 10, 10, BoardKind::Perfboard)];
        let sw = PlacedConnector {
            id: Uuid::new_v4(),
            designator: "SW1".into(),
            label: String::new(),
            anchor: GridPos::new(2, 2),
            rotation: Rotation::Deg0,
            kind: ConnectorKind::LibraryFootprint {
                footprint_id: "tact_switch_6mm".into(),
            },
            body_color: RgbColor::new(52, 56, 64),
            pin_labels: Vec::new(),
        };
        let id = sw.id;
        p.connectors = vec![sw];

        let nl = Netlist::build(&p, 1);
        // Pins 1+3 share a side, as do 2+4; the two terminals stay apart (button open).
        assert_eq!(nl.net_of_pin(id, 0), nl.net_of_pin(id, 2));
        assert_eq!(nl.net_of_pin(id, 1), nl.net_of_pin(id, 3));
        assert_ne!(nl.net_of_pin(id, 0), nl.net_of_pin(id, 1));
    }

    #[test]
    fn a_jst_does_not_short_its_own_pins() {
        let mut p = Project::new_empty("t");
        p.boards = vec![board(GridPos::new(0, 0), 10, 10, BoardKind::Perfboard)];
        let j = PlacedConnector {
            id: Uuid::new_v4(),
            designator: "J1".into(),
            label: String::new(),
            anchor: GridPos::new(2, 2),
            rotation: Rotation::Deg0,
            kind: ConnectorKind::LibraryFootprint {
                footprint_id: "jst_xh_4".into(),
            },
            body_color: RgbColor::new(238, 236, 224),
            pin_labels: Vec::new(),
        };
        let id = j.id;
        p.connectors = vec![j];

        let nl = Netlist::build(&p, 1);
        assert_ne!(nl.net_of_pin(id, 0), nl.net_of_pin(id, 1));
    }

    #[test]
    fn a_wire_across_two_boards_is_one_net() {
        let mut p = Project::new_empty("t");
        p.boards = vec![
            board(GridPos::new(0, 0), 8, 8, BoardKind::Perfboard),
            board(GridPos::new(20, 0), 8, 8, BoardKind::Perfboard),
        ];
        let a = pin_at("P1", GridPos::new(2, 2));
        let b = pin_at("P2", GridPos::new(22, 3));
        p.wires = vec![wire(a.anchor, b.anchor)];
        p.connectors = vec![a.clone(), b.clone()];

        let nl = Netlist::build(&p, 1);
        assert!(same_net(&nl, &a, &b), "nets must span boards");
    }

    #[test]
    fn labels_resolve_to_unnamed_named_and_conflict() {
        let mut p = Project::new_empty("t");
        p.boards = vec![board(GridPos::new(0, 0), 12, 12, BoardKind::Perfboard)];
        let a = pin_at("P1", GridPos::new(1, 1));
        let b = pin_at("P2", GridPos::new(5, 1));
        p.wires = vec![wire(a.anchor, b.anchor)];
        p.connectors = vec![a.clone(), b];

        // 0 labels -> unnamed
        let nl = Netlist::build(&p, 1);
        let net = nl.net_at(GridPos::new(1, 1)).unwrap();
        assert!(matches!(net.name, NetName::Unnamed(_)));

        // 1 label -> named
        p.net_labels = vec![NetLabel::new(GridPos::new(1, 1), "VCC")];
        let nl = Netlist::build(&p, 2);
        let net = nl.net_at(GridPos::new(1, 1)).unwrap();
        assert_eq!(net.name, NetName::Named("VCC".into()));

        // 2 distinct labels in one net -> conflict
        p.net_labels.push(NetLabel::new(GridPos::new(5, 1), "GND"));
        let nl = Netlist::build(&p, 3);
        let net = nl.net_at(GridPos::new(1, 1)).unwrap();
        assert_eq!(
            net.name,
            NetName::Conflict(vec!["GND".into(), "VCC".into()])
        );
        assert_eq!(nl.conflicts().count(), 1);

        // the same name twice is not a conflict
        p.net_labels = vec![
            NetLabel::new(GridPos::new(1, 1), "VCC"),
            NetLabel::new(GridPos::new(5, 1), "VCC"),
        ];
        let nl = Netlist::build(&p, 4);
        assert_eq!(
            nl.net_at(GridPos::new(1, 1)).unwrap().name,
            NetName::Named("VCC".into())
        );
    }

    #[test]
    fn connecting_an_unnamed_group_onto_a_label_absorbs_it() {
        let mut p = Project::new_empty("t");
        p.boards = vec![board(GridPos::new(0, 0), 12, 12, BoardKind::Perfboard)];
        let a = pin_at("P1", GridPos::new(1, 1));
        let b = pin_at("P2", GridPos::new(3, 1));
        let vcc = pin_at("P3", GridPos::new(9, 9));
        p.connectors = vec![a.clone(), b.clone(), vcc.clone()];
        p.net_labels = vec![NetLabel::new(GridPos::new(9, 9), "VCC")];

        // a—b wired together, separate from the VCC pin
        p.wires = vec![wire(a.anchor, b.anchor)];
        let nl = Netlist::build(&p, 1);
        assert!(matches!(
            nl.net_at(GridPos::new(1, 1)).unwrap().name,
            NetName::Unnamed(_)
        ));
        assert!(!same_net(&nl, &a, &vcc));

        // now join the pair onto VCC — the whole group becomes VCC, no special case
        p.wires.push(wire(b.anchor, vcc.anchor));
        let nl = Netlist::build(&p, 2);
        assert_eq!(
            nl.net_at(GridPos::new(1, 1)).unwrap().name,
            NetName::Named("VCC".into())
        );
        assert!(same_net(&nl, &a, &vcc));
    }

    #[test]
    fn hole_and_pin_lookups_agree_for_every_pin() {
        let p = Project::new_starter();
        let nl = Netlist::build(&p, 1);
        for conn in &p.connectors {
            for (i, (hole, _)) in conn.pin_positions().into_iter().enumerate() {
                if let Some(via_pin) = nl.net_of_pin(conn.id, i) {
                    assert_eq!(Some(via_pin), nl.net_of_hole(hole));
                }
            }
        }
    }

    #[test]
    fn isolated_bare_holes_produce_no_net() {
        let mut p = Project::new_empty("t");
        p.boards = vec![board(GridPos::new(0, 0), 10, 10, BoardKind::Perfboard)];
        let nl = Netlist::build(&p, 1);
        assert!(nl.nets.is_empty());
        assert_eq!(nl.net_of_hole(GridPos::new(4, 4)), None);
    }

    #[test]
    fn unnamed_ordinals_are_stable_across_an_unrelated_edit() {
        let mut p = Project::new_empty("t");
        p.boards = vec![board(GridPos::new(0, 0), 20, 20, BoardKind::Perfboard)];
        let a = pin_at("P1", GridPos::new(1, 1));
        let b = pin_at("P2", GridPos::new(4, 1));
        let c = pin_at("P3", GridPos::new(1, 9));
        let d = pin_at("P4", GridPos::new(4, 9));
        p.wires = vec![wire(a.anchor, b.anchor), wire(c.anchor, d.anchor)];
        p.connectors = vec![a, b, c, d];

        let before = Netlist::build(&p, 1)
            .net_at(GridPos::new(1, 9))
            .unwrap()
            .name
            .clone();

        // An edit somewhere else entirely must not renumber this net.
        p.connectors.push(pin_at("P5", GridPos::new(15, 15)));
        p.solder_lines = vec![SolderLine::new(vec![
            GridPos::new(15, 15),
            GridPos::new(16, 15),
        ])];
        let after = Netlist::build(&p, 2)
            .net_at(GridPos::new(1, 9))
            .unwrap()
            .name
            .clone();

        assert_eq!(before, after);
    }

    #[test]
    fn starter_project_resolves_its_named_buses() {
        let p = Project::new_starter();
        let nl = Netlist::build(&p, 1);
        let names: Vec<String> = nl
            .nets
            .iter()
            .filter(|n| n.name.is_named())
            .map(|n| n.name.display())
            .collect();
        for expected in ["VCC", "GND", "I2C_SDA", "I2C_SCL"] {
            assert!(
                names.iter().any(|n| n == expected),
                "demo circuit should expose a {expected} net, got {names:?}"
            );
        }
        assert_eq!(nl.conflicts().count(), 0, "demo circuit must be clean");
    }

    #[test]
    fn waypoints_do_not_connect_holes_they_pass_over() {
        let mut p = Project::new_empty("t");
        p.boards = vec![board(GridPos::new(0, 0), 10, 10, BoardKind::Perfboard)];
        let a = pin_at("P1", GridPos::new(1, 1));
        let b = pin_at("P2", GridPos::new(1, 8));
        let crossed = pin_at("P3", GridPos::new(5, 5));
        let mut w = wire(a.anchor, b.anchor);
        w.waypoints = vec![WorldPos::new(5.0, 5.0)];
        p.wires = vec![w];
        p.connectors = vec![a.clone(), b.clone(), crossed.clone()];

        let nl = Netlist::build(&p, 1);
        assert!(same_net(&nl, &a, &b));
        assert!(
            !same_net(&nl, &a, &crossed),
            "a wire routed over a hole does not land in it"
        );
    }
}
