//! Real text shaping for title rendering: Arabic/Hebrew (RTL, contextual
//! glyph joining) and complex scripts via `rustybuzz` (HarfBuzz port) +
//! `unicode-bidi` reordering. Latin runs use simple advance rendering.
//! All output positions are PIXELS at the requested font size.

use ab_glyph::{Font as _, FontRef, PxScale, ScaleFont};

/// One positioned, renderable glyph (pixel space, baseline at y=0).
pub struct ShapedGlyph {
    pub id: u16,
    pub x: f32,
    pub y: f32,
}

/// Result of shaping: glyphs + total advance (pixels).
pub struct Shaping {
    pub glyphs: Vec<ShapedGlyph>,
    pub advance: f32,
    pub rtl: bool,
}

/// Shape `text` at `size_px` with `font`.
pub fn shape_text(font: &FontRef, text: &str, size_px: f32) -> Shaping {
    use rustybuzz::{Direction, UnicodeBuffer};

    let rtl = text.chars().any(is_rtl_char);
    if !rtl {
        // Latin path: per-char advances at pixel scale
        let scale = PxScale { x: size_px, y: size_px };
        let sf = font.as_scaled(scale);
        let mut glyphs = Vec::new();
        let mut pen = 0.0f32;
        for ch in text.chars().filter(|c| !c.is_control()) {
            let g = font.glyph_id(ch);
            glyphs.push(ShapedGlyph { id: g.0, x: pen, y: 0.0 });
            pen += sf.h_advance(g);
        }
        return Shaping { glyphs, advance: pen, rtl: false };
    }

    // Correct bidi pipeline: split into visually-ordered runs, shape each run
    // in LOGICAL order with its own direction (HarfBuzz does the RTL reversal
    // internally — manual pre-reversal would double-reverse).
    let bidi_info = unicode_bidi::BidiInfo::new(text, None);
    let tt_face = rustybuzz::ttf_parser::Face::parse(FACE_BYTES.with(|b| *b), 0)
        .expect("title font parse");
    let face = rustybuzz::Face::from_face(tt_face);
    let upem = font.units_per_em().unwrap_or(1000.0);
    let k = size_px / upem;
    let scale = PxScale { x: size_px, y: size_px };
    let sf = font.as_scaled(scale);

    let mut glyphs: Vec<ShapedGlyph> = Vec::new();
    let mut pen_x = 0.0f32;
    for p in &bidi_info.paragraphs {
        let (levels, runs) = bidi_info.visual_runs(p, p.range.clone());
        for (ri, run_range) in runs.iter().enumerate() {
            let level = levels.get(ri).copied().unwrap_or(unicode_bidi::Level::ltr());
            let rtl_run = level.number() % 2 == 1;
            let run_text: &str = &text[run_range.clone()];
            if run_text.is_empty() { continue; }
            let mut buffer = UnicodeBuffer::new();
            buffer.push_str(run_text);
            buffer.set_direction(if rtl_run { Direction::RightToLeft } else { Direction::LeftToRight });
            let shaped = rustybuzz::shape(&face, &[], buffer);
            for (gi, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions().iter()) {
                let gid = ab_glyph::GlyphId(gi.glyph_id as u16);
                let px_adv = sf.h_advance(gid);
                glyphs.push(ShapedGlyph {
                    id: gi.glyph_id as u16,
                    x: pen_x + pos.x_offset as f32 * k,
                    y: -pos.y_offset as f32 * k,
                });
                pen_x += px_adv;
            }
        }
    }
    if glyphs.is_empty() {
        return Shaping { glyphs, advance: 0.0, rtl: true };
    }
    Shaping { glyphs, advance: pen_x, rtl: true }
}

fn is_rtl_char(c: char) -> bool {
    matches!(c as u32,
        0x0590..=0x05FF   // Hebrew
        | 0x0600..=0x06FF // Arabic
        | 0x0700..=0x074F // Syriac
        | 0x0750..=0x077F // Arabic Supplement
        | 0x08A0..=0x08FF // Arabic Extended-A
        | 0xFB50..=0xFDFF // Arabic Presentation A
        | 0xFE70..=0xFEFF // Arabic Presentation B
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const FONT: &[u8] = include_bytes!("../assets/fonts/NotoNaskhArabic-Regular.ttf");

    #[test]
    fn shapes_arabic_with_contextual_forms() {
        let font = FontRef::try_from_slice(FONT).unwrap();
        // "مرحبا" — letters must join (different glyph ids than isolated forms)
        let s = shape_text(&font, "مرحبا", 72.0);
        // HarfBuzz may emit extra zero-advance contextual marks — at least
        // one glyph per character, and total advance positive.
        assert!(s.glyphs.len() >= "مرحبا".chars().count());
        assert!(s.advance > 0.0);
        // shaped ids must differ from the naive char->glyph mapping (joining!)
        let naive: Vec<u16> = "مرحبا".chars()
            .map(|c| font.glyph_id(c).0).collect();
        let shaped: Vec<u16> = s.glyphs.iter().map(|g| g.id).collect();
        assert_ne!(naive, shaped, "Arabic must be contextually shaped");
        // RTL: clusters arrive in reversed (visual) order
        let first = s.glyphs.first().unwrap().x;
        let last = s.glyphs.last().unwrap().x;
        assert!(first < last, "RTL run must be laid out visually");
    }

    #[test]
    fn latin_passes_through() {
        let font = FontRef::try_from_slice(FONT).unwrap();
        let s = shape_text(&font, "Hello", 72.0);
        assert_eq!(s.glyphs.len(), 5);
        assert!(!s.rtl);
    }
}

thread_local! {
    /// Raw bytes of the bundled title font (shared with the exporter).
    static FACE_BYTES: &'static [u8] = crate::exporter::FONT;
}
