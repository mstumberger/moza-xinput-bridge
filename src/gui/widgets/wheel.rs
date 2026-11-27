//! Drawn steering wheel (spec section 7). Pure painter drawing; text is never
//! rotated, everything else is rotated about the centre by the steering angle.

use eframe::egui::{self, Align2, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2};

use super::wheel_layout::{has_spot, ButtonSpot, SpotKind, ES_WHEEL_LAYOUT};
use crate::gui::theme;

/// Height reserved under the wheel for the chip row (spec 7.5).
pub const CHIP_ROW_HEIGHT: f32 = 24.0;

pub struct WheelView<'a> {
    pub steer: f64,
    pub steering_range_deg: f32,
    pub buttons: &'a [bool; 64],
    pub labels: &'a [Option<&'a str>; 64],
    pub led_mask: Option<u16>,
    pub led_count: u8,
}

/// Wheel-space point (x right, y down, rim edge at 1.0) to screen. A positive
/// angle turns the drawing clockwise on screen.
pub fn to_screen(p: Vec2, angle_rad: f32, center: Pos2, r: f32) -> Pos2 {
    let (s, c) = angle_rad.sin_cos();
    let x = p.x * c - p.y * s;
    let y = p.x * s + p.y * c;
    center + Vec2::new(x, y) * r
}

/// Colour of LED `i` of `count` when lit: first 40 % accent, next 40 % warn,
/// last 20 % bad (spec 7.6).
pub fn led_color(i: u8, count: u8) -> egui::Color32 {
    let (i, n) = (i as u32 * 5, count.max(1) as u32);
    if i < 2 * n {
        theme::ACCENT
    } else if i < 4 * n {
        theme::WARN
    } else {
        theme::BAD
    }
}

fn v(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

/// Rounded rectangle as a convex polygon in wheel space.
fn rounded_rect(cx: f32, cy: f32, w: f32, h: f32, rad: f32) -> Vec<Vec2> {
    const STEPS: usize = 5;
    let (hw, hh) = (w / 2.0 - rad, h / 2.0 - rad);
    let mut pts = Vec::with_capacity(4 * (STEPS + 1));
    // Corner centres in clockwise order starting top-right, with start angle.
    let corners = [(hw, -hh, -90.0f32), (hw, hh, 0.0), (-hw, hh, 90.0), (-hw, -hh, 180.0)];
    for (ox, oy, start) in corners {
        for k in 0..=STEPS {
            let a = (start + 90.0 * k as f32 / STEPS as f32).to_radians();
            pts.push(v(cx + ox + rad * a.cos(), cy + oy + rad * a.sin()));
        }
    }
    pts
}

/// Points on a circle of `radius` (wheel space) from `from_deg` to `to_deg`,
/// measured from +x towards +y.
fn arc_pts(radius: f32, from_deg: f32, to_deg: f32) -> Vec<Vec2> {
    const STEPS: usize = 6;
    (0..=STEPS)
        .map(|k| {
            let a = (from_deg + (to_deg - from_deg) * k as f32 / STEPS as f32).to_radians();
            v(radius * a.cos(), radius * a.sin())
        })
        .collect()
}

/// Screen direction of a D-pad arrow label.
fn arrow_dir(label: &str) -> Option<Vec2> {
    match label {
        "\u{25B2}" => Some(v(0.0, -1.0)),
        "\u{25B6}" => Some(v(1.0, 0.0)),
        "\u{25BC}" => Some(v(0.0, 1.0)),
        "\u{25C0}" => Some(v(-1.0, 0.0)),
        _ => None,
    }
}

struct Draw<'p> {
    painter: &'p Painter,
    angle: f32,
    center: Pos2,
    r: f32,
}

impl Draw<'_> {
    fn pt(&self, p: Vec2) -> Pos2 {
        to_screen(p, self.angle, self.center, self.r)
    }

    fn poly(&self, pts: &[Vec2], fill: egui::Color32, stroke: Stroke) {
        let pts = pts.iter().map(|&p| self.pt(p)).collect();
        self.painter.add(Shape::convex_polygon(pts, fill, stroke));
    }
}

impl WheelView<'_> {
    /// Allocate a `side` square plus the chip row below and draw the wheel.
    pub fn show(&self, ui: &mut egui::Ui, side: f32) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(side, side), egui::Sense::hover());
        let (chip_rect, _) =
            ui.allocate_exact_size(Vec2::new(side, CHIP_ROW_HEIGHT), egui::Sense::hover());
        let painter = ui.painter_at(rect.union(chip_rect));
        self.paint(&painter, rect);
        self.paint_chips(&painter, chip_rect);
    }

    fn pressed(&self, n: u8) -> bool {
        self.buttons[n as usize - 1]
    }

    fn paint(&self, painter: &Painter, rect: Rect) {
        let angle =
            (self.steer.clamp(-1.0, 1.0) as f32 * self.steering_range_deg / 2.0).to_radians();
        let r = rect.width() / 2.0;
        let d = Draw { painter, angle, center: rect.center(), r };
        let hub = |fill: egui::Color32| Stroke::new(1.0_f32, fill);

        // Rim: a ring (96-gon, since egui 0.31 circle strokes are not centred)
        // with a dark outer and a light inner edge, and the top marker.
        let circle = |radius: f32| -> Vec<Pos2> {
            (0..96)
                .map(|k| {
                    let a = std::f32::consts::TAU * k as f32 / 96.0;
                    d.center + Vec2::new(a.cos(), a.sin()) * radius * r
                })
                .collect()
        };
        painter.add(Shape::closed_line(circle(0.90), Stroke::new(0.20 * r, theme::WHEEL_RIM)));
        painter.add(Shape::closed_line(circle(0.995), Stroke::new(1.5_f32, theme::WHEEL_RIM_DARK)));
        painter.add(Shape::closed_line(circle(0.805), Stroke::new(1.5_f32, theme::WHEEL_RIM_LIGHT)));
        painter.line_segment(
            [d.pt(v(0.0, -0.99)), d.pt(v(0.0, -0.81))],
            Stroke::new((0.05 * r).max(2.0), theme::TEXT),
        );

        // Hub fills first (spokes overlap the plates and run slightly into the
        // rim, covering its inner edge), outlines afterwards on outer edges
        // only, so no seam shows between touching pieces.
        let spoke = |sign: f32| -> Vec<Vec2> {
            let mut pts = vec![v(sign * 0.50, -0.046)];
            pts.extend(arc_pts(0.806, -7.2, 7.2).into_iter().map(|p| v(sign * p.x, p.y)));
            pts.push(v(sign * 0.50, 0.16));
            pts
        };
        let mut bottom = vec![v(-0.14, 0.58), v(0.14, 0.58)];
        bottom.extend(arc_pts(0.806, 76.4, 103.6));
        let upper = [
            v(-0.56, -0.20),
            v(-0.30, -0.27),
            v(0.30, -0.27),
            v(0.56, -0.20),
            v(0.60, 0.16),
            v(-0.60, 0.16),
        ];
        let lower = [
            v(-0.60, 0.15),
            v(0.60, 0.15),
            v(0.50, 0.36),
            v(0.14, 0.60),
            v(-0.14, 0.60),
            v(-0.50, 0.36),
        ];
        for piece in [&spoke(-1.0)[..], &spoke(1.0), &bottom, &upper, &lower] {
            d.poly(piece, theme::WHEEL_HUB, hub(theme::WHEEL_HUB));
        }
        let edge = Stroke::new(1.0_f32, theme::WHEEL_HUB_EDGE);
        let side = |sign: f32| -> Vec<Vec2> {
            [(0.794, 0.10), (0.598, 0.14), (0.60, 0.16), (0.50, 0.36), (0.145, 0.60), (0.188, 0.777)]
                .iter()
                .map(|&(x, y)| v(sign * x, y))
                .collect()
        };
        let top = [
            v(-0.794, -0.10),
            v(-0.5756, -0.06),
            v(-0.56, -0.20),
            v(-0.30, -0.27),
            v(0.30, -0.27),
            v(0.56, -0.20),
            v(0.5756, -0.06),
            v(0.794, -0.10),
        ];
        for line in [&top[..], &side(-1.0), &side(1.0)] {
            let pts = line.iter().map(|&p| d.pt(p)).collect();
            painter.add(Shape::line(pts, edge));
        }

        // Centre badge: hexagon, no text.
        let hex: Vec<Vec2> = (0..6)
            .map(|k| {
                let a = (60.0 * k as f32 - 90.0).to_radians();
                v(0.12 * a.cos(), 0.02 + 0.12 * a.sin())
            })
            .collect();
        d.poly(&hex, theme::WHEEL_BUTTON, edge);

        self.paint_paddles(&d);
        self.paint_leds(&d);
        for spot in ES_WHEEL_LAYOUT {
            self.paint_spot(&d, spot);
        }
    }

    fn paint_paddles(&self, d: &Draw) {
        for (button, sign) in [(13u8, -1.0f32), (14, 1.0)] {
            let pressed = self.pressed(button);
            let (fill, stroke) = if pressed {
                (theme::ACCENT, Stroke::new(1.0_f32, theme::ACCENT))
            } else {
                (theme::WHEEL_BUTTON, Stroke::new(1.0_f32, theme::OUTLINE))
            };
            // Upper and lower parts (spec 7.2).
            let parts = [(0.66, -0.22, 0.26), (0.62, 0.34, 0.30)];
            if pressed {
                let glow = theme::ACCENT.gamma_multiply(0.25);
                for (x, y, h) in parts {
                    d.poly(&rounded_rect(sign * x, y, 0.18, h + 0.06, 0.05), glow, Stroke::NONE);
                }
            }
            for (x, y, h) in parts {
                d.poly(&rounded_rect(sign * x, y, 0.12, h, 0.03), fill, stroke);
            }
        }
    }

    fn paint_leds(&self, d: &Draw) {
        let n = self.led_count.clamp(1, 16);
        let radius = (0.022f32).min(0.17 / n as f32);
        for i in 0..n {
            let x = if n == 1 { 0.0 } else { -0.17 + 0.34 * i as f32 / (n - 1) as f32 };
            let lit = self.led_mask.is_some_and(|m| m & (1 << i) != 0);
            let pos = d.pt(v(x, -0.23));
            if lit {
                d.painter.circle_filled(pos, radius * d.r, led_color(i, n));
            } else {
                d.painter.circle(pos, radius * d.r, theme::WHEEL_BUTTON, Stroke::new(1.0_f32, theme::WHEEL_HUB_EDGE));
            }
        }
    }

    fn paint_spot(&self, d: &Draw, spot: &ButtonSpot) {
        let pressed = self.pressed(spot.button);
        let pos = d.pt(v(spot.x, spot.y));
        let radius = spot.kind.radius() * d.r;
        if spot.kind != SpotKind::Paddle {
            if pressed {
                d.painter.circle_filled(pos, radius * 1.6, theme::ACCENT.gamma_multiply(0.25));
                d.painter.circle_filled(pos, radius, theme::ACCENT);
            } else {
                d.painter.circle(pos, radius, theme::WHEEL_BUTTON, Stroke::new(1.0_f32, theme::OUTLINE));
            }
        }
        let Some(label) = self.labels[spot.button as usize - 1] else {
            return;
        };
        let size = (0.045 * d.r).max(9.0);
        let font = FontId::proportional(size);
        if label.chars().count() > 2 {
            // Long names sit just below the spot, on the hub, so the colour
            // is read against the hub rather than the lit circle.
            let below = pos + Vec2::new(0.0, radius + size * 0.8);
            let color = if pressed { theme::ACCENT } else { theme::WHEEL_LABEL };
            d.painter.text(below, Align2::CENTER_CENTER, label, font, color);
        } else {
            let color = if pressed { theme::ACCENT_TEXT } else { theme::WHEEL_LABEL };
            // The default fonts lack some arrow glyphs, so D-pad arrows are
            // drawn as upright triangles.
            if let Some(dir) = arrow_dir(label) {
                let (fwd, side) = (dir * radius * 0.55, Vec2::new(-dir.y, dir.x) * radius * 0.5);
                let tri = vec![pos + fwd, pos - fwd * 0.7 + side, pos - fwd * 0.7 - side];
                d.painter.add(Shape::convex_polygon(tri, color, Stroke::NONE));
            } else {
                d.painter.text(pos, Align2::CENTER_CENTER, label, font, color);
            }
        }
    }

    /// Chips for pressed buttons that have no drawn position (spec 7.5).
    fn paint_chips(&self, painter: &Painter, rect: Rect) {
        let font = FontId::proportional(theme::SIZE_SMALL);
        let galleys: Vec<_> = (1..=64u8)
            .filter(|&n| self.pressed(n) && !has_spot(n))
            .map(|n| match self.labels[n as usize - 1] {
                Some(l) => format!("{} {}", n, l),
                None => n.to_string(),
            })
            .map(|t| painter.layout_no_wrap(t, font.clone(), theme::ACCENT_TEXT))
            .collect();
        let pad = Vec2::new(8.0, 3.0);
        let gap = 6.0;
        let total: f32 =
            galleys.iter().map(|g| g.size().x + 2.0 * pad.x + gap).sum::<f32>() - gap;
        let mut x = rect.center().x - total / 2.0;
        for g in galleys {
            let size = g.size() + 2.0 * pad;
            if x + size.x > rect.right() + 1.0 || x < rect.left() - 1.0 {
                break;
            }
            let chip = Rect::from_min_size(Pos2::new(x, rect.center().y - size.y / 2.0), size);
            painter.rect_filled(chip, size.y / 2.0, theme::ACCENT);
            painter.galley(chip.min + pad, g, theme::ACCENT_TEXT);
            x += size.x + gap;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Pos2, b: Pos2) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn no_rotation_maps_unit_x_to_centre_plus_r() {
        let p = to_screen(v(1.0, 0.0), 0.0, Pos2::new(100.0, 50.0), 40.0);
        assert!(close(p, Pos2::new(140.0, 50.0)), "{p:?}");
    }

    #[test]
    fn positive_angle_turns_clockwise_on_screen() {
        // Top marker (0,-1) rotated +90 degrees lands right of the centre.
        let p = to_screen(v(0.0, -1.0), 90f32.to_radians(), Pos2::new(10.0, 10.0), 5.0);
        assert!(close(p, Pos2::new(15.0, 10.0)), "{p:?}");
    }

    #[test]
    fn scale_and_offset_apply() {
        let p = to_screen(v(0.5, 0.25), 0.0, Pos2::new(-3.0, 7.0), 8.0);
        assert!(close(p, Pos2::new(1.0, 9.0)), "{p:?}");
    }

    #[test]
    fn led_colours_by_position() {
        let colours: Vec<_> = (0..10).map(|i| led_color(i, 10)).collect();
        assert!(colours[..4].iter().all(|c| *c == theme::ACCENT));
        assert!(colours[4..8].iter().all(|c| *c == theme::WARN));
        assert!(colours[8..].iter().all(|c| *c == theme::BAD));
        assert_eq!(led_color(0, 1), theme::ACCENT);
        assert_eq!(led_color(4, 5), theme::BAD);
    }
}
