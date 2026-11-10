//! Alignment snapping for wire waypoints.
//!
//! Wire waypoints are deliberately off the hole lattice, which makes it hard to get a
//! clean orthogonal route by hand: a waypoint that looks level with an earlier one is
//! usually a fraction of a pitch out. This nudges the cursor onto the same X or Y as a
//! nearby reference point and reports a guide so the canvas can show why.
//!
//! The module is pure — no egui, no app state — so the behaviour can be tested directly
//! rather than inferred from what gets drawn.

use crate::model::WorldPos;

/// The axis a snap locks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapAxis {
    /// Shares an X coordinate with the reference; the guide line is vertical.
    Vertical,
    /// Shares a Y coordinate with the reference; the guide line is horizontal.
    Horizontal,
}

/// One alignment that fired, and the point that caused it.
///
/// Carries the reference point rather than only the axis so the canvas can draw the guide
/// all the way back to what it aligned with, which is the part that makes the feedback
/// readable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapGuide {
    pub axis: SnapAxis,
    /// The point that was aligned to
    pub reference: WorldPos,
    /// Where the cursor ended up
    pub snapped: WorldPos,
}

/// Outcome of resolving a cursor position.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapResult {
    pub pos: WorldPos,
    /// Up to two guides: one per axis. Empty when nothing was near enough.
    pub guides: Vec<SnapGuide>,
}

impl SnapResult {
    pub fn unsnapped(pos: WorldPos) -> Self {
        Self {
            pos,
            guides: Vec::new(),
        }
    }

    pub fn is_snapped(&self) -> bool {
        !self.guides.is_empty()
    }
}

/// Snap a cursor onto the axes of nearby reference points.
///
/// Each axis is resolved independently against the *nearest* candidate on that axis, so a
/// cursor can lock to one point's column and a different point's row at the same time —
/// which is exactly what is wanted when squaring off a corner between two existing wires.
///
/// `tolerance` is in world units; callers derive it from a pixel distance so the feel does
/// not change with zoom.
pub fn resolve(cursor: WorldPos, candidates: &[WorldPos], tolerance: f32) -> SnapResult {
    if tolerance <= 0.0 || candidates.is_empty() {
        return SnapResult::unsnapped(cursor);
    }

    let nearest = |axis: SnapAxis| -> Option<WorldPos> {
        candidates
            .iter()
            .filter(|c| delta(cursor, **c, axis).abs() <= tolerance)
            .min_by(|a, b| {
                // Ties are possible (two points sharing a column); ordering by the other
                // axis too keeps the choice stable rather than dependent on candidate
                // order, which would make the guide flicker between equals.
                let (da, db) = (
                    delta(cursor, **a, axis).abs(),
                    delta(cursor, **b, axis).abs(),
                );
                da.total_cmp(&db)
                    .then_with(|| cross(cursor, **a, axis).total_cmp(&cross(cursor, **b, axis)))
            })
            .copied()
    };

    let v = nearest(SnapAxis::Vertical);
    let h = nearest(SnapAxis::Horizontal);

    let pos = WorldPos::new(
        v.map(|p| p.x).unwrap_or(cursor.x),
        h.map(|p| p.y).unwrap_or(cursor.y),
    );

    let mut guides = Vec::new();
    if let Some(reference) = v {
        guides.push(SnapGuide {
            axis: SnapAxis::Vertical,
            reference,
            snapped: pos,
        });
    }
    if let Some(reference) = h {
        guides.push(SnapGuide {
            axis: SnapAxis::Horizontal,
            reference,
            snapped: pos,
        });
    }

    SnapResult { pos, guides }
}

/// Distance along the axis being locked.
fn delta(cursor: WorldPos, candidate: WorldPos, axis: SnapAxis) -> f32 {
    match axis {
        SnapAxis::Vertical => candidate.x - cursor.x,
        SnapAxis::Horizontal => candidate.y - cursor.y,
    }
}

/// Distance along the other axis, used only to break ties deterministically.
fn cross(cursor: WorldPos, candidate: WorldPos, axis: SnapAxis) -> f32 {
    match axis {
        SnapAxis::Vertical => (candidate.y - cursor.y).abs(),
        SnapAxis::Horizontal => (candidate.x - cursor.x).abs(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: f32 = 0.25;

    #[test]
    fn nothing_in_range_leaves_the_cursor_alone() {
        let cursor = WorldPos::new(5.0, 5.0);
        let r = resolve(cursor, &[WorldPos::new(9.0, 9.0)], TOL);
        assert_eq!(r.pos, cursor);
        assert!(!r.is_snapped());
    }

    #[test]
    fn a_nearby_column_locks_x_and_leaves_y_free() {
        let cursor = WorldPos::new(5.1, 7.3);
        let r = resolve(cursor, &[WorldPos::new(5.0, 2.0)], TOL);
        assert_eq!(r.pos, WorldPos::new(5.0, 7.3), "x snaps, y must not");
        assert_eq!(r.guides.len(), 1);
        assert_eq!(r.guides[0].axis, SnapAxis::Vertical);
        assert_eq!(r.guides[0].reference, WorldPos::new(5.0, 2.0));
    }

    #[test]
    fn a_nearby_row_locks_y_and_leaves_x_free() {
        let cursor = WorldPos::new(5.1, 7.3);
        let r = resolve(cursor, &[WorldPos::new(1.0, 7.2)], TOL);
        assert_eq!(r.pos, WorldPos::new(5.1, 7.2));
        assert_eq!(r.guides.len(), 1);
        assert_eq!(r.guides[0].axis, SnapAxis::Horizontal);
    }

    /// Squaring a corner between two different wires is the case this feature exists for.
    #[test]
    fn two_different_points_can_lock_the_two_axes_at_once() {
        let cursor = WorldPos::new(5.1, 7.1);
        let candidates = [WorldPos::new(5.0, 0.0), WorldPos::new(0.0, 7.0)];
        let r = resolve(cursor, &candidates, TOL);
        assert_eq!(r.pos, WorldPos::new(5.0, 7.0));
        assert_eq!(r.guides.len(), 2);
        assert_eq!(r.guides[0].reference, candidates[0]);
        assert_eq!(r.guides[1].reference, candidates[1]);
        // Both guides must report the final position, not the pre-snap cursor, or the
        // drawn lines would not meet it.
        assert!(r.guides.iter().all(|g| g.snapped == r.pos));
    }

    #[test]
    fn the_nearest_candidate_on_an_axis_wins() {
        let cursor = WorldPos::new(5.0, 0.0);
        let candidates = [WorldPos::new(5.2, 1.0), WorldPos::new(4.95, 1.0)];
        let r = resolve(cursor, &candidates, TOL);
        assert_eq!(r.pos.x, 4.95);
    }

    /// Two candidates equidistant on the locked axis must resolve the same way every
    /// frame, or the guide line jitters between them while the cursor sits still.
    #[test]
    fn ties_are_broken_deterministically_regardless_of_candidate_order() {
        let cursor = WorldPos::new(5.0, 5.0);
        let a = WorldPos::new(5.1, 5.5);
        let b = WorldPos::new(4.9, 9.0);
        let forward = resolve(cursor, &[a, b], TOL);
        let backward = resolve(cursor, &[b, a], TOL);
        assert_eq!(forward, backward);
        assert_eq!(
            forward.guides[0].reference, a,
            "the closer point on the free axis should win the tie"
        );
    }

    #[test]
    fn exactly_at_the_tolerance_still_snaps_and_beyond_it_does_not() {
        let cursor = WorldPos::new(0.0, 0.0);
        assert!(resolve(cursor, &[WorldPos::new(TOL, 9.0)], TOL).is_snapped());
        assert!(!resolve(cursor, &[WorldPos::new(TOL * 1.01, 9.0)], TOL).is_snapped());
    }

    #[test]
    fn a_zero_tolerance_disables_snapping_entirely() {
        let cursor = WorldPos::new(5.0, 5.0);
        let r = resolve(cursor, &[WorldPos::new(5.0, 1.0)], 0.0);
        assert_eq!(r.pos, cursor);
        assert!(!r.is_snapped());
    }

    #[test]
    fn snapping_is_idempotent() {
        let cursor = WorldPos::new(5.1, 7.1);
        let candidates = [WorldPos::new(5.0, 0.0), WorldPos::new(0.0, 7.0)];
        let once = resolve(cursor, &candidates, TOL);
        let twice = resolve(once.pos, &candidates, TOL);
        assert_eq!(once.pos, twice.pos, "re-resolving must not drift");
    }
}
