//! Color maths and geometry for the picker, with no terminal and no drawing: hex <-> RGB <-> HSV,
//! the wheel's cell <-> (hue, saturation) mapping and hit testing, the sliders, and degrading
//! truecolor to the nearest of the 256 indexed colors.

use std::cell::Cell;

use ratatui::style::Color;

use crate::profile::{hex_rgb, normalize_hex};

/// Hue in degrees `[0, 360)`, saturation and value in `[0, 1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> Hsv {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    Hsv { h: h.rem_euclid(360.0), s: if max == 0.0 { 0.0 } else { d / max }, v: max }
}

pub fn hsv_to_rgb(c: Hsv) -> (u8, u8, u8) {
    let h = c.h.rem_euclid(360.0) / 60.0;
    let (s, v) = (c.s.clamp(0.0, 1.0), c.v.clamp(0.0, 1.0));
    let ch = v * s;
    let x = ch * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (ch, x, 0.0),
        1 => (x, ch, 0.0),
        2 => (0.0, ch, x),
        3 => (0.0, x, ch),
        4 => (x, 0.0, ch),
        _ => (ch, 0.0, x),
    };
    let m = v - ch;
    let to8 = |f: f32| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (to8(r), to8(g), to8(b))
}

pub fn hsv_to_hex(c: Hsv) -> String {
    let (r, g, b) = hsv_to_rgb(c);
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// `#rrggbb` (or `#rgb`) -> HSV. `None` if it is not a color.
pub fn hex_to_hsv(hex: &str) -> Option<Hsv> {
    let (r, g, b) = hex_rgb(hex)?;
    Some(rgb_to_hsv(r, g, b))
}

/// Parse typed text and, if it is a color, update `current`, keeping the old hue when the new color
/// has none (gray, black) so the marker does not jump to red.
pub fn hsv_from_typed(typed: &str, current: Hsv) -> Option<Hsv> {
    normalize_hex(typed)?;
    let mut n = hex_to_hsv(typed)?;
    if n.s < 0.005 || n.v < 0.005 {
        n.h = current.h;
    }
    if n.v < 0.005 {
        n.s = current.s;
    }
    Some(n)
}

// ---- the wheel ------------------------------------------------------------------------

/// A circle drawn with half-block characters: every cell is two square pixels stacked, so a wheel
/// `cols` cells wide and `rows` tall is `cols` x `2*rows` pixels. For a round circle on a ~1:2 cell,
/// `cols` should be `2 * rows`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WheelGeom {
    pub cols: u16,
    pub rows: u16,
}

impl WheelGeom {
    fn center_radius(&self) -> (f32, f32, f32) {
        let (w, h) = (self.cols as f32, self.rows as f32 * 2.0);
        (w / 2.0, h / 2.0, (w.min(h) / 2.0 - 0.5).max(0.5))
    }

    /// Hue and saturation of the pixel at (x, y), `None` outside the circle.
    pub fn pixel_hs(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let (cx, cy, r) = self.center_radius();
        let (dx, dy) = (x - cx, cy - y);
        let dist = dx.hypot(dy);
        (dist <= r).then(|| ((dy.atan2(dx).to_degrees()).rem_euclid(360.0), (dist / r).min(1.0)))
    }

    /// The pixel a cell's center stands for.
    fn cell_point(col: i32, row: i32) -> (f32, f32) {
        (col as f32 + 0.5, row as f32 * 2.0 + 1.0)
    }

    /// A click at cell (col, row), relative to the wheel's top-left. `None` outside the circle.
    pub fn hit(&self, col: i32, row: i32) -> Option<(f32, f32)> {
        let (x, y) = Self::cell_point(col, row);
        self.pixel_hs(x, y)
    }

    /// A drag at cell (col, row): like [`hit`], but outside the circle it clamps to the rim.
    pub fn hit_clamped(&self, col: i32, row: i32) -> (f32, f32) {
        let (x, y) = Self::cell_point(col, row);
        let (cx, cy, r) = self.center_radius();
        let (dx, dy) = (x - cx, cy - y);
        let dist = dx.hypot(dy);
        if dist == 0.0 {
            return (0.0, 0.0);
        }
        ((dy.atan2(dx).to_degrees()).rem_euclid(360.0), (dist / r).min(1.0))
    }

    /// The pixel position of a color, and the cell that holds it.
    pub fn marker(&self, hue: f32, sat: f32) -> (f32, f32, u16, u16) {
        let (cx, cy, r) = self.center_radius();
        let a = hue.to_radians();
        let (x, y) = (cx + sat * r * a.cos(), cy - sat * r * a.sin());
        let col = (x.floor().max(0.0) as u16).min(self.cols.saturating_sub(1));
        let row = ((y / 2.0).floor().max(0.0) as u16).min(self.rows.saturating_sub(1));
        (x, y, col, row)
    }
}

// ---- sliders ------------------------------------------------------------------------

/// Position along a bar `width` cells wide -> `[0, 1]` (the first cell is 0, the last is 1).
pub fn bar_fraction(col: i32, width: u16) -> f32 {
    if width <= 1 { 0.0 } else { (col as f32 / (width - 1) as f32).clamp(0.0, 1.0) }
}

/// Where along a bar a fraction sits, as a cell index.
pub fn bar_cell(fraction: f32, width: u16) -> u16 {
    if width <= 1 { 0 } else { (fraction.clamp(0.0, 1.0) * (width - 1) as f32).round() as u16 }
}

// ---- terminal color depth ---------------------------------------------------------------

/// Does this terminal do 24-bit color? (`COLORTERM=truecolor|24bit`.)
pub fn detect_truecolor() -> bool {
    std::env::var("COLORTERM").is_ok_and(|v| v.contains("truecolor") || v.contains("24bit"))
}

/// The nearest of the xterm 256 colors (the 6x6x6 cube or the gray ramp, whichever is closer).
pub fn to_indexed(r: u8, g: u8, b: u8) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let near = |c: u8| {
        LEVELS.iter().enumerate().min_by_key(|(_, l)| (**l as i32 - c as i32).abs()).map(|(i, _)| i).unwrap_or(0)
    };
    let (ri, gi, bi) = (near(r), near(g), near(b));
    let cube = (LEVELS[ri], LEVELS[gi], LEVELS[bi]);
    let cube_idx = 16 + 36 * ri as u8 + 6 * gi as u8 + bi as u8;
    let avg = (r as u32 + g as u32 + b as u32) / 3;
    let gray_step = ((avg.saturating_sub(8)) / 10).min(23) as u8;
    let gray_v = 8 + 10 * gray_step;
    let gray_idx = 232 + gray_step;
    let dist = |a: (u8, u8, u8), b: (u8, u8, u8)| -> i32 {
        let d = |x: u8, y: u8| (x as i32 - y as i32).pow(2);
        d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2)
    };
    if dist((r, g, b), (gray_v, gray_v, gray_v)) < dist((r, g, b), cube) { gray_idx } else { cube_idx }
}

thread_local! {
    static TRUECOLOR: Cell<bool> = const { Cell::new(true) };
}

/// Set for the frame about to be drawn (a thread-local, so parallel tests cannot interfere).
pub fn set_truecolor(on: bool) {
    TRUECOLOR.with(|t| t.set(on));
}

/// A drawing color: exact on truecolor terminals, else the nearest of the 256.
pub fn term_color(r: u8, g: u8, b: u8) -> Color {
    if TRUECOLOR.with(Cell::get) { Color::Rgb(r, g, b) } else { Color::Indexed(to_indexed(r, g, b)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_rgb_hsv_round_trip_for_every_primary_and_a_spread_of_colors() {
        for hex in [
            "#000000", "#ffffff", "#ff0000", "#00ff00", "#0000ff", "#ffff00", "#00ffff", "#ff00ff", "#ff6a00",
            "#2c2c2c", "#d8d8d8", "#7aa2f7", "#040a05", "#123456",
        ] {
            let hsv = hex_to_hsv(hex).unwrap();
            assert_eq!(hsv_to_hex(hsv), hex, "{hex} -> {hsv:?}");
        }
        // every 8-bit gray and a coarse sweep of the cube survive the trip
        for g in 0..=255u8 {
            let hex = format!("#{g:02x}{g:02x}{g:02x}");
            assert_eq!(hsv_to_hex(hex_to_hsv(&hex).unwrap()), hex);
        }
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let hex = format!("#{r:02x}{g:02x}{b:02x}");
                    assert_eq!(hsv_to_hex(hex_to_hsv(&hex).unwrap()), hex);
                }
            }
        }
    }

    #[test]
    fn known_hsv_values() {
        let red = hex_to_hsv("#ff0000").unwrap();
        assert_eq!((red.h, red.s, red.v), (0.0, 1.0, 1.0));
        assert_eq!(hex_to_hsv("#00ff00").unwrap().h, 120.0);
        assert_eq!(hex_to_hsv("#0000ff").unwrap().h, 240.0);
        assert_eq!(hex_to_hsv("#808080").unwrap().s, 0.0);
        assert_eq!(hsv_to_hex(Hsv { h: 60.0, s: 1.0, v: 1.0 }), "#ffff00");
        assert_eq!(hsv_to_hex(Hsv { h: 0.0, s: 0.0, v: 0.5 }), "#808080");
        assert_eq!(hsv_to_hex(Hsv { h: 360.0, s: 1.0, v: 1.0 }), "#ff0000", "360 wraps to red");
        assert_eq!(hex_to_hsv("zzz"), None);
        assert_eq!(hex_to_hsv("#f60").map(hsv_to_hex).as_deref(), Some("#ff6600"));
    }

    #[test]
    fn typed_hex_keeps_the_hue_when_the_color_has_none() {
        let was = Hsv { h: 200.0, s: 0.8, v: 0.9 };
        assert_eq!(hsv_from_typed("#808080", was).unwrap().h, 200.0, "gray keeps the hue");
        let black = hsv_from_typed("#000000", was).unwrap();
        assert_eq!((black.h, black.s), (200.0, 0.8), "black keeps hue and saturation");
        assert_eq!(hsv_from_typed("#ff0000", was).unwrap().h, 0.0);
        assert_eq!(hsv_from_typed("#12", was), None, "an incomplete hex changes nothing");
        assert_eq!(hsv_from_typed("", was), None);
    }

    #[test]
    fn the_wheel_is_a_circle_and_hit_testing_knows_outside() {
        let g = WheelGeom { cols: 24, rows: 12 };
        // the center is saturation 0
        let (_, s) = g.hit(12, 6).unwrap();
        assert!(s < 0.1, "{s}");
        // far right middle: hue ~0, saturation near 1
        let (h, s) = g.hit(22, 6).unwrap();
        assert!(!(12.0..=348.0).contains(&h) && s > 0.8, "{h} {s}");
        // straight up: hue ~90; left: ~180; down: ~270
        let (h, _) = g.hit(12, 1).unwrap();
        assert!((h - 90.0).abs() < 12.0, "{h}");
        let (h, _) = g.hit(1, 6).unwrap();
        assert!((h - 180.0).abs() < 12.0, "{h}");
        let (h, _) = g.hit(12, 10).unwrap();
        assert!((h - 270.0).abs() < 12.0, "{h}");
        // the corners of the bounding box are outside the circle; so is anything past the edge
        for (c, r) in [(0, 0), (23, 0), (0, 11), (23, 11), (-1, 6), (24, 6), (12, -1), (12, 12), (100, 100)] {
            assert_eq!(g.hit(c, r), None, "({c},{r}) is outside");
        }
        // every cell either hits or not, and everything that hits is in range
        let mut inside = 0;
        for r in 0..12 {
            for c in 0..24 {
                if let Some((h, s)) = g.hit(c, r) {
                    inside += 1;
                    assert!((0.0..360.0).contains(&h) && (0.0..=1.0).contains(&s));
                }
            }
        }
        // area of a circle of diameter 24px over 24x24 px is pi/4 of the cells
        assert!((inside as f32 / (24.0 * 12.0) - std::f32::consts::FRAC_PI_4).abs() < 0.08, "{inside}");
    }

    #[test]
    fn dragging_outside_clamps_to_the_rim_instead_of_jumping() {
        let g = WheelGeom { cols: 24, rows: 12 };
        let (h, s) = g.hit_clamped(40, 6);
        assert!(!(12.0..=348.0).contains(&h) && s == 1.0, "{h} {s}");
        let (h, s) = g.hit_clamped(12, -5);
        assert!((h - 90.0).abs() < 5.0 && s == 1.0, "{h} {s}");
        // inside it agrees with a plain hit
        assert_eq!(g.hit_clamped(18, 6), g.hit(18, 6).unwrap());
    }

    #[test]
    fn the_marker_cell_matches_the_hit_test() {
        let g = WheelGeom { cols: 24, rows: 12 };
        for (c, r) in [(18, 6), (6, 6), (12, 2), (12, 10), (16, 3), (8, 9)] {
            let (h, s) = g.hit(c, r).unwrap();
            let (_, _, mc, mr) = g.marker(h, s);
            assert!((mc as i32 - c).abs() <= 1 && (mr as i32 - r).abs() <= 1, "({c},{r}) -> ({mc},{mr})");
        }
        // the center and the rim stay inside the grid
        let (_, _, c, r) = g.marker(0.0, 0.0);
        assert_eq!((c, r), (12, 6));
        let (_, _, c, r) = g.marker(45.0, 1.0);
        assert!(c < 24 && r < 12);
    }

    #[test]
    fn slider_mapping_reaches_both_ends_and_inverts() {
        assert_eq!(bar_fraction(0, 20), 0.0);
        assert_eq!(bar_fraction(19, 20), 1.0);
        assert_eq!(bar_fraction(-5, 20), 0.0, "dragging past the left end clamps");
        assert_eq!(bar_fraction(99, 20), 1.0, "and past the right");
        assert_eq!(bar_fraction(3, 1), 0.0, "a one-cell bar cannot divide by zero");
        for col in 0..20 {
            assert_eq!(bar_cell(bar_fraction(col, 20), 20), col as u16);
        }
        assert_eq!(bar_cell(2.0, 20), 19);
    }

    #[test]
    fn indexed_fallback_picks_the_nearest_of_the_256() {
        assert_eq!(to_indexed(0, 0, 0), 16);
        assert_eq!(to_indexed(255, 255, 255), 231);
        assert_eq!(to_indexed(255, 0, 0), 196);
        assert_eq!(to_indexed(0, 255, 0), 46);
        assert_eq!(to_indexed(0, 0, 255), 21);
        let gray = to_indexed(128, 128, 128);
        assert!((232..=255).contains(&gray), "mid gray uses the gray ramp: {gray}");
        // never panics, always a valid index
        for r in (0..=255).step_by(5) {
            for g in (0..=255).step_by(5) {
                let _ = to_indexed(r as u8, g as u8, 77);
            }
        }
    }

    #[test]
    fn term_color_degrades_per_thread() {
        set_truecolor(true);
        assert_eq!(term_color(1, 2, 3), Color::Rgb(1, 2, 3));
        set_truecolor(false);
        assert!(matches!(term_color(255, 0, 0), Color::Indexed(196)));
        set_truecolor(true);
    }
}
