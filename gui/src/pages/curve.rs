//! Response curves of sticks and triggers, edited the way the official app (3.4.9.5) does for the
//! Vader 4 series, and the chart that shows them.

use flugdiggi_core::config::{Curve, Stick};
use gpui_kit::*;

use crate::model::unit_to_u8;
use crate::theme::Colors;

/// Raw maximum of stick curve values.
pub const STICK_MAX: u8 = 127;
/// Raw maximum of trigger values.
pub const TRIGGER_MAX: u8 = 255;

pub const CURVE_KINDS: [&str; 4] = ["Default", "Quick", "Slow", "Custom"];
pub const CURVE_CUSTOM: u8 = 3;

/// Point 1 of the presets; point 2 is always at the top right.
const PRESET_POINT1: [[u8; 2]; 3] = [[64, 64], [64, 96], [64, 32]];

/// A raw stick value in whole percent, rounded like the official app.
fn pct(v: u8) -> f32 {
    (f32::from(v) * 100.0 / f32::from(STICK_MAX)).round()
}

/// The stick's edge byte as (outer deadzone, output reduction), both raw. Values above 127 are negative
/// edges, which lower the maximum output instead of ignoring the outer travel.
fn edge_parts(edge: u8) -> (u8, u8) {
    if edge <= STICK_MAX { (edge, 0) } else { (0, edge.saturating_sub(STICK_MAX)) }
}

fn lerp((ax, ay): (f32, f32), (bx, by): (f32, f32), x: f32) -> f32 {
    if (bx - ax).abs() <= f32::EPSILON { ay } else { ay + (by - ay) * (x - ax) / (bx - ax) }
}

/// The 9-value curve table the official app writes along with the curve: output percent plus 50 at
/// inputs of 0, 12, 25, … 100 percent (`BaseSwitchModel.CalculateCurvePoints`).
pub fn stick_table(s: &Stick) -> [u8; 9] {
    let c = &s.curve;
    let (outer, reduce) = edge_parts(s.edge);
    let start = (pct(c.start), 0.0);
    let mut p1 = (pct(c.point1[0]), pct(c.point1[1]));
    let mut p2 = (pct(c.point2[0]), pct(c.point2[1]));
    let end = (100.0 - pct(outer), 100.0 - pct(reduce));
    if p2.0 <= p1.0 && p1.1 < p2.1 && p1.0 == 0.0 {
        p1 = p2;
    }
    if p2.0 <= p1.0 && p1.1 < p2.1 && (p1.0 - 100.0).abs() <= f32::EPSILON {
        p2 = p1;
    }
    std::array::from_fn(|i| {
        let b = f32::from(u8::try_from(i.saturating_mul(100) / 8).unwrap_or(100));
        let out = if b <= start.0 {
            if p1.0 == 0.0 { lerp(p1, p2, b) } else { 0.0 }
        } else if b <= p1.0 {
            lerp(start, p1, b)
        } else if b <= p2.0 {
            lerp(p1, p2, b)
        } else if b < end.0 {
            lerp(p2, end, b)
        } else {
            100.0
        };
        unit_to_u8((out + 50.0).floor())
    })
}

fn sync_table(s: &mut Stick) {
    if s.curve_points.len() == 9 {
        s.curve_points = stick_table(s).to_vec();
    }
}

/// Selects one of the curve kinds. A preset also resets both deadzones, as in the official app.
pub fn set_stick_kind(s: &mut Stick, kind: u8) {
    s.curve.kind = kind;
    if let Some(p1) = PRESET_POINT1.get(usize::from(kind)) {
        s.curve.start = 0;
        s.edge = 0;
        s.curve.point1 = *p1;
        s.curve.point2 = [STICK_MAX, STICK_MAX];
    }
    sync_table(s);
}

/// Input range the curve spans, raw: (deadzone, start of the outer deadzone).
fn span(s: &Stick) -> (f32, f32) {
    let (outer, _) = edge_parts(s.edge);
    (f32::from(s.curve.start), f32::from(STICK_MAX.saturating_sub(outer)))
}

/// Sets the deadzone and the outer deadzone. The curve's points keep their place relative to the
/// range between them, and the curve becomes custom.
pub fn set_stick_deadzones(s: &mut Stick, start: u8, outer: u8) {
    let outer = outer.min(STICK_MAX);
    let start = start.min(STICK_MAX.saturating_sub(outer));
    let (lo, hi) = span(s);
    s.curve.start = start;
    if s.edge <= STICK_MAX || outer > 0 {
        s.edge = outer;
    }
    let (nlo, nhi) = span(s);
    let place = |x: u8| {
        let t = if hi > lo { ((f32::from(x) - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
        unit_to_u8(nlo + t * (nhi - nlo))
    };
    s.curve.point1[0] = place(s.curve.point1[0]);
    s.curve.point2[0] = place(s.curve.point2[0]);
    s.curve.kind = CURVE_CUSTOM;
    sync_table(s);
}

/// Moves a point of a custom curve. Point 1 stays left of and below point 2.
pub fn set_stick_point(s: &mut Stick, point: usize, axis: usize, v: u8) {
    let (_, hi) = span(s);
    let hi = unit_to_u8(hi);
    let c = &mut s.curve;
    match (point, axis) {
        (0, 0) => c.point1[0] = v.clamp(c.start.min(c.point2[0]), c.point2[0]),
        (0, _) => c.point1[1] = v.min(c.point2[1]),
        (_, 0) => c.point2[0] = v.clamp(c.point1[0].min(hi), hi),
        (_, _) => c.point2[1] = v.clamp(c.point1[1], STICK_MAX),
    }
    c.kind = CURVE_CUSTOM;
    sync_table(s);
}

/// Sets where a trigger starts to respond; it stays below the end. Point 1 follows, as in the official app.
pub fn set_trigger_start(c: &mut Curve, v: u8) {
    let v = v.min(c.end.saturating_sub(1));
    c.start = v;
    c.point1 = [v, v];
}

/// Sets where a trigger reaches full output; it stays above the start. Point 2 follows.
pub fn set_trigger_end(c: &mut Curve, v: u8) {
    let v = v.max(c.start.saturating_add(1));
    c.end = v;
    c.point2 = [v, v];
}

/// Points of a stick's curve in `0.0..=1.0` (input, output).
pub fn stick_points(s: &Stick) -> Vec<(f32, f32)> {
    let m = f32::from(STICK_MAX);
    let f = |v: u8| (f32::from(v) / m).clamp(0.0, 1.0);
    let c = &s.curve;
    let (outer, reduce) = edge_parts(s.edge);
    let start = f(c.start);
    vec![
        (0.0, 0.0),
        (start, 0.0),
        (f(c.point1[0]).max(start), f(c.point1[1])),
        (f(c.point2[0]), f(c.point2[1])),
        (1.0 - f(outer), 1.0 - f(reduce)),
        (1.0, 1.0 - f(reduce)),
    ]
}

/// Points of a trigger's response: nothing before the start, full output from the end on.
pub fn trigger_points(c: &Curve) -> Vec<(f32, f32)> {
    let m = f32::from(TRIGGER_MAX);
    let start = (f32::from(c.start) / m).clamp(0.0, 1.0);
    let end = (f32::from(c.end) / m).clamp(start, 1.0);
    vec![(0.0, 0.0), (start, 0.0), (end, 1.0), (1.0, 1.0)]
}

/// The chart: the curve on top of shading for the travel outside `active`, the input range the curve
/// responds to.
pub fn chart(points: Vec<(f32, f32)>, active: (f32, f32), edited: bool) -> impl IntoElement {
    let (start, end) = active;
    let color = if edited { Colors::edited() } else { rgb(0x00ec_ecec).into() };
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| {
            let (x0, y0) = (bounds.origin.x.as_f32(), bounds.origin.y.as_f32());
            let (w, h) = (bounds.size.width.as_f32(), bounds.size.height.as_f32());
            let rect = |x: f32, y: f32, rw: f32, rh: f32| Bounds::new(point(px(x), px(y)), size(px(rw), px(rh)));
            window.paint_quad(fill(bounds, rgb(0x001e_1e1e)));
            let grid: Hsla = rgb(0x002c_2c2c).into();
            for i in 1..4u8 {
                let f = f32::from(i) / 4.0;
                window.paint_quad(fill(rect(x0 + w * f, y0, 1.0, h), grid));
                window.paint_quad(fill(rect(x0, y0 + h * f, w, 1.0), grid));
            }
            let shade: Hsla = rgb(0x0033_3333).into();
            if start > 0.0 {
                window.paint_quad(fill(rect(x0, y0, w * start, h), shade));
            }
            if end < 1.0 {
                window.paint_quad(fill(rect(x0 + w * end, y0, w * (1.0 - end), h), shade));
            }
            let pad = 6.0;
            let to =
                |(x, y): (f32, f32)| point(px(x0 + pad + (w - pad * 2.) * x), px(y0 + h - pad - (h - pad * 2.) * y));
            let mut path = PathBuilder::stroke(px(2.));
            let mut it = points.iter();
            if let Some(&first) = it.next() {
                path.move_to(to(first));
                for &p in it {
                    path.line_to(to(p));
                }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, color);
            }
        },
    )
    .w_full()
    .h(px(180.))
    .rounded(px(8.))
}

/// Input range a stick's curve responds to, as fractions.
pub fn stick_active(s: &Stick) -> (f32, f32) {
    let m = f32::from(STICK_MAX);
    let (outer, _) = edge_parts(s.edge);
    ((f32::from(s.curve.start) / m).clamp(0.0, 1.0), 1.0 - (f32::from(outer) / m).clamp(0.0, 1.0))
}

pub fn trigger_active(c: &Curve) -> (f32, f32) {
    let m = f32::from(TRIGGER_MAX);
    ((f32::from(c.start) / m).clamp(0.0, 1.0), (f32::from(c.end) / m).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::{
        CURVE_CUSTOM, Curve, STICK_MAX, Stick, set_stick_deadzones, set_stick_kind, set_trigger_end, set_trigger_start,
        stick_table, unit_to_u8,
    };

    fn stick(kind: u8, start: u8, p1: [u8; 2], p2: [u8; 2], edge: u8) -> Stick {
        Stick {
            curve: Curve { kind, start, point1: p1, point2: p2, end: STICK_MAX },
            curve_type_ext: 0,
            curve_points: vec![0; 9],
            circularity: 0,
            edge,
        }
    }

    #[test]
    fn the_table_of_a_linear_curve_matches_what_the_official_app_stored() {
        // From a slot written by the official app.
        let s = stick(0, 0, [63, 63], [127, 127], 0);
        assert_eq!(stick_table(&s), [50, 62, 75, 87, 100, 112, 125, 137, 150]);
    }

    #[test]
    fn presets_reset_the_deadzones_and_move_point_one() {
        let mut s = stick(3, 20, [70, 40], [110, 120], 10);
        set_stick_kind(&mut s, 1);
        assert_eq!((s.curve.start, s.edge, s.curve.point1, s.curve.point2), (0, 0, [64, 96], [127, 127]));
        assert_eq!(s.curve_points, stick_table(&s).to_vec());
        let custom = s.clone();
        set_stick_kind(&mut s, CURVE_CUSTOM);
        assert_eq!(s.curve.point1, custom.curve.point1, "custom keeps the points");
    }

    #[test]
    fn deadzones_rescale_the_points_and_make_the_curve_custom() {
        let mut s = stick(0, 0, [64, 64], [127, 127], 0);
        set_stick_deadzones(&mut s, 27, 0);
        assert_eq!(s.curve.kind, CURVE_CUSTOM);
        assert_eq!(s.curve.point2[0], 127);
        assert_eq!(s.curve.point1[0], 27 + unit_to_u8(64.0 * 100.0 / 127.0));
        set_stick_deadzones(&mut s, 0, 0);
        assert_eq!(s.curve.point1, [64, 64], "moving back restores the point");
        let t = stick_table(&s);
        assert_eq!(s.curve_points, t.to_vec());
    }

    #[test]
    fn deadzones_together_never_exceed_the_travel() {
        let mut s = stick(0, 0, [64, 64], [127, 127], 0);
        set_stick_deadzones(&mut s, 100, 50);
        assert_eq!(u16::from(s.curve.start) + u16::from(s.edge), u16::from(STICK_MAX));
    }

    #[test]
    fn the_table_is_zero_in_the_deadzone_and_full_in_the_outer_deadzone() {
        let s = stick(3, 32, [64, 64], [95, 127], 32);
        let t = stick_table(&s);
        assert_eq!(t.first(), Some(&50));
        assert_eq!(t.get(1), Some(&50));
        assert_eq!(t.last(), Some(&150));
    }

    #[test]
    fn trigger_points_follow_start_and_end() {
        let mut c = Curve { kind: 0, start: 0, point1: [0, 0], point2: [255, 255], end: 255 };
        set_trigger_start(&mut c, 40);
        set_trigger_end(&mut c, 200);
        assert_eq!((c.start, c.point1, c.end, c.point2), (40, [40, 40], 200, [200, 200]));
        set_trigger_start(&mut c, 250);
        assert_eq!(c.start, 199);
        set_trigger_end(&mut c, 10);
        assert_eq!(c.end, 200);
    }
}
