//! Response curve chart for sticks and triggers.

use flugdiggi_core::config::Curve;
use gpui_kit::*;

use crate::theme::Colors;

pub const CURVE_KINDS: [&str; 4] = ["Default", "Quick", "Slow", "Custom"];

/// Points of the curve in `0.0..=1.0` (input, output), for a curve in raw units `0..=max`.
pub fn curve_points(c: &Curve, max: u8) -> Vec<(f32, f32)> {
    let m = f32::from(max.max(1));
    let start = (f32::from(c.start) / m).clamp(0.0, 1.0);
    let end = (f32::from(c.end) / m).clamp(start, 1.0);
    let span = (end - start).max(f32::EPSILON);
    let shape = |exp: f32| -> Vec<(f32, f32)> {
        let mut pts = vec![(0.0, 0.0), (start, 0.0)];
        for i in 1..=24u8 {
            let t = f32::from(i) / 24.0;
            pts.push((start + t * span, t.powf(exp)));
        }
        pts.push((1.0, 1.0));
        pts
    };
    match c.kind {
        1 => shape(0.5),
        2 => shape(2.0),
        3 => {
            let p = |[x, y]: [u8; 2]| ((f32::from(x) / m).clamp(0.0, 1.0), (f32::from(y) / m).clamp(0.0, 1.0));
            vec![(0.0, 0.0), (start, 0.0), p(c.point1), p(c.point2), (end, 1.0), (1.0, 1.0)]
        }
        _ => shape(1.0),
    }
}

/// The chart: ignored input ranges shaded, the curve on top.
pub fn chart(c: &Curve, max: u8, edited: bool) -> impl IntoElement {
    let pts = curve_points(c, max);
    let m = f32::from(max.max(1));
    let start = (f32::from(c.start) / m).clamp(0.0, 1.0);
    let end = (f32::from(c.end) / m).clamp(start, 1.0);
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
            let mut it = pts.iter();
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
