//! Quick actions for the toolbar's context row.
//!
//! The row is open for extension: adding an action means writing one unit struct and
//! adding one line to [`context_actions`]. Nothing else in the app changes.
//!
//! Three responsibilities, deliberately split:
//!
//! 1. [`ContextScope`] — a **read-only** view of the state an action may inspect.
//! 2. [`ContextAction`] — decides whether it applies, and draws itself. It cannot touch
//!    the app; it returns a [`ContextIntent`] describing what the user asked for. This is
//!    not merely stylistic: an action reachable *from* the app cannot also borrow the app
//!    mutably, so returning an intent is what makes the whole arrangement compile.
//! 3. `PerfworkApp::apply_context_intent` — the only place that mutates, and its arms
//!    only delegate to methods that already exist.

use egui::{Color32, RichText, Ui};

use crate::canvas::SelectedItem;
use crate::model::{GridPos, Project, RgbColor, WireMode};
use crate::netlist::Netlist;

/// Preset conductor widths, cycled by the width button and the `T` key rather than
/// dragged on a slider. Both arrays stay inside the ranges the inspectors allow
/// (wire `0.6..=2.2`, solder `0.7..=1.5`).
pub const WIRE_WIDTH_STEPS: [f32; 5] = [0.7, 1.0, 1.4, 1.8, 2.2];
pub const SOLDER_WIDTH_STEPS: [f32; 4] = [0.8, 1.0, 1.25, 1.5];

/// Advance to the next preset at or after the current value, wrapping at the end.
pub fn cycle_width(current: f32, steps: &[f32]) -> f32 {
    let idx = steps.iter().position(|s| *s > current + 1e-4).unwrap_or(0);
    steps[idx]
}

/// Which preset a value sits on, for the `2/5` readout. Nearest match, since a project
/// loaded from JSON may hold any value in range.
pub fn width_step_index(current: f32, steps: &[f32]) -> usize {
    steps
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (*a - current)
                .abs()
                .partial_cmp(&(*b - current).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Visual grouping within the context row; groups are separated in the order listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActionGroup {
    /// Options for the active tool (wire mode, colour, header size, placement rotation).
    ToolOptions,
    /// Acts on the selected object.
    Selection,
    /// Acts on the net the selection belongs to.
    Net,
}

impl ActionGroup {
    pub const ORDER: [ActionGroup; 3] = [
        ActionGroup::ToolOptions,
        ActionGroup::Selection,
        ActionGroup::Net,
    ];
}

/// What the user asked for. Pure data, applied by the app afterwards.
#[derive(Debug, Clone, PartialEq)]
pub enum ContextIntent {
    ToggleFocusNet,
    PinNet(GridPos),
    UnpinNet,
    NameNet(GridPos),
    ZoomToSelection,
    SetWireMode(WireMode),
    SetWireColor(RgbColor),
    CycleConductorWidth,
    SetHeaderSize { cols: u32, rows: u32 },
    RotatePlacement,
}

/// Read-only state an action may consult.
pub struct ContextScope<'a> {
    pub selected: &'a [SelectedItem],
    pub project: &'a Project,
    pub netlist: &'a Netlist,
    pub tool: crate::app::ToolMode,
    pub focus_net_mode: bool,
    pub highlighted_net: Option<GridPos>,
    pub wire_mode: WireMode,
    pub wire_color: RgbColor,
    pub header_cols: u32,
    pub header_rows: u32,
    pub active_rotation: crate::model::Rotation,
}

impl ContextScope<'_> {
    /// The net anchor for the current selection, if it has exactly one. Several actions
    /// key off this, so it lives here rather than being re-derived per action.
    pub fn selected_net(&self) -> Option<GridPos> {
        match self.selected {
            [SelectedItem::Wire(id)] => self.netlist.net_of_wire(*id).map(|n| n.anchor()),
            [SelectedItem::SolderLine(id)] => {
                self.netlist.net_of_solder_line(*id).map(|n| n.anchor())
            }
            [SelectedItem::Connector(id)] => match self.netlist.nets_of_connector(*id).as_slice() {
                [only] => Some(only.anchor()),
                _ => None,
            },
            _ => None,
        }
    }

    /// The selected conductor's current width, if one is selected.
    pub fn selected_conductor_width(&self) -> Option<(f32, &'static [f32])> {
        match self.selected {
            [SelectedItem::Wire(id)] => self
                .project
                .wires
                .iter()
                .find(|w| w.id == *id)
                .map(|w| (w.thickness, WIRE_WIDTH_STEPS.as_slice())),
            [SelectedItem::SolderLine(id)] => self
                .project
                .solder_lines
                .iter()
                .find(|s| s.id == *id)
                .map(|s| (s.width_scale, SOLDER_WIDTH_STEPS.as_slice())),
            _ => None,
        }
    }

    pub fn has_selection(&self) -> bool {
        !self.selected.is_empty()
    }
}

/// One quick action. Stateless: implementors are unit structs in a static registry.
pub trait ContextAction: Sync {
    /// Stable identifier, used by tests to pin coverage.
    fn id(&self) -> &'static str;
    fn group(&self) -> ActionGroup;
    /// Whether this action is offered for the given state.
    fn applies(&self, scope: &ContextScope) -> bool;
    /// Draw the control. Returns an intent if the user operated it.
    fn ui(&self, ui: &mut Ui, scope: &ContextScope) -> Option<ContextIntent>;
}

// ---------------------------------------------------------------- tool options

struct WireRoutingMode;
impl ContextAction for WireRoutingMode {
    fn id(&self) -> &'static str {
        "wire.mode"
    }
    fn group(&self) -> ActionGroup {
        ActionGroup::ToolOptions
    }
    fn applies(&self, scope: &ContextScope) -> bool {
        scope.tool == crate::app::ToolMode::DrawWire
            || matches!(scope.selected, [SelectedItem::Wire(_)])
    }
    fn ui(&self, ui: &mut Ui, scope: &ContextScope) -> Option<ContextIntent> {
        // Reflects the selected wire when there is one, else the tool default.
        let current = match scope.selected {
            [SelectedItem::Wire(id)] => scope
                .project
                .wires
                .iter()
                .find(|w| w.id == *id)
                .map(|w| w.mode)
                .unwrap_or(scope.wire_mode),
            _ => scope.wire_mode,
        };
        let mut intent = None;
        ui.label(RichText::new("Route").size(10.5));
        for mode in [WireMode::Straight, WireMode::Spline] {
            let label = match mode {
                WireMode::Straight => "Straight",
                WireMode::Spline => "Spline",
            };
            if ui.selectable_label(current == mode, label).clicked() {
                intent = Some(ContextIntent::SetWireMode(mode));
            }
        }
        ui.label(
            RichText::new("(Tab)")
                .size(9.5)
                .color(Color32::from_rgb(130, 140, 156)),
        );
        intent
    }
}

struct WireColorSwatches;
impl ContextAction for WireColorSwatches {
    fn id(&self) -> &'static str {
        "wire.color"
    }
    fn group(&self) -> ActionGroup {
        ActionGroup::ToolOptions
    }
    fn applies(&self, scope: &ContextScope) -> bool {
        scope.tool == crate::app::ToolMode::DrawWire
            || matches!(scope.selected, [SelectedItem::Wire(_)])
    }
    fn ui(&self, ui: &mut Ui, scope: &ContextScope) -> Option<ContextIntent> {
        let current = match scope.selected {
            [SelectedItem::Wire(id)] => scope
                .project
                .wires
                .iter()
                .find(|w| w.id == *id)
                .map(|w| w.color)
                .unwrap_or(scope.wire_color),
            _ => scope.wire_color,
        };
        let mut intent = None;
        for (name, color) in crate::app::WIRE_PALETTE {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(15.0, 15.0), egui::Sense::click());
            ui.painter().rect_filled(rect, 3.0, color.to_color32());
            if current == *color {
                ui.painter().rect_stroke(
                    rect,
                    3.0,
                    egui::Stroke::new(1.8_f32, Color32::WHITE),
                    egui::StrokeKind::Inside,
                );
            }
            if response.on_hover_text(*name).clicked() {
                intent = Some(ContextIntent::SetWireColor(*color));
            }
        }
        intent
    }
}

struct ConductorWidth;
impl ContextAction for ConductorWidth {
    fn id(&self) -> &'static str {
        "conductor.width"
    }
    fn group(&self) -> ActionGroup {
        ActionGroup::ToolOptions
    }
    fn applies(&self, scope: &ContextScope) -> bool {
        scope.selected_conductor_width().is_some()
    }
    fn ui(&self, ui: &mut Ui, scope: &ContextScope) -> Option<ContextIntent> {
        let (current, steps) = scope.selected_conductor_width()?;
        let step = width_step_index(current, steps);
        ui.button(format!("Width {}/{} (T)", step + 1, steps.len()))
            .on_hover_text("Cycle through conductor widths — click or press T")
            .clicked()
            .then_some(ContextIntent::CycleConductorWidth)
    }
}

struct HeaderSize;
impl ContextAction for HeaderSize {
    fn id(&self) -> &'static str {
        "header.size"
    }
    fn group(&self) -> ActionGroup {
        ActionGroup::ToolOptions
    }
    fn applies(&self, scope: &ContextScope) -> bool {
        scope.tool == crate::app::ToolMode::PlaceHeaderLine
    }
    fn ui(&self, ui: &mut Ui, scope: &ContextScope) -> Option<ContextIntent> {
        let mut cols = scope.header_cols;
        let mut rows = scope.header_rows;
        ui.label(RichText::new("Pins").size(10.5));
        let a = ui.add(egui::DragValue::new(&mut cols).range(1..=40));
        ui.label("×");
        let b = ui.add(egui::DragValue::new(&mut rows).range(1..=4));
        (a.changed() || b.changed()).then_some(ContextIntent::SetHeaderSize { cols, rows })
    }
}

struct PlacementRotation;
impl ContextAction for PlacementRotation {
    fn id(&self) -> &'static str {
        "placement.rotate"
    }
    fn group(&self) -> ActionGroup {
        ActionGroup::ToolOptions
    }
    fn applies(&self, scope: &ContextScope) -> bool {
        // A *tool* option, not a selection action: without it a rotated part cannot be
        // placed at all except through the sidebar.
        matches!(
            scope.tool,
            crate::app::ToolMode::PlaceSinglePin
                | crate::app::ToolMode::PlaceHeaderLine
                | crate::app::ToolMode::PlaceLibraryFootprint
                | crate::app::ToolMode::PlaceBoard
        )
    }
    fn ui(&self, ui: &mut Ui, scope: &ContextScope) -> Option<ContextIntent> {
        ui.button(format!("Rotate {}° (R)", scope.active_rotation.degrees()))
            .on_hover_text("Rotate the part being placed")
            .clicked()
            .then_some(ContextIntent::RotatePlacement)
    }
}

// ------------------------------------------------------------------- selection

struct ZoomToSelection;
impl ContextAction for ZoomToSelection {
    fn id(&self) -> &'static str {
        "selection.zoom"
    }
    fn group(&self) -> ActionGroup {
        ActionGroup::Selection
    }
    fn applies(&self, scope: &ContextScope) -> bool {
        scope.has_selection()
    }
    fn ui(&self, ui: &mut Ui, _scope: &ContextScope) -> Option<ContextIntent> {
        ui.button("Zoom to fit")
            .on_hover_text("Frame the viewport on the current selection")
            .clicked()
            .then_some(ContextIntent::ZoomToSelection)
    }
}

// ------------------------------------------------------------------------ nets

struct FocusNet;
impl ContextAction for FocusNet {
    fn id(&self) -> &'static str {
        "net.focus"
    }
    fn group(&self) -> ActionGroup {
        ActionGroup::Net
    }
    fn applies(&self, scope: &ContextScope) -> bool {
        scope.highlighted_net.is_some() || scope.selected_net().is_some()
    }
    fn ui(&self, ui: &mut Ui, scope: &ContextScope) -> Option<ContextIntent> {
        let mut on = scope.focus_net_mode;
        ui.checkbox(&mut on, "Focus")
            .on_hover_text("Dim everything that is not on the pinned net")
            .changed()
            .then_some(ContextIntent::ToggleFocusNet)
    }
}

struct PinNet;
impl ContextAction for PinNet {
    fn id(&self) -> &'static str {
        "net.pin"
    }
    fn group(&self) -> ActionGroup {
        ActionGroup::Net
    }
    fn applies(&self, scope: &ContextScope) -> bool {
        scope.selected_net().is_some() || scope.highlighted_net.is_some()
    }
    fn ui(&self, ui: &mut Ui, scope: &ContextScope) -> Option<ContextIntent> {
        let selected_net = scope.selected_net();
        let pinned = scope.highlighted_net.is_some()
            && (selected_net.is_none() || scope.highlighted_net == selected_net);
        if pinned {
            ui.button("Unpin net")
                .on_hover_text("Release the pinned net highlight (Esc)")
                .clicked()
                .then_some(ContextIntent::UnpinNet)
        } else {
            let anchor = selected_net?;
            ui.button("Pin net")
                .on_hover_text("Keep this net highlighted while you work")
                .clicked()
                .then_some(ContextIntent::PinNet(anchor))
        }
    }
}

struct NameNet;
impl ContextAction for NameNet {
    fn id(&self) -> &'static str {
        "net.name"
    }
    fn group(&self) -> ActionGroup {
        ActionGroup::Net
    }
    fn applies(&self, scope: &ContextScope) -> bool {
        scope
            .selected_net()
            .or(scope.highlighted_net)
            .and_then(|a| scope.netlist.net_at(a))
            .is_some()
    }
    fn ui(&self, ui: &mut Ui, scope: &ContextScope) -> Option<ContextIntent> {
        let anchor = scope.selected_net().or(scope.highlighted_net)?;
        let net = scope.netlist.net_at(anchor)?;
        let label = if net.name.is_named() {
            format!("Net: {}", net.name.display())
        } else {
            "Name net…".to_string()
        };
        ui.button(label)
            .on_hover_text("Name this net, or rename it")
            .clicked()
            .then_some(ContextIntent::NameNet(anchor))
    }
}

/// The registry. Adding an action means adding one line here.
pub fn context_actions() -> &'static [&'static dyn ContextAction] {
    &[
        &WireRoutingMode,
        &WireColorSwatches,
        &ConductorWidth,
        &HeaderSize,
        &PlacementRotation,
        &ZoomToSelection,
        &FocusNet,
        &PinNet,
        &NameNet,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_cycling_wraps_and_stays_in_range() {
        for steps in [WIRE_WIDTH_STEPS.as_slice(), SOLDER_WIDTH_STEPS.as_slice()] {
            let lo = steps[0];
            let hi = *steps.last().unwrap();

            // Starting below the first step lands on it.
            assert_eq!(cycle_width(0.0, steps), lo);
            // Walking the whole array returns to the start.
            let mut w = lo;
            for expected in steps.iter().skip(1) {
                w = cycle_width(w, steps);
                assert_eq!(w, *expected);
            }
            assert_eq!(cycle_width(hi, steps), lo, "must wrap at the top");

            // Never leaves the range, from any starting point including junk from JSON.
            for start in [-5.0, 0.65, 1.0, 1.37, 2.2, 99.0] {
                let next = cycle_width(start, steps);
                assert!(next >= lo && next <= hi, "{next} outside [{lo}, {hi}]");
            }
        }
    }

    #[test]
    fn width_step_index_picks_the_nearest_preset() {
        assert_eq!(width_step_index(0.7, &WIRE_WIDTH_STEPS), 0);
        assert_eq!(width_step_index(2.2, &WIRE_WIDTH_STEPS), 4);
        // A value loaded from JSON that is not on a step snaps to the closest one.
        assert_eq!(width_step_index(1.05, &WIRE_WIDTH_STEPS), 1);
        assert_eq!(width_step_index(1.3, &WIRE_WIDTH_STEPS), 2);
    }

    #[test]
    fn registry_ids_are_unique() {
        let mut ids: Vec<&str> = context_actions().iter().map(|a| a.id()).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "duplicate action id in the registry");
    }
}
