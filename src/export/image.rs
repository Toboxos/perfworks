//! PNG export of the component and solder sides of a board.
//!
//! The output covers **exactly** the board's substrate rectangle, so the image maps 1:1
//! onto the top face of the same board in [`crate::export::step`]. That is what makes it
//! usable as a CAD decal: drop the STEP in, apply the PNG to the top face at the board's
//! own width and height, and the artwork lands where it belongs with no scaling guesswork.
//! STEP itself has no texture concept, which is why the artwork has to travel separately.
//!
//! The module is split so that the part worth testing can be tested. [`ImagePlan`] does
//! all the coordinate work and is pure; `render_png` is a thin wrapper around an offscreen
//! framebuffer and cannot run without a GL context.

use egui::Rect;

use crate::canvas::Viewport;
use crate::model::{Perfboard, PITCH_MM};

/// Which side of the board to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardSide {
    /// Component side, as seen from above
    Front,
    /// Solder side, mirrored left-to-right as it would appear looking at the back
    Back,
}

impl BoardSide {
    pub const ALL: [BoardSide; 2] = [BoardSide::Front, BoardSide::Back];

    pub fn suffix(self) -> &'static str {
        match self {
            BoardSide::Front => "front",
            BoardSide::Back => "back",
        }
    }

    fn flipped(self) -> bool {
        self == BoardSide::Back
    }
}

/// Everything needed to render one side of one board at a chosen resolution.
///
/// Built by [`ImagePlan::new`], which is the single place the board's millimetre extents
/// are turned into pixels and into a camera. Keeping that pure is what lets the 1:1
/// mapping be asserted without a window.
#[derive(Debug, Clone)]
pub struct ImagePlan {
    pub width_px: u32,
    pub height_px: u32,
    pub viewport: Viewport,
    /// Target rect in the offscreen surface; always anchored at the origin.
    pub canvas_rect: Rect,
    /// Resolution the plan was built at, carried through for the file metadata.
    pub px_per_mm: f32,
    /// Board corner radius in pixels, used to knock the corners out of the alpha channel.
    pub corner_radius_px: f32,
}

/// Resolution bounds. The lower bound keeps a plan from collapsing to a degenerate
/// surface; the upper one keeps a large board from asking for a texture the driver will
/// refuse to allocate.
pub const MIN_PX_PER_MM: f32 = 2.0;
pub const MAX_PX_PER_MM: f32 = 40.0;

/// A sensible default: 20 px/mm is ~508 dpi, which keeps 2.54mm pads crisp as a decal
/// without producing an unreasonable texture for an 80mm board.
pub const DEFAULT_PX_PER_MM: f32 = 20.0;

impl ImagePlan {
    pub fn new(board: &Perfboard, side: BoardSide, px_per_mm: f32) -> Self {
        let px_per_mm = px_per_mm.clamp(MIN_PX_PER_MM, MAX_PX_PER_MM);
        let (w_mm, h_mm) = board.size_mm();

        let width_px = (w_mm * px_per_mm).round().max(1.0) as u32;
        let height_px = (h_mm * px_per_mm).round().max(1.0) as u32;
        let canvas_rect = Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width_px as f32, height_px as f32),
        );

        // Centre the camera on the board and scale so one pitch is exactly
        // `px_per_mm * 2.54` pixels. The board's own bounds then land on the surface
        // edges, because world_to_screen measures outward from the canvas centre.
        let (min, max) = board.world_bounds();
        let mut viewport = Viewport {
            center: crate::model::WorldPos::new((min.x + max.x) * 0.5, (min.y + max.y) * 0.5),
            pixels_per_pitch: px_per_mm * PITCH_MM,
            flipped: side.flipped(),
        };
        // `fit_bounds` is deliberately not used: it adds margin and rounds the zoom to fit,
        // which is right for the screen and wrong for a decal that has to be exact.
        viewport.flipped = side.flipped();

        Self {
            width_px,
            height_px,
            viewport,
            canvas_rect,
            px_per_mm,
            corner_radius_px: board.outline.corner_radius_mm * px_per_mm,
        }
    }

    /// Total pixels, for sanity-checking an allocation before asking the driver for it.
    pub fn pixel_count(&self) -> u64 {
        self.width_px as u64 * self.height_px as u64
    }
}

/// Make everything outside the board's rounded outline fully transparent.
///
/// The image rectangle *is* the board rectangle, so the only pixels outside the board are
/// the four corner arcs — and those come back opaque, carrying the editor's dark canvas
/// colour. Left alone they put dark triangles on the corners of the decal, right where the
/// mounting holes are. Operates on RGBA, non-premultiplied, as PNG stores it.
pub fn clear_outside_rounded_rect(pixels: &mut [u8], width: u32, height: u32, radius_px: f32) {
    if radius_px <= 0.0 {
        return;
    }
    let r = radius_px.min(width as f32 * 0.5).min(height as f32 * 0.5);
    let r2 = r * r;
    let corners = [
        (r, r),
        (width as f32 - r, r),
        (r, height as f32 - r),
        (width as f32 - r, height as f32 - r),
    ];

    for y in 0..height {
        for x in 0..width {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // Only the corner squares can lie outside; everything else is board.
            let in_corner_x = px < r || px > width as f32 - r;
            let in_corner_y = py < r || py > height as f32 - r;
            if !(in_corner_x && in_corner_y) {
                continue;
            }
            let outside = corners
                .iter()
                .all(|(cx, cy)| (px - cx).powi(2) + (py - cy).powi(2) > r2);
            if outside {
                let i = ((y * width + x) * 4 + 3) as usize;
                pixels[i] = 0;
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{render_png, RenderError};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::sync::Arc;

    use egui::Context;
    use glow::HasContext as _;

    use super::ImagePlan;
    use crate::canvas::{render_workspace, CanvasRenderPreview};
    use crate::model::Project;

    #[derive(Debug)]
    pub enum RenderError {
        /// eframe was built without a GL context, or the app was constructed headlessly.
        NoGlContext,
        /// The driver refused the offscreen target.
        Framebuffer(String),
        Encode(String),
    }

    impl std::fmt::Display for RenderError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                RenderError::NoGlContext => write!(f, "no OpenGL context available"),
                RenderError::Framebuffer(m) => write!(f, "offscreen target failed: {m}"),
                RenderError::Encode(m) => write!(f, "PNG encoding failed: {m}"),
            }
        }
    }

    /// Render one plan to PNG bytes.
    ///
    /// Runs a throwaway [`Context`] rather than borrowing the live one: the live context is
    /// mid-frame while this is called from a menu handler, and re-entering it would corrupt
    /// its pass state. The cost is one font atlas build per export, which is invisible next
    /// to the file dialog that preceded it.
    pub fn render_png(
        gl: &Arc<glow::Context>,
        plan: &ImagePlan,
        project: &Project,
    ) -> Result<Vec<u8>, RenderError> {
        let ctx = Context::default();
        let rect = plan.canvas_rect;
        let viewport = plan.viewport.clone();

        let mut raw = egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        };
        raw.viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .inner_rect = Some(rect);

        let output = ctx.run(raw, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let painter = ui.painter_at(rect);
                    // No selection and no preview state: an exported image is the board as
                    // built, not the editor's current view of it. Highlights, ghosts and
                    // hover rings would all be noise on a decal.
                    let preview = CanvasRenderPreview {
                        ghost_board: None,
                        ghost_connectors: &[],
                        active_wire_in_progress: None,
                        active_solder_in_progress: None,
                        hovered_hole: None,
                        ghost_components: false,
                        show_x_ray_blocked_holes: false,
                        show_pin_labels: true,
                        show_net_highlights: false,
                        highlighted_net: None,
                        highlight_pinned: false,
                        focus_net: false,
                        conflict_nets: &[],
                    };
                    render_workspace(&painter, rect, &viewport, project, &[], &preview);
                });
        });

        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);

        unsafe {
            let prev_fbo = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);

            let texture = gl
                .create_texture()
                .map_err(|e| RenderError::Framebuffer(e.to_string()))?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::SRGB8_ALPHA8 as i32,
                plan.width_px as i32,
                plan.height_px as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );

            let fbo = gl
                .create_framebuffer()
                .map_err(|e| RenderError::Framebuffer(e.to_string()))?;
            // Bind BOTH targets. Binding only DRAW leaves READ_FRAMEBUFFER on the window's
            // back buffer, and read_pixels then silently returns the window contents rather
            // than what was just rendered -- which looks exactly like a renderer that drew
            // nothing at all.
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );

            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            if status != glow::FRAMEBUFFER_COMPLETE {
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                gl.delete_framebuffer(fbo);
                gl.delete_texture(texture);
                return Err(RenderError::Framebuffer(format!("status 0x{status:x}")));
            }

            gl.viewport(0, 0, plan.width_px as i32, plan.height_px as i32);
            gl.disable(glow::SCISSOR_TEST);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);

            let mut painter = egui_glow::Painter::new(gl.clone(), "", None, false)
                .map_err(|e| RenderError::Framebuffer(e.to_string()))?;
            painter.paint_and_update_textures(
                [plan.width_px, plan.height_px],
                output.pixels_per_point,
                &primitives,
                &output.textures_delta,
            );

            let mut pixels = vec![0u8; (plan.pixel_count() * 4) as usize];
            gl.read_buffer(glow::COLOR_ATTACHMENT0);
            gl.read_pixels(
                0,
                0,
                plan.width_px as i32,
                plan.height_px as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );

            painter.destroy();
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.delete_framebuffer(fbo);
            gl.delete_texture(texture);
            // Hand the context back exactly as it was found; eframe paints the real frame
            // into this same context immediately after update returns.
            if prev_fbo != 0 {
                gl.bind_framebuffer(
                    glow::FRAMEBUFFER,
                    Some(glow::NativeFramebuffer(
                        std::num::NonZeroU32::new(prev_fbo as u32).unwrap(),
                    )),
                );
            }

            // GL reads bottom-up; PNG is top-down.
            flip_rows_in_place(&mut pixels, plan.width_px as usize, plan.height_px as usize);
            super::clear_outside_rounded_rect(
                &mut pixels,
                plan.width_px,
                plan.height_px,
                plan.corner_radius_px,
            );
            encode_png(&pixels, plan)
        }
    }

    fn flip_rows_in_place(pixels: &mut [u8], width: usize, height: usize) {
        let stride = width * 4;
        for y in 0..height / 2 {
            let (top, bottom) = pixels.split_at_mut((y + 1) * stride);
            let top_row = &mut top[y * stride..];
            let bottom_row = &mut bottom[(height - 2 * y - 2) * stride..][..stride];
            top_row[..stride].swap_with_slice(bottom_row);
        }
    }

    fn encode_png(pixels: &[u8], plan: &ImagePlan) -> Result<Vec<u8>, RenderError> {
        let buffer = image::RgbaImage::from_raw(plan.width_px, plan.height_px, pixels.to_vec())
            .ok_or_else(|| RenderError::Encode("pixel buffer size mismatch".into()))?;
        let mut out = Vec::new();
        buffer
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .map_err(|e| RenderError::Encode(e.to_string()))?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GridPos, WorldPos};

    fn board_5x7() -> Perfboard {
        let preset = crate::library::get_board_preset_by_id("board_perf_24x18").unwrap();
        let mut b = Perfboard::new("B", GridPos::new(3, 7), preset.cols, preset.rows);
        b.outline = preset.outline;
        b.mounting = preset.mounting;
        b
    }

    /// The whole point of the export: the image is the board, at a known scale. If this
    /// drifts, every decal applied from it is silently the wrong size.
    #[test]
    fn image_covers_exactly_the_board_outline_at_the_requested_scale() {
        let board = board_5x7();
        let (w_mm, h_mm) = board.size_mm();
        let plan = ImagePlan::new(&board, BoardSide::Front, 20.0);

        assert_eq!(plan.width_px, (w_mm * 20.0).round() as u32);
        assert_eq!(plan.height_px, (h_mm * 20.0).round() as u32);

        // The board's own world bounds must land on the surface corners.
        let (min, max) = board.world_bounds();
        let p_min = plan.viewport.world_to_screen(plan.canvas_rect, min);
        let p_max = plan.viewport.world_to_screen(plan.canvas_rect, max);
        assert!(p_min.x.abs() < 0.5, "left edge at {}", p_min.x);
        assert!(p_min.y.abs() < 0.5, "top edge at {}", p_min.y);
        assert!(
            (p_max.x - plan.width_px as f32).abs() < 0.5,
            "right edge at {} not {}",
            p_max.x,
            plan.width_px
        );
        assert!(
            (p_max.y - plan.height_px as f32).abs() < 0.5,
            "bottom edge at {} not {}",
            p_max.y,
            plan.height_px
        );
    }

    #[test]
    fn a_hole_lands_at_its_millimetre_position_in_the_image() {
        let board = board_5x7();
        let plan = ImagePlan::new(&board, BoardSide::Front, 20.0);

        // Pick an interior hole and check it against hole_to_local_mm, which is the same
        // frame the STEP export uses. Agreement between the two is what makes the decal
        // line up with the solid.
        for local in [
            GridPos::new(0, 0),
            GridPos::new(12, 9),
            GridPos::new(23, 17),
        ] {
            let global = board.local_to_global(local);
            let (mx, my) = board.hole_to_local_mm(global);
            let p = plan.viewport.grid_to_screen(plan.canvas_rect, global);
            assert!(
                (p.x - mx * plan.px_per_mm).abs() < 0.5,
                "hole {local:?} at x={} px, expected {}",
                p.x,
                mx * plan.px_per_mm
            );
            assert!(
                (p.y - my * plan.px_per_mm).abs() < 0.5,
                "hole {local:?} at y={} px, expected {}",
                p.y,
                my * plan.px_per_mm
            );
        }
    }

    /// The back view must be mirrored, not merely re-rendered, or it is the front view
    /// with a different file name.
    #[test]
    fn the_back_side_is_mirrored_and_the_same_size() {
        let board = board_5x7();
        let front = ImagePlan::new(&board, BoardSide::Front, 10.0);
        let back = ImagePlan::new(&board, BoardSide::Back, 10.0);

        assert_eq!(
            (front.width_px, front.height_px),
            (back.width_px, back.height_px)
        );
        assert!(!front.viewport.flipped);
        assert!(back.viewport.flipped);

        let left_hole = board.local_to_global(GridPos::new(0, 0));
        let fx = front
            .viewport
            .grid_to_screen(front.canvas_rect, left_hole)
            .x;
        let bx = back.viewport.grid_to_screen(back.canvas_rect, left_hole).x;
        assert!(
            (fx + bx - front.width_px as f32).abs() < 0.5,
            "mirrored x should sum to the image width: {fx} + {bx} != {}",
            front.width_px
        );

        // Y is untouched by the flip; only X mirrors.
        let fy = front
            .viewport
            .grid_to_screen(front.canvas_rect, left_hole)
            .y;
        let by = back.viewport.grid_to_screen(back.canvas_rect, left_hole).y;
        assert!((fy - by).abs() < 1e-3);
    }

    #[test]
    fn resolution_is_clamped_to_a_surface_a_driver_will_allocate() {
        let board = board_5x7();
        for requested in [0.0_f32, -5.0, 1.0, 1000.0, f32::INFINITY] {
            let plan = ImagePlan::new(&board, BoardSide::Front, requested);
            assert!(
                plan.px_per_mm >= MIN_PX_PER_MM && plan.px_per_mm <= MAX_PX_PER_MM,
                "{requested} produced {}",
                plan.px_per_mm
            );
            assert!(plan.width_px >= 1 && plan.height_px >= 1);
            assert!(
                plan.pixel_count() < 100_000_000,
                "{requested} asks for {} pixels",
                plan.pixel_count()
            );
        }
    }

    #[test]
    fn a_boards_position_on_the_global_grid_does_not_change_its_image() {
        let a = board_5x7();
        let mut b = board_5x7();
        b.origin = GridPos::new(-40, 120);

        let pa = ImagePlan::new(&a, BoardSide::Front, 12.0);
        let pb = ImagePlan::new(&b, BoardSide::Front, 12.0);
        assert_eq!((pa.width_px, pa.height_px), (pb.width_px, pb.height_px));

        // Same hole, same pixel — the camera follows the board.
        let ha = pa
            .viewport
            .grid_to_screen(pa.canvas_rect, a.local_to_global(GridPos::new(5, 5)));
        let hb = pb
            .viewport
            .grid_to_screen(pb.canvas_rect, b.local_to_global(GridPos::new(5, 5)));
        assert!((ha.x - hb.x).abs() < 0.01 && (ha.y - hb.y).abs() < 0.01);
    }

    #[test]
    fn corner_mask_clears_only_outside_the_rounded_outline() {
        let (w, h, r) = (40u32, 30u32, 8.0_f32);
        let mut px = vec![255u8; (w * h * 4) as usize];
        clear_outside_rounded_rect(&mut px, w, h, r);

        let alpha = |x: u32, y: u32| px[((y * w + x) * 4 + 3) as usize];

        // The four extreme corners are outside any circle of radius r centred r in.
        for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
            assert_eq!(alpha(x, y), 0, "corner ({x},{y}) should be transparent");
        }
        // Mid-edges and the centre are board and must be untouched.
        for (x, y) in [(w / 2, 0), (0, h / 2), (w / 2, h / 2), (w - 1, h / 2)] {
            assert_eq!(alpha(x, y), 255, "({x},{y}) is board, not corner");
        }
        // A point just inside the corner arc stays opaque.
        assert_eq!(alpha(4, 4), 255, "inside the corner radius");
        // Colour channels are never touched, only alpha.
        assert!(px
            .chunks(4)
            .all(|p| p[0] == 255 && p[1] == 255 && p[2] == 255));
    }

    #[test]
    fn corner_mask_is_a_no_op_for_a_square_cornered_board() {
        let mut px = vec![200u8; 10 * 10 * 4];
        let before = px.clone();
        clear_outside_rounded_rect(&mut px, 10, 10, 0.0);
        assert_eq!(px, before);
    }

    #[test]
    fn plan_centres_on_the_board_not_the_world_origin() {
        let board = board_5x7();
        let plan = ImagePlan::new(&board, BoardSide::Front, 10.0);
        let (min, max) = board.world_bounds();
        let expected = WorldPos::new((min.x + max.x) * 0.5, (min.y + max.y) * 0.5);
        assert!((plan.viewport.center.x - expected.x).abs() < 1e-4);
        assert!((plan.viewport.center.y - expected.y).abs() < 1e-4);
    }
}
