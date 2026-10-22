//! Text shaping and rasterisation with cosmic-text, blended into a
//! tiny-skia pixmap.

use cosmic_text::{
    Attrs, Buffer, Color as TextColor, Family, FontSystem, Metrics, Shaping, Style, SwashCache,
    Weight, Wrap,
};
use tiny_skia::{Color, Pixmap};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Face {
    Sans,
    Mono,
    Serif,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub face: Face,
    pub italic: bool,
    pub bold: bool,
}

impl TextStyle {
    pub const fn sans(size: f32) -> Self {
        TextStyle {
            size,
            face: Face::Sans,
            italic: false,
            bold: false,
        }
    }
    pub const fn mono(size: f32) -> Self {
        TextStyle {
            size,
            face: Face::Mono,
            italic: false,
            bold: false,
        }
    }
    pub const fn serif_italic(size: f32) -> Self {
        TextStyle {
            size,
            face: Face::Serif,
            italic: true,
            bold: false,
        }
    }
    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
}

pub struct Text {
    fonts: FontSystem,
    cache: SwashCache,
}

impl Default for Text {
    fn default() -> Self {
        Self::new()
    }
}

impl Text {
    pub fn new() -> Self {
        let mut fonts = FontSystem::new();
        let has = |fonts: &FontSystem, name: &str| {
            fonts
                .db()
                .faces()
                .any(|f| f.families.iter().any(|(n, _)| n == name))
        };
        let sans = if has(&fonts, "Inter") {
            "Inter"
        } else {
            "DejaVu Sans"
        };
        fonts.db_mut().set_sans_serif_family(sans);
        fonts.db_mut().set_monospace_family("DejaVu Sans Mono");
        fonts.db_mut().set_serif_family("DejaVu Serif");
        Text {
            fonts,
            cache: SwashCache::new(),
        }
    }

    fn buffer(&mut self, s: &str, style: TextStyle, max_width: Option<f32>) -> Buffer {
        let line_height = (style.size * 1.3).ceil();
        let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(style.size, line_height));
        let family = match style.face {
            Face::Sans => Family::SansSerif,
            Face::Mono => Family::Monospace,
            Face::Serif => Family::Serif,
        };
        let mut attrs = Attrs::new().family(family);
        if style.italic {
            attrs = attrs.style(Style::Italic);
        }
        if style.bold {
            attrs = attrs.weight(Weight::SEMIBOLD);
        }
        {
            let mut b = buffer.borrow_with(&mut self.fonts);
            b.set_wrap(if max_width.is_some() {
                Wrap::WordOrGlyph
            } else {
                Wrap::None
            });
            b.set_size(max_width, None);
            b.set_text(s, &attrs, Shaping::Advanced, None);
            b.shape_until_scroll(false);
        }
        buffer
    }

    /// Width and height of `s` laid out with `style`.
    pub fn measure(&mut self, s: &str, style: TextStyle, max_width: Option<f32>) -> (f32, f32) {
        let buffer = self.buffer(s, style, max_width);
        let mut w: f32 = 0.0;
        let mut h: f32 = 0.0;
        for run in buffer.layout_runs() {
            w = w.max(run.line_w);
            h = run.line_top + run.line_height;
        }
        (w, h)
    }

    /// Draw `s` with its top-left corner at (x, y). Returns the height used.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        pixmap: &mut Pixmap,
        s: &str,
        x: f32,
        y: f32,
        style: TextStyle,
        color: Color,
        max_width: Option<f32>,
    ) -> f32 {
        let mut buffer = self.buffer(s, style, max_width);
        let c = color.to_color_u8();
        let tc = TextColor::rgba(c.red(), c.green(), c.blue(), c.alpha());
        let (pw, ph) = (pixmap.width() as i32, pixmap.height() as i32);
        let ox = x.round() as i32;
        let oy = y.round() as i32;
        let pixels = pixmap.pixels_mut();
        let mut height: f32 = 0.0;
        for run in buffer.layout_runs() {
            height = run.line_top + run.line_height;
        }
        buffer.draw(&mut self.fonts, &mut self.cache, tc, |gx, gy, w, h, col| {
            let a = col.a() as u32;
            if a == 0 {
                return;
            }
            for dy in 0..h as i32 {
                for dx in 0..w as i32 {
                    let px = ox + gx + dx;
                    let py = oy + gy + dy;
                    if px < 0 || py < 0 || px >= pw || py >= ph {
                        continue;
                    }
                    let i = (py * pw + px) as usize;
                    blend(&mut pixels[i], col.r(), col.g(), col.b(), a);
                }
            }
        });
        height
    }
}

/// Source-over blend of an unpremultiplied colour onto a premultiplied pixel.
fn blend(dst: &mut tiny_skia::PremultipliedColorU8, r: u8, g: u8, b: u8, a: u32) {
    let inv = 255 - a;
    let mix = |s: u8, d: u8| -> u8 { ((s as u32 * a + d as u32 * inv) / 255) as u8 };
    let out_a = (a + dst.alpha() as u32 * inv / 255) as u8;
    let nr = mix(r, dst.red()).min(out_a);
    let ng = mix(g, dst.green()).min(out_a);
    let nb = mix(b, dst.blue()).min(out_a);
    if let Some(p) = tiny_skia::PremultipliedColorU8::from_rgba(nr, ng, nb, out_a) {
        *dst = p;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_and_draws() {
        let mut text = Text::new();
        let (w, h) = text.measure("Hello", TextStyle::sans(16.0), None);
        let (w2, _) = text.measure("Hello, world", TextStyle::sans(16.0), None);
        if w == 0.0 {
            // No fonts installed on this host; nothing else to check.
            return;
        }
        assert!(w2 > w && h > 0.0);
        let mut pixmap = Pixmap::new(200, 40).unwrap();
        pixmap.fill(Color::WHITE);
        text.draw(
            &mut pixmap,
            "Hello",
            4.0,
            4.0,
            TextStyle::sans(16.0),
            Color::BLACK,
            None,
        );
        assert!(pixmap.pixels().iter().any(|p| p.red() < 128));
    }
}
