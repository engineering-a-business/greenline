//! Markup model and geometry.
//!
//! Annotations live in page-local *view points*: the page as displayed (after
//! /Rotate and CropBox), origin top-left, y down, 1 unit = 1 PDF point.
//! Every annotation is reduced to a list of `Prim`s; the screen painter and
//! the PDF exporter both draw from those, so export matches the screen.

use egui::{pos2, vec2, Pos2, Rect, Vec2};
use serde::{Deserialize, Serialize};

pub type Rgb = [u8; 3];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Annot {
    pub id: u64,
    pub page: usize,
    pub color: Rgb,
    pub width: f32,
    pub kind: Kind,
    /// Text-box width the user chose (drag or handle); the box widens back to
    /// this while editing and is shrunk to fit the text on commit.
    #[serde(default)]
    pub wrap_w: Option<f32>,
    /// Markups sharing a group id on one page select and move together.
    #[serde(default)]
    pub group: Option<u64>,
    #[serde(flatten)]
    pub look: Look,
}

/// Transparency and fill. Stored flat in the markup (older files get the defaults).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Look {
    /// Whole markup, 0..=1.
    pub opacity: f32,
    /// Inside colour for shapes and text boxes that take one (`Kind::fills`).
    pub fill: Option<Rgb>,
    /// The fill's own opacity, 0..=1, on top of `opacity`.
    pub fill_opacity: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self { opacity: 1.0, fill: None, fill_opacity: 1.0 }
    }
}

fn a8(x: f32) -> u8 {
    (x.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Kind {
    /// Text box with a leader arrow pointing at `anchor`. `pos` is the box's
    /// top-left; height follows the wrapped text.
    Callout { anchor: Pos2, pos: Pos2, box_w: f32, text: String, size: f32 },
    Text { pos: Pos2, box_w: f32, text: String, size: f32 },
    Cloud { rect: Rect },
    Rect { rect: Rect },
    Highlight { rect: Rect },
    Arrow { a: Pos2, b: Pos2 },
    Line { a: Pos2, b: Pos2 },
    Pen { pts: Vec<Pos2> },
    /// Stamp or signature from the image library (`stamps`), drawn into `rect`.
    /// `image` is the stored file's name; `name` is what the markup list shows.
    Image { rect: Rect, image: String, name: String },
    /// Dimension from `a` to `b`. Its line sits `off` points to the side of a→b
    /// (along `dim_normal`), joined to the points by extension lines.
    /// `scale` is real inches per paper inch (48 for 1/4" = 1'-0"); `size` is the label's text size.
    Distance { a: Pos2, b: Pos2, off: f32, scale: f32, size: f32 },
    /// Measured polygon, labelled with its area.
    Area { pts: Vec<Pos2>, scale: f32, size: f32 },
    /// Measured polyline, labelled with its total length; `closed` when it returns to its start.
    Perimeter { pts: Vec<Pos2>, closed: bool, scale: f32, size: f32 },
}

/// `alpha` is 0..=255 throughout.
#[derive(Clone, Debug)]
pub enum Prim {
    Stroke { pts: Vec<Pos2>, closed: bool, width: f32, color: Rgb, alpha: u8 },
    /// Closed polygon fill (any shape, not only convex) with optional outline.
    Fill { pts: Vec<Pos2>, color: Rgb, alpha: u8, outline: Option<(Rgb, f32)> },
    /// One line of Helvetica text; `origin` is the left end of the baseline.
    Text { origin: Pos2, text: String, size: f32, color: Rgb, alpha: u8 },
    /// Library image stretched over `rect` (it keeps its aspect when resized).
    Image { rect: Rect, image: String, alpha: u8 },
}

pub const WHITE: Rgb = [255, 255, 255];

/// Architectural drawing scales as (real inches per paper inch, name), largest first.
pub const SCALES: [(f32, &str); 10] = [
    (12.0, "1\" = 1'-0\""),
    (16.0, "3/4\" = 1'-0\""),
    (24.0, "1/2\" = 1'-0\""),
    (32.0, "3/8\" = 1'-0\""),
    (48.0, "1/4\" = 1'-0\""),
    (64.0, "3/16\" = 1'-0\""),
    (96.0, "1/8\" = 1'-0\""),
    (128.0, "3/32\" = 1'-0\""),
    (192.0, "1/16\" = 1'-0\""),
    (384.0, "1/32\" = 1'-0\""),
];

pub fn scale_name(scale: f32) -> String {
    SCALES.iter().find(|(s, _)| (*s - scale).abs() < 1e-3).map(|(_, n)| n.to_string()).unwrap_or_else(|| format!("1:{scale}"))
}

/// Real-world inches for `pt` page points on a sheet drawn at `scale`
/// (assumes the PDF is at full sheet size: 72 points to the paper inch).
pub fn real_inches(pt: f32, scale: f32) -> f32 {
    pt / 72.0 * scale
}

/// Feet and inches to the nearest 1/4": `12'-6 1/2"`.
pub fn fmt_len(inches: f32) -> String {
    let q = (inches.abs() * 4.0).round() as i64;
    let (ft, rem) = (q / 48, q % 48);
    let (whole, frac) = (rem / 4, rem % 4);
    let frac = ["", " 1/4", " 1/2", " 3/4"][frac as usize];
    format!("{ft}'-{whole}{frac}\"")
}

/// Square feet, one decimal, thousands separated: `1,234.5 SF`.
pub fn fmt_area(sq_in: f32) -> String {
    let tenths = (sq_in.abs() as f64 / 144.0 * 10.0).round() as i64;
    let (whole, dec) = (tenths / 10, tenths % 10);
    let digits = whole.to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("{grouped}.{dec} SF")
}

/// Unit normal of a→b that a dimension's `off` is measured along.
pub fn dim_normal(a: Pos2, b: Pos2) -> Vec2 {
    let d = b - a;
    if d.length() < 1e-4 {
        return Vec2::Y;
    }
    let d = d.normalized();
    vec2(-d.y, d.x)
}

pub fn poly_len(pts: &[Pos2], closed: bool) -> f32 {
    let open: f32 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    match (closed, pts.first(), pts.last()) {
        (true, Some(f), Some(l)) if pts.len() > 2 => open + (*f - *l).length(),
        _ => open,
    }
}

/// Shoelace area (always positive).
pub fn poly_area(pts: &[Pos2]) -> f32 {
    if pts.len() < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..pts.len() {
        let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
        s += p.x * q.y - q.x * p.y;
    }
    (s / 2.0).abs()
}

/// Area centroid, or the vertex average for a degenerate polygon.
fn centroid(pts: &[Pos2]) -> Pos2 {
    let avg = || pts.iter().fold(Vec2::ZERO, |s, p| s + p.to_vec2()) / pts.len().max(1) as f32;
    let (mut a, mut c) = (0.0, Vec2::ZERO);
    for i in 0..pts.len() {
        let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
        let cross = p.x * q.y - q.x * p.y;
        a += cross;
        c += (p.to_vec2() + q.to_vec2()) * cross;
    }
    if a.abs() < 1e-3 {
        return avg().to_pos2();
    }
    (c / (3.0 * a)).to_pos2()
}

// ---------------------------------------------------------------------------
// Text layout (Helvetica metrics; Liberation Sans on screen is metric-compatible)

/// Helvetica AFM advance widths for ASCII 32..=126, in 1/1000 em.
const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // ' '..'/'
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // 0-9
    278, 278, 584, 584, 584, 556, 1015, // :;<=>?@
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, // A-M
    722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, // N-Z
    278, 278, 278, 469, 556, 333, // [\]^_`
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, // a-m
    556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, // n-z
    334, 260, 334, 584, // {|}~
];

pub fn char_width(c: char, size: f32) -> f32 {
    let w = match c as u32 {
        32..=126 => HELV[(c as u32 - 32) as usize] as f32,
        _ => crate::fonts::advance(c).unwrap_or(556.0),
    };
    w * size / 1000.0
}

pub fn text_width(s: &str, size: f32) -> f32 {
    s.chars().map(|c| char_width(c, size)).sum()
}

/// Greedy word wrap. Explicit newlines are kept; over-long words are broken.
pub fn wrap(text: &str, size: f32, max_w: f32) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if text_width(&candidate, size) <= max_w || line.is_empty() && text_width(word, size) <= max_w {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            // Break a word that alone is too wide.
            for c in word.chars() {
                if !line.is_empty() && text_width(&line, size) + char_width(c, size) > max_w {
                    out.push(std::mem::take(&mut line));
                }
                line.push(c);
            }
        }
        out.push(line);
    }
    out
}

pub fn pad(size: f32) -> f32 {
    0.45 * size
}
pub fn line_h(size: f32) -> f32 {
    1.2 * size
}
/// Distance from a line's top to its baseline.
pub fn ascent(size: f32) -> f32 {
    0.88 * size
}

pub fn text_lines(text: &str, size: f32, box_w: f32) -> Vec<String> {
    wrap(text, size, (box_w - 2.0 * pad(size)).max(size))
}

pub fn box_rect(pos: Pos2, box_w: f32, text: &str, size: f32) -> Rect {
    let n = text_lines(text, size, box_w).len().max(1) as f32;
    Rect::from_min_size(pos, vec2(box_w, 2.0 * pad(size) + n * line_h(size)))
}

pub fn default_box_w(size: f32) -> f32 {
    16.0 * size
}

// ---------------------------------------------------------------------------

impl Annot {
    /// Rect of the text box for Callout/Text kinds.
    pub fn text_box(&self) -> Option<Rect> {
        match &self.kind {
            Kind::Callout { pos, box_w, text, size, .. } | Kind::Text { pos, box_w, text, size } => {
                Some(box_rect(*pos, *box_w, text, *size))
            }
            _ => None,
        }
    }

    /// Change a text box's width, keeping the edge nearest a callout's target fixed.
    pub fn set_box_w(&mut self, new_w: f32) {
        match &mut self.kind {
            Kind::Callout { anchor, pos, box_w, .. } => {
                if anchor.x > pos.x + *box_w {
                    pos.x += *box_w - new_w;
                }
                *box_w = new_w;
            }
            Kind::Text { box_w, .. } => *box_w = new_w,
            _ => {}
        }
    }

    /// Shrink a text box to its widest wrapped line.
    pub fn fit_box_w(&mut self) {
        let (Some(r), Some(size)) = (self.text_box(), self.text_size()) else { return };
        let text = match &self.kind {
            Kind::Callout { text, .. } | Kind::Text { text, .. } => text.clone(),
            _ => return,
        };
        let widest = text_lines(&text, size, r.width()).iter().map(|l| text_width(l.trim_end(), size)).fold(0.0, f32::max);
        // A little slack so float rounding never re-wraps the last word.
        let fit = (widest + 2.0 * pad(size) + 0.5).max(4.0 * size);
        if fit < r.width() {
            self.set_box_w(fit);
        }
    }

    /// Give a box room to grow while typing (it is shrunk to fit again on commit).
    pub fn widen_for_edit(&mut self) {
        if let (Some(r), Some(size)) = (self.text_box(), self.text_size()) {
            let w = self.wrap_w.unwrap_or_else(|| default_box_w(size));
            if r.width() < w {
                self.set_box_w(w);
            }
        }
    }

    /// What lists and exported comments call it: the note, the image name, or the kind.
    pub fn label(&self) -> String {
        if let Some(m) = self.measure_text() {
            return format!("{} {m}", self.kind_name());
        }
        match &self.kind {
            Kind::Image { name, .. } if !name.trim().is_empty() => name.clone(),
            _ if !self.text().trim().is_empty() => self.text().to_string(),
            _ => self.kind_name().to_string(),
        }
    }

    /// A measurement's value as shown on the sheet (None for other markups).
    pub fn measure_text(&self) -> Option<String> {
        Some(match &self.kind {
            Kind::Distance { a, b, scale, .. } => fmt_len(real_inches((*b - *a).length(), *scale)),
            Kind::Perimeter { pts, closed, scale, .. } => fmt_len(real_inches(poly_len(pts, *closed), *scale)),
            Kind::Area { pts, scale, .. } => {
                let k = real_inches(1.0, *scale);
                fmt_area(poly_area(pts) * k * k)
            }
            _ => return None,
        })
    }

    /// A measurement's drawing scale.
    pub fn scale(&self) -> Option<f32> {
        match &self.kind {
            Kind::Distance { scale, .. } | Kind::Area { scale, .. } | Kind::Perimeter { scale, .. } => Some(*scale),
            _ => None,
        }
    }

    /// Text size of a note or a measurement's label.
    pub fn label_size_mut(&mut self) -> Option<&mut f32> {
        match &mut self.kind {
            Kind::Callout { size, .. } | Kind::Text { size, .. } | Kind::Distance { size, .. } | Kind::Area { size, .. } | Kind::Perimeter { size, .. } => Some(size),
            _ => None,
        }
    }

    /// Note text ("" for shapes).
    pub fn text(&self) -> &str {
        match &self.kind {
            Kind::Callout { text, .. } | Kind::Text { text, .. } => text,
            _ => "",
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            Kind::Callout { .. } => "Callout",
            Kind::Text { .. } => "Text",
            Kind::Cloud { .. } => "Cloud",
            Kind::Rect { .. } => "Rectangle",
            Kind::Highlight { .. } => "Highlight",
            Kind::Arrow { .. } => "Arrow",
            Kind::Line { .. } => "Line",
            Kind::Pen { .. } => "Pen",
            Kind::Image { .. } => "Stamp",
            Kind::Distance { .. } => "Distance",
            Kind::Area { .. } => "Area",
            Kind::Perimeter { .. } => "Perimeter",
        }
    }

    pub fn text_mut(&mut self) -> Option<&mut String> {
        match &mut self.kind {
            Kind::Callout { text, .. } | Kind::Text { text, .. } => Some(text),
            _ => None,
        }
    }

    pub fn text_size(&self) -> Option<f32> {
        match &self.kind {
            Kind::Callout { size, .. } | Kind::Text { size, .. } => Some(*size),
            _ => None,
        }
    }

    pub fn bounds(&self) -> Rect {
        let w = self.width.max(1.0);
        match &self.kind {
            // The leader lies between box and anchor; the arrowhead spreads up to arrow_len around the anchor.
            Kind::Callout { anchor, .. } => self.text_box().unwrap().union(Rect::from_center_size(*anchor, Vec2::splat(2.0 * arrow_len(w)))),
            Kind::Text { .. } => self.text_box().unwrap(),
            Kind::Cloud { rect } => rect.expand(cloud_radius(*rect) + w),
            Kind::Rect { rect } | Kind::Highlight { rect } => rect.expand(w),
            Kind::Image { rect, .. } => *rect,
            Kind::Arrow { a, b } | Kind::Line { a, b } => Rect::from_two_pos(*a, *b).expand(arrow_len(w) + w),
            Kind::Pen { pts } => {
                let mut r = Rect::NOTHING;
                for p in pts {
                    r.extend_with(*p);
                }
                r.expand(w)
            }
            Kind::Distance { .. } | Kind::Area { .. } | Kind::Perimeter { .. } => {
                // The measured points too: a dimension's extension lines stop short of them.
                let mut r = match &self.kind {
                    Kind::Distance { a, b, .. } => Rect::from_two_pos(*a, *b),
                    _ => Rect::NOTHING,
                };
                for prim in self.prims() {
                    if let Prim::Stroke { pts, .. } | Prim::Fill { pts, .. } = prim {
                        pts.iter().for_each(|p| r.extend_with(*p));
                    }
                }
                r.expand(w)
            }
        }
    }

    pub fn translate(&mut self, d: Vec2) {
        match &mut self.kind {
            Kind::Callout { pos, .. } | Kind::Text { pos, .. } => *pos += d,
            Kind::Cloud { rect } | Kind::Rect { rect } | Kind::Highlight { rect } | Kind::Image { rect, .. } => *rect = rect.translate(d),
            Kind::Arrow { a, b } | Kind::Line { a, b } => {
                *a += d;
                *b += d;
            }
            Kind::Pen { pts } | Kind::Area { pts, .. } | Kind::Perimeter { pts, .. } => pts.iter_mut().for_each(|p| *p += d),
            Kind::Distance { a, b, .. } => {
                *a += d;
                *b += d;
            }
        }
    }

    /// Distance (view points) from `p` to the annotation; 0 when inside a filled area.
    pub fn distance(&self, p: Pos2) -> f32 {
        if let Some(r) = self.text_box() {
            if r.contains(p) {
                return 0.0;
            }
        }
        if let Kind::Highlight { rect } | Kind::Image { rect, .. } = &self.kind {
            if rect.contains(p) {
                return 0.0;
            }
        }
        let mut best = f32::INFINITY;
        for prim in self.prims() {
            let (pts, closed) = match &prim {
                Prim::Stroke { pts, closed, .. } => (pts, *closed),
                Prim::Fill { pts, .. } => (pts, true),
                Prim::Image { rect, .. } => {
                    best = best.min(polyline_dist(&rect_pts(*rect), true, p));
                    continue;
                }
                Prim::Text { .. } => continue,
            };
            best = best.min(polyline_dist(pts, closed, p));
        }
        (best - self.width / 2.0).max(0.0)
    }

    /// Whether a marquee `r` crosses the markup: its drawn lines, text box or
    /// highlight. Like clicking, the empty inside of a rectangle doesn't count.
    pub fn touches(&self, r: Rect) -> bool {
        if !self.bounds().intersects(r) {
            return false;
        }
        if self.text_box().is_some_and(|b| b.intersects(r)) {
            return true;
        }
        if let Kind::Highlight { rect } | Kind::Image { rect, .. } = &self.kind {
            if rect.intersects(r) {
                return true;
            }
        }
        let r = r.expand(self.width / 2.0);
        self.prims().iter().any(|prim| match prim {
            Prim::Stroke { pts, closed, .. } => polyline_hits(pts, *closed, r),
            Prim::Fill { pts, .. } => polyline_hits(pts, true, r),
            Prim::Text { .. } | Prim::Image { .. } => false,
        })
    }

    pub fn prims(&self) -> Vec<Prim> {
        let mut out = self.opaque_prims();
        if self.look.opacity < 1.0 {
            fade(&mut out, self.look.opacity);
        }
        out
    }

    /// The chosen fill as a prim over `pts`, if this markup has one.
    fn fill_prim(&self, pts: Vec<Pos2>) -> Option<Prim> {
        let f = self.look.fill.filter(|_| self.kind.fills())?;
        Some(Prim::Fill { pts, color: f, alpha: a8(self.look.fill_opacity), outline: None })
    }

    fn opaque_prims(&self) -> Vec<Prim> {
        let c = self.color;
        let w = self.width;
        let mut out = Vec::new();
        match &self.kind {
            Kind::Callout { anchor, text, size, .. } => {
                let r = self.text_box().unwrap();
                if !r.contains(*anchor) {
                    // Leader leaves the box square to its side for a short landing, then bends to the target.
                    let (start, normal) = leader_start(r, *anchor);
                    let room = (*anchor - start).dot(normal) - arrow_len(w) - 1.0;
                    let knee = start + normal * landing(*size).min(room.max(0.0));
                    arrow_path_prims(&mut out, &[start, knee], *anchor, w, c);
                }
                // White unless a fill is chosen.
                let (fc, fa) = self.look.fill.map(|f| (f, a8(self.look.fill_opacity))).unwrap_or((WHITE, 255));
                out.push(Prim::Fill { pts: rect_pts(r), color: fc, alpha: fa, outline: Some((c, w)) });
                text_prims(&mut out, r, text, *size, c);
            }
            Kind::Text { text, size, .. } => {
                let r = self.text_box().unwrap();
                out.extend(self.fill_prim(rect_pts(r)));
                text_prims(&mut out, r, text, *size, c);
            }
            Kind::Cloud { rect } => {
                let pts = cloud_pts(*rect);
                out.extend(self.fill_prim(pts.clone()));
                out.push(Prim::Stroke { pts, closed: true, width: w, color: c, alpha: 255 });
            }
            Kind::Rect { rect } => {
                out.extend(self.fill_prim(rect_pts(*rect)));
                out.push(Prim::Stroke { pts: rect_pts(*rect), closed: true, width: w, color: c, alpha: 255 });
            }
            Kind::Highlight { rect } => out.push(Prim::Fill { pts: rect_pts(*rect), color: c, alpha: 90, outline: None }),
            Kind::Arrow { a, b } => arrow_prims(&mut out, *a, *b, w, c),
            Kind::Line { a, b } => out.push(Prim::Stroke { pts: vec![*a, *b], closed: false, width: w, color: c, alpha: 255 }),
            Kind::Pen { pts } => out.push(Prim::Stroke { pts: pts.clone(), closed: false, width: w, color: c, alpha: 255 }),
            Kind::Image { rect, image, .. } => out.push(Prim::Image { rect: *rect, image: image.clone(), alpha: 255 }),
            Kind::Distance { a, b, off, size, .. } => {
                let n = dim_normal(*a, *b);
                let side = if *off < 0.0 { -1.0 } else { 1.0 };
                let (p1, p2) = (*a + n * *off, *b + n * *off);
                // Extension lines: a small gap off the measured points, running a little past the dimension line.
                let (gap, over) = (2.0, 3.0 + w);
                if off.abs() > gap {
                    for (from, to) in [(*a, p1), (*b, p2)] {
                        out.push(Prim::Stroke { pts: vec![from + n * side * gap, to + n * side * over], closed: false, width: w * 0.6, color: c, alpha: 255 });
                    }
                }
                out.push(Prim::Stroke { pts: vec![p1, p2], closed: false, width: w, color: c, alpha: 255 });
                // Architectural ticks at both ends.
                let dir = if (*b - *a).length() > 1e-4 { (*b - *a).normalized() } else { Vec2::X };
                let t = (dir + n).normalized() * (3.0 + 1.5 * w);
                for p in [p1, p2] {
                    out.push(Prim::Stroke { pts: vec![p - t, p + t], closed: false, width: w * 1.5, color: c, alpha: 255 });
                }
                let text = self.measure_text().unwrap_or_default();
                label_prims(&mut out, p1 + (p2 - p1) * 0.5, n * side, &text, *size, c);
            }
            Kind::Area { pts, size, .. } => {
                out.extend(self.fill_prim(pts.clone()));
                out.push(Prim::Stroke { pts: pts.clone(), closed: true, width: w, color: c, alpha: 255 });
                let text = self.measure_text().unwrap_or_default();
                label_prims(&mut out, centroid(pts), Vec2::ZERO, &text, *size, c);
            }
            Kind::Perimeter { pts, closed, size, .. } => {
                if *closed {
                    out.extend(self.fill_prim(pts.clone()));
                }
                out.push(Prim::Stroke { pts: pts.clone(), closed: *closed, width: w, color: c, alpha: 255 });
                let text = self.measure_text().unwrap_or_default();
                if *closed && pts.len() > 2 {
                    label_prims(&mut out, centroid(pts), Vec2::ZERO, &text, *size, c);
                } else if let Some(seg) = pts.windows(2).max_by(|x, y| (x[1] - x[0]).length().total_cmp(&(y[1] - y[0]).length())) {
                    // Beside the longest run, like a dimension.
                    label_prims(&mut out, seg[0] + (seg[1] - seg[0]) * 0.5, dim_normal(seg[0], seg[1]), &text, *size, c);
                }
            }
        }
        out
    }
}

/// Scales every prim's alpha by `opacity`. A fill's outline becomes its own
/// stroke so it fades too.
fn fade(prims: &mut Vec<Prim>, opacity: f32) {
    let k = |a: u8| a8(a as f32 / 255.0 * opacity);
    let mut out = Vec::with_capacity(prims.len());
    for p in prims.drain(..) {
        match p {
            Prim::Fill { pts, color, alpha, outline } => {
                let outline = outline.map(|(oc, w)| Prim::Stroke { pts: pts.clone(), closed: true, width: w, color: oc, alpha: k(255) });
                out.push(Prim::Fill { pts, color, alpha: k(alpha), outline: None });
                out.extend(outline);
            }
            Prim::Stroke { pts, closed, width, color, alpha } => out.push(Prim::Stroke { pts, closed, width, color, alpha: k(alpha) }),
            Prim::Text { origin, text, size, color, alpha } => out.push(Prim::Text { origin, text, size, color, alpha: k(alpha) }),
            Prim::Image { rect, image, alpha } => out.push(Prim::Image { rect, image, alpha: k(alpha) }),
        }
    }
    *prims = out;
}

impl Kind {
    /// Whether a fill colour applies (callouts always have one: white by default).
    pub fn fills(&self) -> bool {
        matches!(self, Kind::Callout { .. } | Kind::Text { .. } | Kind::Cloud { .. } | Kind::Rect { .. } | Kind::Area { .. } | Kind::Perimeter { closed: true, .. })
    }
}

/// Triangles (index triples) covering a simple polygon, by ear clipping.
/// Used to fill shapes on screen that aren't convex (clouds, areas).
pub fn triangulate(pts: &[Pos2]) -> Vec<[usize; 3]> {
    let mut idx: Vec<usize> = (0..pts.len()).collect();
    idx.dedup_by(|a, b| (pts[*a] - pts[*b]).length() < 1e-4);
    if idx.len() < 3 {
        return Vec::new();
    }
    let area2: f32 = (0..idx.len()).map(|i| {
        let (p, q) = (pts[idx[i]], pts[idx[(i + 1) % idx.len()]]);
        p.x * q.y - q.x * p.y
    }).sum();
    let ccw = area2 > 0.0;
    let cross = |a: Pos2, b: Pos2, c: Pos2| (b - a).x * (c - a).y - (b - a).y * (c - a).x;
    let convex = |a, b, c| if ccw { cross(a, b, c) > 0.0 } else { cross(a, b, c) < 0.0 };
    let inside = |p: Pos2, a, b, c| {
        let (d1, d2, d3) = (cross(a, b, p), cross(b, c, p), cross(c, a, p));
        let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
        let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
        !(neg && pos)
    };
    let mut out = Vec::with_capacity(idx.len());
    // Each pass clips one vertex, so this loop runs at most len - 3 times.
    while idx.len() > 3 {
        let n = idx.len();
        let ear = (0..n).find(|&i| {
            let (ia, ib, ic) = (idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]);
            let (a, b, c) = (pts[ia], pts[ib], pts[ic]);
            convex(a, b, c) && idx.iter().all(|&j| j == ia || j == ib || j == ic || !inside(pts[j], a, b, c))
        });
        // Self-touching or degenerate input: clip anyway rather than loop forever.
        let i = ear.unwrap_or(0);
        out.push([idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]]);
        idx.remove(i);
    }
    if idx.len() == 3 {
        out.push([idx[0], idx[1], idx[2]]);
    }
    out
}

/// A measurement's value on a white plate centred at `at`, pushed along `away`
/// (a unit vector, or zero to stay centred) until the plate clears `at`.
fn label_prims(out: &mut Vec<Prim>, at: Pos2, away: Vec2, text: &str, size: f32, color: Rgb) {
    let p = 0.25 * size;
    let half = vec2(text_width(text, size) / 2.0 + p, line_h(size) / 2.0 + p * 0.5);
    let reach = away.x.abs() * half.x + away.y.abs() * half.y;
    let center = at + away * (reach + 1.0);
    let r = Rect::from_center_size(center, half * 2.0);
    out.push(Prim::Fill { pts: rect_pts(r), color: WHITE, alpha: 255, outline: None });
    let origin = pos2(r.min.x + p, r.min.y + p * 0.5 + (line_h(size) - size) / 2.0 + ascent(size));
    out.push(Prim::Text { origin, text: text.to_string(), size, color, alpha: 255 });
}

fn text_prims(out: &mut Vec<Prim>, r: Rect, text: &str, size: f32, color: Rgb) {
    let x = r.min.x + pad(size);
    for (i, line) in text_lines(text, size, r.width()).into_iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let y = r.min.y + pad(size) + i as f32 * line_h(size) + ascent(size);
        out.push(Prim::Text { origin: pos2(x, y), text: line, size, color, alpha: 255 });
    }
}

/// Rect with one corner at `fixed`, reaching towards `p`, with width/height
/// `aspect`: the larger of the two drags wins.
pub fn aspect_rect(fixed: Pos2, p: Pos2, aspect: f32) -> Rect {
    let d = p - fixed;
    let (mut w, mut h) = (d.x.abs(), d.y.abs());
    if w > h * aspect {
        h = w / aspect;
    } else {
        w = h * aspect;
    }
    let (w, h) = if w < 4.0 { (4.0, 4.0 / aspect) } else { (w, h) };
    Rect::from_two_pos(fixed, fixed + vec2(d.x.signum() * w, d.y.signum() * h))
}

pub fn rect_pts(r: Rect) -> Vec<Pos2> {
    vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()]
}

pub fn arrow_len(width: f32) -> f32 {
    6.0 + 3.5 * width
}

/// Line from `from` to `tip` with a filled head at `tip`.
fn arrow_prims(out: &mut Vec<Prim>, from: Pos2, tip: Pos2, w: f32, c: Rgb) {
    arrow_path_prims(out, &[from], tip, w, c);
}

/// Polyline through `path` ending in a filled arrowhead at `tip`.
fn arrow_path_prims(out: &mut Vec<Prim>, path: &[Pos2], tip: Pos2, w: f32, c: Rgb) {
    // Head direction from the last path point that isn't the tip itself.
    let Some(&from) = path.iter().rev().find(|p| (tip - **p).length() >= 0.01) else { return };
    let d = tip - from;
    let len = d.length();
    let dir = d / len;
    let n = vec2(-dir.y, dir.x);
    let hl = arrow_len(w).min(len);
    let base = tip - dir * hl;
    let mut pts = path.to_vec();
    pts.dedup_by(|a, b| (*a - *b).length() < 0.01);
    pts.push(base + dir * (w * 0.5));
    out.push(Prim::Stroke { pts, closed: false, width: w, color: c, alpha: 255 });
    let hw = hl * 0.38;
    out.push(Prim::Fill { pts: vec![tip, base + n * hw, base - n * hw], color: c, alpha: 255, outline: None });
}

/// Length of the straight landing segment on a callout leader.
pub fn landing(size: f32) -> f32 {
    2.0 * size
}

/// Middle of the box side facing the anchor, and that side's outward normal.
/// Left/right sides are used unless the anchor sits within the box's width.
fn leader_start(r: Rect, p: Pos2) -> (Pos2, Vec2) {
    let c = r.center();
    if p.x > r.max.x {
        (pos2(r.max.x, c.y), Vec2::X)
    } else if p.x < r.min.x {
        (pos2(r.min.x, c.y), -Vec2::X)
    } else if p.y > c.y {
        (pos2(c.x, r.max.y), Vec2::Y)
    } else {
        (pos2(c.x, r.min.y), -Vec2::Y)
    }
}

pub fn cloud_radius(rect: Rect) -> f32 {
    (rect.width().min(rect.height()) / 6.0).clamp(4.0, 12.0)
}

/// Scalloped outline: perimeter split into chords, each replaced by an outward arc.
pub fn cloud_pts(rect: Rect) -> Vec<Pos2> {
    let r = rect;
    let chord = cloud_radius(r) * 2.0;
    let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
    let mut perim = Vec::new();
    for i in 0..4 {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        let n = ((b - a).length() / chord).round().max(1.0) as usize;
        for k in 0..n {
            perim.push(a + (b - a) * (k as f32 / n as f32));
        }
    }
    let mut pts = Vec::new();
    let center = r.center();
    for i in 0..perim.len() {
        let (a, b) = (perim[i], perim[(i + 1) % perim.len()]);
        let mid = a + (b - a) * 0.5;
        let half = (b - a).length() / 2.0;
        if half < 0.01 {
            continue;
        }
        let mut n = vec2(-(b - a).y, (b - a).x).normalized();
        if n.dot(mid - center) < 0.0 {
            n = -n;
        }
        // Arc through a and b with sagitta s (bulging outward).
        let s = half * 0.8;
        let rad = (half * half + s * s) / (2.0 * s);
        let cc = mid + n * (s - rad);
        let a0 = (a - cc).angle();
        let mut a1 = (b - cc).angle();
        // Sweep the short way round through the bulge.
        let mut sweep = a1 - a0;
        while sweep > std::f32::consts::PI {
            sweep -= std::f32::consts::TAU;
        }
        while sweep < -std::f32::consts::PI {
            sweep += std::f32::consts::TAU;
        }
        let mid_ang = a0 + sweep / 2.0;
        let probe = cc + Vec2::angled(mid_ang) * rad;
        if (probe - mid).dot(n) < 0.0 {
            sweep = if sweep > 0.0 { sweep - std::f32::consts::TAU } else { sweep + std::f32::consts::TAU };
        }
        a1 = a0 + sweep;
        let steps = 10;
        for k in 0..steps {
            let t = a0 + (a1 - a0) * (k as f32 / steps as f32);
            pts.push(cc + Vec2::angled(t) * rad);
        }
    }
    pts
}

fn seg_dist(a: Pos2, b: Pos2, p: Pos2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq();
    if l2 < 1e-6 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

fn polyline_dist(pts: &[Pos2], closed: bool, p: Pos2) -> f32 {
    let mut best = f32::INFINITY;
    if pts.len() == 1 {
        return (p - pts[0]).length();
    }
    for w in pts.windows(2) {
        best = best.min(seg_dist(w[0], w[1], p));
    }
    if closed && pts.len() > 2 {
        best = best.min(seg_dist(pts[pts.len() - 1], pts[0], p));
    }
    best
}

/// Liang–Barsky: does segment `a`–`b` pass through `r`?
fn seg_hits(a: Pos2, b: Pos2, r: Rect) -> bool {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [(-d.x, a.x - r.min.x), (d.x, r.max.x - a.x), (-d.y, a.y - r.min.y), (d.y, r.max.y - a.y)] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else if p < 0.0 {
            t0 = t0.max(q / p);
        } else {
            t1 = t1.min(q / p);
        }
    }
    t0 <= t1
}

fn polyline_hits(pts: &[Pos2], closed: bool, r: Rect) -> bool {
    if pts.len() == 1 {
        return r.contains(pts[0]);
    }
    pts.windows(2).any(|w| seg_hits(w[0], w[1], r)) || (closed && pts.len() > 2 && seg_hits(pts[pts.len() - 1], pts[0], r))
}

/// Ramer–Douglas–Peucker simplification for pen strokes.
pub fn simplify(pts: &[Pos2], eps: f32) -> Vec<Pos2> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let (mut idx, mut dmax) = (0, 0.0);
    for (i, p) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
        let d = seg_dist(a, b, *p);
        if d > dmax {
            idx = i;
            dmax = d;
        }
    }
    if dmax > eps {
        let mut left = simplify(&pts[..=idx], eps);
        let right = simplify(&pts[idx..], eps);
        left.pop();
        left.extend(right);
        left
    } else {
        vec![a, b]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_breaks_on_width() {
        let lines = wrap("check this dimension against the structural set please", 12.0, 120.0);
        assert!(lines.len() > 1);
        for l in &lines {
            assert!(text_width(l, 12.0) <= 120.0, "{l}");
        }
        assert_eq!(wrap("a\n\nb", 12.0, 100.0), vec!["a", "", "b"]);
    }

    #[test]
    fn fit_box_shrinks_and_keeps_callout_edge() {
        let mut a = Annot { id: 1, page: 0, color: [0, 0, 0], width: 2.0, wrap_w: None, group: None, look: Look::default(), kind: Kind::Callout { anchor: pos2(500.0, 0.0), pos: pos2(100.0, 0.0), box_w: 300.0, text: "Short".into(), size: 12.0 } };
        let right = a.text_box().unwrap().max.x;
        a.fit_box_w();
        let r = a.text_box().unwrap();
        assert!(r.width() < 100.0);
        assert!((r.max.x - right).abs() < 0.01, "edge facing the anchor stays put");
        assert_eq!(text_lines("Short", 12.0, r.width()).len(), 1);
        a.widen_for_edit();
        assert!((a.text_box().unwrap().width() - default_box_w(12.0)).abs() < 0.01);
    }

    #[test]
    fn marquee_touches_lines_not_empty_space() {
        let ann = |kind| Annot { id: 1, page: 0, color: [0, 0, 0], width: 2.0, wrap_w: None, group: None, look: Look::default(), kind };
        let box_at = |x: f32, y: f32| Rect::from_min_size(pos2(x, y), vec2(10.0, 10.0));
        let diag = ann(Kind::Line { a: pos2(0.0, 0.0), b: pos2(100.0, 100.0) });
        assert!(diag.touches(box_at(45.0, 45.0)));
        assert!(!diag.touches(box_at(80.0, 5.0)), "inside the bounding box but off the line");
        assert!(diag.touches(Rect::from_min_max(pos2(-10.0, -10.0), pos2(200.0, 200.0))), "fully enclosed");
        let rect = ann(Kind::Rect { rect: Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0)) });
        assert!(!rect.touches(box_at(45.0, 45.0)), "empty interior");
        assert!(rect.touches(box_at(95.0, 45.0)), "crosses the edge");
        let hl = ann(Kind::Highlight { rect: Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 20.0)) });
        assert!(hl.touches(box_at(45.0, 5.0)));
        let note = ann(Kind::Text { pos: pos2(200.0, 200.0), box_w: 100.0, text: "Check".into(), size: 12.0 });
        assert!(note.touches(box_at(220.0, 205.0)));
        assert!(!note.touches(box_at(0.0, 300.0)));
    }

    #[test]
    fn aspect_rect_keeps_shape_and_corner() {
        let r = aspect_rect(pos2(100.0, 100.0), pos2(40.0, 90.0), 2.0);
        assert_eq!(r.max, pos2(100.0, 100.0), "fixed corner stays");
        assert!((r.width() - 60.0).abs() < 1e-3 && (r.height() - 30.0).abs() < 1e-3, "{r:?}");
        let r = aspect_rect(pos2(0.0, 0.0), pos2(10.0, 50.0), 2.0);
        assert!((r.width() - 100.0).abs() < 1e-3 && (r.height() - 50.0).abs() < 1e-3, "{r:?}");
    }

    #[test]
    fn image_hits_inside_and_labels_by_name() {
        let a = Annot { id: 1, page: 0, color: [0, 0, 0], width: 2.0, wrap_w: None, group: None, look: Look::default(), kind: Kind::Image { rect: Rect::from_min_size(pos2(10.0, 10.0), vec2(50.0, 50.0)), image: "x.png".into(), name: "NC seal".into() } };
        assert_eq!(a.distance(pos2(30.0, 30.0)), 0.0);
        assert!((a.distance(pos2(100.0, 30.0)) - 39.0).abs() < 1e-3, "outline distance less half the line width");
        assert!(a.touches(Rect::from_min_size(pos2(20.0, 20.0), vec2(5.0, 5.0))));
        assert_eq!(a.bounds(), Rect::from_min_size(pos2(10.0, 10.0), vec2(50.0, 50.0)));
        assert_eq!(a.label(), "NC seal");
    }

    #[test]
    fn lengths_in_feet_and_inches() {
        assert_eq!(fmt_len(0.0), "0'-0\"");
        assert_eq!(fmt_len(150.5), "12'-6 1/2\"");
        assert_eq!(fmt_len(11.9), "1'-0\"", "rounds up into the next foot");
        assert_eq!(fmt_len(3.26), "0'-3 1/4\"");
        assert_eq!(fmt_area(144.0 * 1234.54), "1,234.5 SF");
        assert_eq!(fmt_area(144.0 * 12.0), "12.0 SF");
    }

    #[test]
    fn measurements_use_their_scale() {
        let ann = |kind| Annot { id: 1, page: 0, color: [0, 0, 0], width: 1.0, wrap_w: None, group: None, look: Look::default(), kind };
        // 1/4" = 1'-0": one paper inch (72 pt) is 4 feet.
        let d = ann(Kind::Distance { a: pos2(0.0, 0.0), b: pos2(72.0, 0.0), off: -20.0, scale: 48.0, size: 10.0 });
        assert_eq!(d.measure_text().unwrap(), "4'-0\"");
        assert_eq!(d.label(), "Distance 4'-0\"");
        // 1/8": a 1 x 2 inch rectangle is 8' x 16' = 128 SF.
        let sq = vec![pos2(0.0, 0.0), pos2(72.0, 0.0), pos2(72.0, 144.0), pos2(0.0, 144.0)];
        let a = ann(Kind::Area { pts: sq.clone(), scale: 96.0, size: 10.0 });
        assert_eq!(a.measure_text().unwrap(), "128.0 SF");
        let p = ann(Kind::Perimeter { pts: sq.clone(), closed: true, scale: 96.0, size: 10.0 });
        assert_eq!(p.measure_text().unwrap(), "48'-0\"");
        let open = ann(Kind::Perimeter { pts: sq, closed: false, scale: 96.0, size: 10.0 });
        assert_eq!(open.measure_text().unwrap(), "32'-0\"");
        // The dimension line sits at the offset and is what a click hits.
        assert!(d.distance(pos2(36.0, -20.0)) < 0.01);
        assert!(d.bounds().contains(pos2(36.0, -20.0)) && d.bounds().contains(pos2(0.0, 0.0)));
    }

    #[test]
    fn fill_and_opacity_reach_the_prims() {
        let mut a = Annot { id: 1, page: 0, color: [228, 0, 0], width: 2.0, wrap_w: None, group: None, look: Look::default(), kind: Kind::Rect { rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(50.0, 50.0)) } };
        assert_eq!(a.prims().len(), 1, "no fill by default");
        a.look = Look { opacity: 0.5, fill: Some([255, 214, 0]), fill_opacity: 0.4 };
        let p = a.prims();
        assert!(matches!(p[0], Prim::Fill { color: [255, 214, 0], alpha: 51, .. }), "{p:?}");
        assert!(matches!(p[1], Prim::Stroke { alpha: 128, .. }));
        // Lines don't take a fill.
        a.kind = Kind::Line { a: pos2(0.0, 0.0), b: pos2(9.0, 9.0) };
        assert_eq!(a.prims().len(), 1);
        // Callouts: fill replaces the white box, and the outline fades with the rest.
        a.kind = Kind::Callout { anchor: pos2(0.0, 0.0), pos: pos2(100.0, 0.0), box_w: 100.0, text: "x".into(), size: 12.0 };
        let p = a.prims();
        assert!(p.iter().any(|p| matches!(p, Prim::Fill { color: [255, 214, 0], .. })));
        assert!(p.iter().all(|p| !matches!(p, Prim::Fill { outline: Some(_), .. })));
        assert!(p.iter().any(|p| matches!(p, Prim::Text { alpha: 128, .. })));
    }

    #[test]
    fn triangulates_concave_shapes() {
        let l = [pos2(0.0, 0.0), pos2(20.0, 0.0), pos2(20.0, 10.0), pos2(10.0, 10.0), pos2(10.0, 20.0), pos2(0.0, 20.0)];
        let t = triangulate(&l);
        assert_eq!(t.len(), 4);
        let area: f32 = t.iter().map(|[a, b, c]| poly_area(&[l[*a], l[*b], l[*c]])).sum();
        assert!((area - 300.0).abs() < 1e-3, "{area}");
        let cloud = cloud_pts(Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 60.0)));
        let t = triangulate(&cloud);
        let area: f32 = t.iter().map(|[a, b, c]| poly_area(&[cloud[*a], cloud[*b], cloud[*c]])).sum();
        assert_eq!(t.len(), cloud.len() - 2, "every vertex clipped");
        assert!((area - poly_area(&cloud)).abs() < 1.0, "{area} vs {}", poly_area(&cloud));
        let outer = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 60.0)).expand(cloud_radius(Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 60.0))));
        assert!(t.iter().all(|[a, b, c]| outer.contains(((cloud[*a].to_vec2() + cloud[*b].to_vec2() + cloud[*c].to_vec2()) / 3.0).to_pos2())));
    }

    #[test]
    fn cloud_is_closed_and_outside() {
        let r = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 60.0));
        let pts = cloud_pts(r);
        assert!(pts.len() > 20);
        // Scallops bulge outward: nothing meaningfully inside the rect's interior.
        for p in &pts {
            assert!(!r.shrink(1.0).contains(*p), "{p:?}");
        }
    }
}
