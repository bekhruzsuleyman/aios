//! Drawing helpers on tiny-skia: theme colours, rounded panels with a soft
//! shadow, the static pastel backdrop, and conversion to wl_shm pixels.

use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub dark: bool,
    pub panel: Color,
    pub panel_border: Color,
    pub chip: Color,
    pub text: Color,
    pub text_dim: Color,
    pub accent: Color,
    pub ok: Color,
    pub warn: Color,
    pub bad: Color,
    pub shadow: Color,
    pub field: Color,
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color::from_rgba8(r, g, b, a)
}

impl Theme {
    pub fn light() -> Theme {
        Theme {
            dark: false,
            panel: rgba(255, 255, 255, 214),
            panel_border: rgba(255, 255, 255, 240),
            chip: rgba(236, 238, 245, 230),
            text: rgba(32, 35, 46, 255),
            text_dim: rgba(108, 113, 133, 255),
            accent: rgba(54, 93, 220, 255),
            ok: rgba(40, 170, 100, 255),
            warn: rgba(214, 150, 30, 255),
            bad: rgba(214, 70, 70, 255),
            shadow: rgba(40, 40, 80, 255),
            field: rgba(244, 245, 250, 255),
        }
    }

    pub fn dark() -> Theme {
        Theme {
            dark: true,
            panel: rgba(30, 32, 44, 222),
            panel_border: rgba(255, 255, 255, 30),
            chip: rgba(52, 55, 72, 230),
            text: rgba(238, 240, 247, 255),
            text_dim: rgba(150, 156, 178, 255),
            accent: rgba(130, 165, 255, 255),
            ok: rgba(90, 210, 140, 255),
            warn: rgba(240, 190, 80, 255),
            bad: rgba(255, 120, 120, 255),
            shadow: rgba(0, 0, 0, 255),
            field: rgba(42, 45, 60, 255),
        }
    }
}

pub fn paint(color: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(color);
    p.anti_alias = true;
    p
}

pub fn with_alpha(c: Color, a: f32) -> Color {
    let mut c = c;
    c.set_alpha((c.alpha() * a).clamp(0.0, 1.0));
    c
}

/// Fill a rounded rectangle.
pub fn rounded(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let r = r.min(w / 2.0).min(h / 2.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    if let Some(path) = pb.finish() {
        pixmap.fill_path(
            &path,
            &paint(color),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// A panel: soft layered shadow, translucent fill, hairline border.
#[allow(clippy::too_many_arguments)]
pub fn panel(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    theme: &Theme,
    shadow: f32,
) {
    let layers = 8;
    for i in (1..=layers).rev() {
        let s = shadow * i as f32 / layers as f32;
        let a =
            0.035 * (1.0 - i as f32 / (layers as f32 + 1.0)) * if theme.dark { 2.0 } else { 1.0 };
        rounded(
            pixmap,
            x - s * 0.5,
            y - s * 0.2,
            w + s,
            h + s,
            r + s * 0.5,
            with_alpha(theme.shadow, a),
        );
    }
    rounded(
        pixmap,
        x - 1.0,
        y - 1.0,
        w + 2.0,
        h + 2.0,
        r + 1.0,
        theme.panel_border,
    );
    rounded(pixmap, x, y, w, h, r, theme.panel);
}

pub fn circle(pixmap: &mut Pixmap, cx: f32, cy: f32, r: f32, color: Color) {
    if let Some(path) = PathBuilder::from_circle(cx, cy, r) {
        pixmap.fill_path(
            &path,
            &paint(color),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

pub fn hline(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, color: Color) {
    if let Some(rect) = Rect::from_xywh(x, y, w, 1.0) {
        pixmap.fill_rect(rect, &paint(color), Transform::identity(), None);
    }
}

/// A check mark inside a circle (finished plan step).
pub fn check(pixmap: &mut Pixmap, cx: f32, cy: f32, color: Color) {
    circle(pixmap, cx, cy, 8.0, color);
    let mut pb = PathBuilder::new();
    pb.move_to(cx - 4.0, cy);
    pb.line_to(cx - 1.0, cy + 3.0);
    pb.line_to(cx + 4.5, cy - 3.5);
    if let Some(path) = pb.finish() {
        let stroke = tiny_skia::Stroke {
            width: 2.0,
            line_cap: tiny_skia::LineCap::Round,
            ..Default::default()
        };
        pixmap.stroke_path(
            &path,
            &paint(Color::WHITE),
            &stroke,
            Transform::identity(),
            None,
        );
    }
}

/// The desktop backdrop: a soft pastel gradient (mint top-left, lavender
/// bottom-left, peach right) that already looks blurred. Computed once per
/// size and theme; nothing is blurred per frame.
type Blob = ([f32; 2], [f32; 3]);

pub fn backdrop(width: u32, height: u32, dark: bool) -> Option<Pixmap> {
    let mut pixmap = Pixmap::new(width.max(1), height.max(1))?;
    let (base, blobs): ([f32; 3], [Blob; 3]) = if dark {
        (
            [22.0, 24.0, 34.0],
            [
                ([0.0, 0.0], [40.0, 82.0, 74.0]),
                ([0.0, 1.0], [62.0, 50.0, 96.0]),
                ([1.0, 0.45], [96.0, 62.0, 52.0]),
            ],
        )
    } else {
        (
            [246.0, 244.0, 248.0],
            [
                ([0.0, 0.0], [196.0, 236.0, 220.0]),
                ([0.0, 1.0], [221.0, 211.0, 246.0]),
                ([1.0, 0.45], [252.0, 219.0, 200.0]),
            ],
        )
    };
    let (w, h) = (width as f32, height as f32);
    let diag = (w * w + h * h).sqrt();
    let pixels = pixmap.pixels_mut();
    for y in 0..height {
        for x in 0..width {
            let (fx, fy) = (x as f32, y as f32);
            let mut acc = [0.0f32; 3];
            let mut total = 0.0f32;
            for (pos, col) in &blobs {
                let dx = fx - pos[0] * w;
                let dy = fy - pos[1] * h;
                let d = (dx * dx + dy * dy).sqrt() / diag;
                let wgt = (-d * d * 9.0).exp();
                for c in 0..3 {
                    acc[c] += col[c] * wgt;
                }
                total += wgt;
            }
            let base_w = 0.25;
            let mut rgb = [0u8; 3];
            for c in 0..3 {
                rgb[c] = ((acc[c] + base[c] * base_w) / (total + base_w)).clamp(0.0, 255.0) as u8;
            }
            // Light ordered dither so the gradient does not band.
            let n = ((x * 7 + y * 13) % 4) as u8;
            let i = (y * width + x) as usize;
            if let Some(p) = tiny_skia::PremultipliedColorU8::from_rgba(
                rgb[0].saturating_add(n / 2),
                rgb[1].saturating_add(n / 2),
                rgb[2].saturating_add(n / 2),
                255,
            ) {
                pixels[i] = p;
            }
        }
    }
    Some(pixmap)
}

/// Copy a premultiplied RGBA pixmap into an ARGB8888 (little-endian BGRA)
/// wl_shm buffer.
pub fn to_argb(pixmap: &Pixmap, out: &mut [u8]) {
    for (src, dst) in pixmap.data().chunks_exact(4).zip(out.chunks_exact_mut(4)) {
        dst[0] = src[2];
        dst[1] = src[1];
        dst[2] = src[0];
        dst[3] = src[3];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backdrop_is_not_flat() {
        let p = backdrop(64, 40, false).unwrap();
        let first = p.pixels()[0];
        let last = p.pixels()[p.pixels().len() - 1];
        assert_ne!(first, last);
        assert!(p.pixels().iter().all(|px| px.alpha() == 255));
    }

    #[test]
    fn argb_conversion_swaps_red_and_blue() {
        let mut p = Pixmap::new(1, 1).unwrap();
        p.fill(Color::from_rgba8(10, 20, 30, 255));
        let mut out = [0u8; 4];
        to_argb(&p, &mut out);
        assert_eq!(out, [30, 20, 10, 255]);
    }

    #[test]
    fn panel_draws() {
        let mut p = Pixmap::new(100, 100).unwrap();
        panel(&mut p, 10.0, 10.0, 80.0, 60.0, 16.0, &Theme::light(), 12.0);
        assert!(p.pixel(50, 40).unwrap().alpha() > 200);
    }
}
