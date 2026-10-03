//! Half-block frame buffer plus a text layer.
//!
//! Each terminal cell shows `▀` with the foreground as the upper pixel and the
//! background as the lower pixel, so a `cols x rows` pane is a `cols x rows*2`
//! pixel canvas. Text spans are drawn on top and win over pixels.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const fn hex(v: u32) -> Rgb {
        Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }
    pub fn mix(self, o: Rgb, t: f32) -> Rgb {
        let f = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8;
        Rgb(f(self.0, o.0), f(self.1, o.1), f(self.2, o.2))
    }
    pub fn shade(self) -> Rgb {
        self.mix(Rgb(0, 0, 0), 0.25)
    }
    pub fn color(self) -> Color {
        Color::Rgb(self.0, self.1, self.2)
    }
}

/// Sprite palette indexed by ASCII key.
pub struct Pal([Option<Rgb>; 128]);

impl Pal {
    pub fn of(entries: &[(char, Rgb)]) -> Pal {
        let mut p = Pal([None; 128]);
        for &(k, c) in entries {
            p.0[k as usize & 127] = Some(c);
        }
        p
    }
    fn get(&self, k: char) -> Option<Rgb> {
        if k.is_ascii() {
            self.0[k as usize]
        } else {
            None
        }
    }
}

#[derive(Default, Clone, Copy)]
pub struct SpriteOpts {
    /// Replace the eyes with shaded skin.
    pub closed: bool,
    /// Show the back of the head: face keys become hair.
    pub back: bool,
    /// Push the upper body down one pixel.
    pub bow: bool,
}

struct Span {
    col: i32,
    row: i32,
    text: String,
    fg: Rgb,
    bg: Rgb,
}

pub struct Canvas {
    pub cols: i32,
    pub rows: i32,
    pub w: i32,
    pub h: i32,
    /// Horizontal origin shift applied to every pixel and text call.
    pub ox: i32,
    px: Vec<Rgb>,
    spans: Vec<Span>,
}

impl Canvas {
    pub fn new() -> Canvas {
        Canvas { cols: 0, rows: 0, w: 0, h: 0, ox: 0, px: Vec::new(), spans: Vec::new() }
    }

    pub fn begin(&mut self, cols: u16, rows: u16, bg: Rgb) {
        self.cols = cols as i32;
        self.rows = rows as i32;
        self.w = self.cols;
        self.h = self.rows * 2;
        self.ox = 0;
        self.px.clear();
        self.px.resize((self.w * self.h) as usize, bg);
        self.spans.clear();
    }

    pub fn pi(&mut self, x: i32, y: i32, c: Rgb) {
        let x = x + self.ox;
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        self.px[(y * self.w + x) as usize] = c;
    }

    pub fn p(&mut self, x: f32, y: f32, c: Rgb) {
        self.pi(x.round() as i32, y.round() as i32, c);
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb) {
        for j in 0..h {
            for i in 0..w {
                self.pi(x + i, y + j, c);
            }
        }
    }

    pub fn sprite(&mut self, rows: &[&str], pal: &Pal, x: i32, y: i32, o: SpriteOpts) {
        for (r, line) in rows.iter().enumerate() {
            let yy = y + r as i32 + if o.bow && r < 6 { 1 } else { 0 };
            for (c, mut k) in line.chars().enumerate() {
                if k == '.' {
                    continue;
                }
                if o.closed && k == 'E' {
                    k = 's';
                }
                if o.back && matches!(k, 'S' | 'E' | 's') {
                    k = 'h';
                }
                if let Some(col) = pal.get(k) {
                    self.pi(x + c as i32, yy, col);
                }
            }
        }
    }

    /// Queue text at a cell. Returns the column after the text.
    pub fn text(&mut self, col: i32, row: i32, s: &str, fg: Rgb, bg: Rgb) -> i32 {
        let width = str_width(s) as i32;
        self.spans.push(Span { col: col + self.ox, row, text: s.to_string(), fg, bg });
        col + width
    }

    pub fn blit(&self, buf: &mut Buffer, area: Rect) {
        let cols = self.cols.min(area.width as i32);
        let rows = self.rows.min(area.height as i32);
        for row in 0..rows {
            for col in 0..cols {
                let top = self.px[((row * 2) * self.w + col) as usize];
                let bot = self.px[((row * 2 + 1) * self.w + col) as usize];
                buf[(area.x + col as u16, area.y + row as u16)]
                    .set_symbol("▀")
                    .set_fg(top.color())
                    .set_bg(bot.color());
            }
        }
        for s in &self.spans {
            if s.row < 0 || s.row >= rows || s.col >= cols {
                continue;
            }
            // Drop whatever hangs off the left edge.
            let mut col = s.col;
            let mut text = s.text.as_str();
            while col < 0 {
                let Some(ch) = text.chars().next() else { break };
                col += ch.width().unwrap_or(0) as i32;
                text = &text[ch.len_utf8()..];
            }
            let style = Style::default().fg(s.fg.color()).bg(s.bg.color());
            buf.set_stringn(
                area.x + col as u16,
                area.y + s.row as u16,
                text,
                (cols - col) as usize,
                style,
            );
        }
    }

    /// Debug dump as a binary PPM. Text cells are filled with their background.
    pub fn ppm(&self) -> Vec<u8> {
        let mut px = self.px.clone();
        for s in &self.spans {
            let mut col = s.col;
            for ch in s.text.chars() {
                let w = ch.width().unwrap_or(0) as i32;
                for c in col..col + w {
                    for y in [s.row * 2, s.row * 2 + 1] {
                        if c >= 0 && c < self.w && y >= 0 && y < self.h {
                            let mid = if ch == ' ' { s.bg } else { s.bg.mix(s.fg, 0.5) };
                            px[(y * self.w + c) as usize] = if y % 2 == 0 { s.bg } else { mid };
                        }
                    }
                }
                col += w;
            }
        }
        let mut out = format!("P6 {} {} 255\n", self.w, self.h).into_bytes();
        for c in px {
            out.extend_from_slice(&[c.0, c.1, c.2]);
        }
        out
    }
}

pub fn str_width(s: &str) -> usize {
    s.chars().map(|c| c.width().unwrap_or(0)).sum()
}

/// Truncate or pad to exactly `w` display columns.
pub fn fit(s: &str, w: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0);
        if used + cw > w {
            break;
        }
        out.push(ch);
        used += cw;
    }
    out.extend(std::iter::repeat(' ').take(w - used));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_handles_wide_chars() {
        assert_eq!(fit("abc", 5), "abc  ");
        assert_eq!(fit("abcdef", 3), "abc");
        assert_eq!(fit("審査ガイド", 5), "審査 ");
        assert_eq!(str_width(&fit("審査ガイド", 5)), 5);
    }
}
