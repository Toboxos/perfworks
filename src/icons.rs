use egui::{Color32, CornerRadius, FontId, Painter, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

/// Vector icons painted directly with `egui::Painter` so they never depend on missing system/emoji glyphs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CadIcon {
    Add,
    Select,
    Move,
    Pin,
    Header,
    Wire,
    Solder,
    CutTrack,
    Erase,
    Ghost,
    FlipView,
    Rotate,
}

/// Renders an Autodesk Fusion-style floating toolbar button with a crisp vector icon above a text caption
pub fn toolbar_icon_button(
    ui: &mut Ui,
    icon: CadIcon,
    caption: &str,
    active: bool,
    accent: bool,
) -> Response {
    let desired_size = Vec2::new(56.0, 46.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let bg_fill = if active {
            Color32::from_rgb(42, 118, 196)
        } else if accent {
            if hovered {
                Color32::from_rgb(48, 136, 222)
            } else {
                Color32::from_rgb(36, 104, 176)
            }
        } else if hovered {
            Color32::from_rgb(44, 50, 62)
        } else {
            Color32::TRANSPARENT
        };

        if bg_fill != Color32::TRANSPARENT {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(7), bg_fill);
        }

        if active {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(7),
                Stroke::new(1.2_f32, Color32::from_rgb(110, 195, 255)),
                egui::StrokeKind::Inside,
            );
        }

        let fg_color = if active || accent || hovered {
            Color32::WHITE
        } else {
            Color32::from_rgb(206, 214, 228)
        };

        let icon_center = Pos2::new(rect.center().x, rect.min.y + 15.5);
        paint_cad_icon(ui.painter(), icon, icon_center, 16.0, fg_color);

        let text_pos = Pos2::new(rect.center().x, rect.max.y - 8.5);
        ui.painter().text(
            text_pos,
            egui::Align2::CENTER_CENTER,
            caption,
            FontId::proportional(10.5),
            fg_color,
        );
    }

    response
}

/// Paint a crisp 2D vector icon centered at `center` with characteristic `size`
pub fn paint_cad_icon(painter: &Painter, icon: CadIcon, center: Pos2, size: f32, color: Color32) {
    let r = size * 0.5;
    let stroke = Stroke::new(1.8_f32, color);
    let thin_stroke = Stroke::new(1.4_f32, color);

    match icon {
        CadIcon::Add => {
            // Plus sign inside a subtle rounded frame
            let box_r = Rect::from_center_size(center, Vec2::splat(size * 1.05));
            painter.rect_stroke(
                box_r,
                CornerRadius::same(3),
                thin_stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x - r * 0.65, center.y),
                    Pos2::new(center.x + r * 0.65, center.y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x, center.y - r * 0.65),
                    Pos2::new(center.x, center.y + r * 0.65),
                ],
                stroke,
            );
        }

        CadIcon::Select => {
            // Crisp CAD cursor arrow polygon
            let tip = Pos2::new(center.x - r * 0.55, center.y - r * 0.85);
            let p_left = Pos2::new(center.x - r * 0.55, center.y + r * 0.72);
            let p_notch = Pos2::new(center.x - r * 0.10, center.y + r * 0.35);
            let p_right = Pos2::new(center.x + r * 0.62, center.y + r * 0.25);
            painter.add(egui::Shape::convex_polygon(
                vec![tip, p_left, p_notch, p_right],
                color,
                Stroke::new(1.0_f32, color),
            ));
            // Tail stem
            painter.line_segment(
                [
                    Pos2::new(center.x - r * 0.12, center.y + r * 0.28),
                    Pos2::new(center.x + r * 0.22, center.y + r * 0.88),
                ],
                Stroke::new(2.0_f32, color),
            );
        }

        CadIcon::Move => {
            // 4-way move crosshair arrows
            let span = r * 0.88;
            let head = r * 0.32;
            painter.line_segment(
                [
                    Pos2::new(center.x - span, center.y),
                    Pos2::new(center.x + span, center.y),
                ],
                thin_stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x, center.y - span),
                    Pos2::new(center.x, center.y + span),
                ],
                thin_stroke,
            );
            // Arrowheads
            // Left
            painter.line_segment(
                [
                    Pos2::new(center.x - span, center.y),
                    Pos2::new(center.x - span + head, center.y - head),
                ],
                thin_stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x - span, center.y),
                    Pos2::new(center.x - span + head, center.y + head),
                ],
                thin_stroke,
            );
            // Right
            painter.line_segment(
                [
                    Pos2::new(center.x + span, center.y),
                    Pos2::new(center.x + span - head, center.y - head),
                ],
                thin_stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x + span, center.y),
                    Pos2::new(center.x + span - head, center.y + head),
                ],
                thin_stroke,
            );
            // Up
            painter.line_segment(
                [
                    Pos2::new(center.x, center.y - span),
                    Pos2::new(center.x - head, center.y - span + head),
                ],
                thin_stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x, center.y - span),
                    Pos2::new(center.x + head, center.y - span + head),
                ],
                thin_stroke,
            );
            // Down
            painter.line_segment(
                [
                    Pos2::new(center.x, center.y + span),
                    Pos2::new(center.x - head, center.y + span - head),
                ],
                thin_stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x, center.y + span),
                    Pos2::new(center.x + head, center.y + span - head),
                ],
                thin_stroke,
            );
        }

        CadIcon::Pin => {
            // Copper pad ring with square gold pin post inside
            painter.circle_stroke(center, r * 0.80, stroke);
            let inner = Rect::from_center_size(center, Vec2::splat(r * 0.68));
            painter.rect_filled(
                inner,
                CornerRadius::same(1),
                Color32::from_rgb(240, 200, 82),
            );
        }

        CadIcon::Header => {
            // 3-pin header housing with 3 gold posts
            let body = Rect::from_center_size(center, Vec2::new(size * 1.15, size * 0.62));
            painter.rect_stroke(
                body,
                CornerRadius::same(2),
                thin_stroke,
                egui::StrokeKind::Inside,
            );
            for dx in [-r * 0.68, 0.0, r * 0.68] {
                let p = Pos2::new(center.x + dx, center.y);
                painter.rect_filled(
                    Rect::from_center_size(p, Vec2::splat(3.2)),
                    CornerRadius::same(1),
                    Color32::from_rgb(240, 200, 82),
                );
            }
        }

        CadIcon::Wire => {
            // S-curve spline wire between two endpoint pads
            let p_start = Pos2::new(center.x - r * 0.78, center.y + r * 0.62);
            let p_mid1 = Pos2::new(center.x - r * 0.15, center.y + r * 0.55);
            let p_mid2 = Pos2::new(center.x + r * 0.15, center.y - r * 0.55);
            let p_end = Pos2::new(center.x + r * 0.78, center.y - r * 0.62);
            painter.add(egui::Shape::line(
                vec![p_start, p_mid1, p_mid2, p_end],
                Stroke::new(2.0_f32, Color32::from_rgb(92, 200, 255)),
            ));
            painter.circle_filled(p_start, 2.6, color);
            painter.circle_filled(p_end, 2.6, color);
        }

        CadIcon::Solder => {
            // Pill-shaped metallic solder bridge joining two adjacent pads
            let p1 = Pos2::new(center.x - r * 0.52, center.y);
            let p2 = Pos2::new(center.x + r * 0.52, center.y);
            painter.line_segment([p1, p2], Stroke::new(5.5_f32, color));
            painter.circle_filled(p1, 3.6, color);
            painter.circle_filled(p2, 3.6, color);
            painter.circle_filled(p1, 1.4, Color32::from_rgb(30, 34, 42));
            painter.circle_filled(p2, 1.4, Color32::from_rgb(30, 34, 42));
        }

        CadIcon::CutTrack => {
            // Circle with diagonal X track cut
            painter.circle_stroke(center, r * 0.82, thin_stroke);
            let d = r * 0.52;
            let red = Color32::from_rgb(250, 100, 100);
            painter.line_segment(
                [
                    Pos2::new(center.x - d, center.y - d),
                    Pos2::new(center.x + d, center.y + d),
                ],
                Stroke::new(1.8_f32, red),
            );
            painter.line_segment(
                [
                    Pos2::new(center.x + d, center.y - d),
                    Pos2::new(center.x - d, center.y + d),
                ],
                Stroke::new(1.8_f32, red),
            );
        }

        CadIcon::Erase => {
            // Clean trash/eraser diagonal X inside rounded badge
            let d = r * 0.58;
            painter.line_segment(
                [
                    Pos2::new(center.x - d, center.y - d),
                    Pos2::new(center.x + d, center.y + d),
                ],
                Stroke::new(2.0_f32, Color32::from_rgb(250, 115, 115)),
            );
            painter.line_segment(
                [
                    Pos2::new(center.x + d, center.y - d),
                    Pos2::new(center.x - d, center.y + d),
                ],
                Stroke::new(2.0_f32, Color32::from_rgb(250, 115, 115)),
            );
        }

        CadIcon::Ghost => {
            // Eye / X-Ray outline (almond curves + pupil circle)
            let left = Pos2::new(center.x - r * 0.88, center.y);
            let right = Pos2::new(center.x + r * 0.88, center.y);
            let top = Pos2::new(center.x, center.y - r * 0.56);
            let bottom = Pos2::new(center.x, center.y + r * 0.56);
            painter.add(egui::Shape::line(
                vec![left, top, right, bottom, left],
                thin_stroke,
            ));
            painter.circle_filled(center, r * 0.28, color);
        }

        CadIcon::FlipView => {
            // Two overlapping board rectangles with horizontal flip arrows
            let back = Rect::from_center_size(
                Pos2::new(center.x - 2.0, center.y - 1.5),
                Vec2::new(size * 0.72, size * 0.55),
            );
            let front = Rect::from_center_size(
                Pos2::new(center.x + 2.5, center.y + 2.0),
                Vec2::new(size * 0.72, size * 0.55),
            );
            painter.rect_stroke(
                back,
                CornerRadius::same(2),
                Stroke::new(1.2_f32, color.gamma_multiply(0.65)),
                egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                front,
                CornerRadius::same(2),
                thin_stroke,
                egui::StrokeKind::Inside,
            );
        }

        CadIcon::Rotate => {
            // Circular arc with arrowhead
            painter.circle_stroke(center, r * 0.68, thin_stroke);
            painter.line_segment(
                [
                    Pos2::new(center.x + r * 0.68, center.y - 2.0),
                    Pos2::new(center.x + r * 0.38, center.y - 5.0),
                ],
                thin_stroke,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_paints_geometry_without_font_glyphs() {
        for icon in [
            CadIcon::Add,
            CadIcon::Select,
            CadIcon::Move,
            CadIcon::Pin,
            CadIcon::Header,
            CadIcon::Wire,
            CadIcon::Solder,
            CadIcon::CutTrack,
            CadIcon::Erase,
            CadIcon::Ghost,
            CadIcon::FlipView,
            CadIcon::Rotate,
        ] {
            let ctx = egui::Context::default();
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                let painter = ctx.layer_painter(egui::LayerId::background());
                paint_cad_icon(&painter, icon, egui::pos2(40.0, 40.0), 16.0, Color32::WHITE);
            });
            assert!(!output.shapes.is_empty(), "{icon:?} has no geometry");
            assert!(
                output
                    .shapes
                    .iter()
                    .all(|s| !matches!(s.shape, egui::Shape::Text(_))),
                "{icon:?} relies on a font"
            );
            assert!(!ctx
                .tessellate(output.shapes, output.pixels_per_point)
                .is_empty());
        }
    }
}
