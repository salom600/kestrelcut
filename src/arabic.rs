//! Arabic text shaping for egui.
//!
//! egui/epaint has no complex-script shaping (HarfBuzz) or bidi, so Arabic
//! strings would render disconnected and left-to-right. This module provides
//! a compact context-form shaper based on the Unicode Arabic Presentation
//! Forms-B block (U+FE70..U+FEFF) plus simplified bidi reordering, producing
//! a *visual-order* string that egui can paint as-is for correct RTL display.

#[derive(Clone, Copy, PartialEq)]
enum Join {
    Dual,  // connects on both sides
    Right, // connects only to the previous letter (final form)
    None,  // does not connect
}

/// (isolated, final) and (initial, medial) presentation forms per base letter.
fn forms(c: char) -> Option<([u32; 2], [u32; 2], Join)> {
    let d = |i: u32| [i, i + 1]; // iso, fin
    let q = |i: u32| [i, i + 1, i + 2, i + 3]; // iso, fin, ini, med
    Some(match c {
        '\u{0621}' => ([0xFE80, 0], [0, 0], Join::None),
        '\u{0622}' => (d(0xFE81), [0, 0], Join::Right),
        '\u{0623}' => (d(0xFE83), [0, 0], Join::Right),
        '\u{0624}' => (d(0xFE85), [0, 0], Join::Right),
        '\u{0625}' => (d(0xFE87), [0, 0], Join::Right),
        '\u{0626}' => (d(0xFE89), [0xFE89 + 2, 0xFE89 + 3], Join::Dual),
        '\u{0627}' => (d(0xFE8D), [0, 0], Join::Right),
        '\u{0628}' => (d(0xFE8F), [0xFE8F + 2, 0xFE8F + 3], Join::Dual),
        '\u{0629}' => (d(0xFE93), [0, 0], Join::Right),
        '\u{062A}' => (d(0xFE95), [0xFE95 + 2, 0xFE95 + 3], Join::Dual),
        '\u{062B}' => (d(0xFE99), [0xFE99 + 2, 0xFE99 + 3], Join::Dual),
        '\u{062C}' => (d(0xFE9D), [0xFE9D + 2, 0xFE9D + 3], Join::Dual),
        '\u{062D}' => (d(0xFEA1), [0xFEA1 + 2, 0xFEA1 + 3], Join::Dual),
        '\u{062E}' => (d(0xFEA5), [0xFEA5 + 2, 0xFEA5 + 3], Join::Dual),
        '\u{062F}' => (d(0xFEA9), [0, 0], Join::Right),
        '\u{0630}' => (d(0xFEAB), [0, 0], Join::Right),
        '\u{0631}' => (d(0xFEAD), [0, 0], Join::Right),
        '\u{0632}' => (d(0xFEAF), [0, 0], Join::Right),
        '\u{0633}' => (d(0xFEB1), [0xFEB1 + 2, 0xFEB1 + 3], Join::Dual),
        '\u{0634}' => (d(0xFEB5), [0xFEB5 + 2, 0xFEB5 + 3], Join::Dual),
        '\u{0635}' => (d(0xFEB9), [0xFEB9 + 2, 0xFEB9 + 3], Join::Dual),
        '\u{0636}' => (d(0xFEBD), [0xFEBD + 2, 0xFEBD + 3], Join::Dual),
        '\u{0637}' => (d(0xFEC1), [0xFEC1 + 2, 0xFEC1 + 3], Join::Dual),
        '\u{0638}' => (d(0xFEC5), [0xFEC5 + 2, 0xFEC5 + 3], Join::Dual),
        '\u{0639}' => (d(0xFEC9), [0xFEC9 + 2, 0xFEC9 + 3], Join::Dual),
        '\u{063A}' => (d(0xFECD), [0xFECD + 2, 0xFECD + 3], Join::Dual),
        '\u{0640}' => ([0x0640, 0x0640], [0x0640, 0x0640], Join::Dual), // tatweel
        '\u{0641}' => (d(0xFED1), [0xFED1 + 2, 0xFED1 + 3], Join::Dual),
        '\u{0642}' => (d(0xFED5), [0xFED5 + 2, 0xFED5 + 3], Join::Dual),
        '\u{0643}' => (d(0xFED9), [0xFED9 + 2, 0xFED9 + 3], Join::Dual),
        '\u{0644}' => (d(0xFEDD), [0xFEDD + 2, 0xFEDD + 3], Join::Dual),
        '\u{0645}' => (d(0xFEE1), [0xFEE1 + 2, 0xFEE1 + 3], Join::Dual),
        '\u{0646}' => (d(0xFEE5), [0xFEE5 + 2, 0xFEE5 + 3], Join::Dual),
        '\u{0647}' => (d(0xFEE9), [0xFEE9 + 2, 0xFEE9 + 3], Join::Dual),
        '\u{0648}' => (d(0xFEED), [0, 0], Join::Right),
        '\u{0649}' => (d(0xFEEF), [0, 0], Join::Right),
        '\u{064A}' => (d(0xFEF1), [0xFEF1 + 2, 0xFEF1 + 3], Join::Dual),
        _ => return None,
    })
}

fn is_transparent(c: char) -> bool {
    matches!(c, '\u{064B}'..='\u{065F}' | '\u{0670}' | '\u{06D6}'..='\u{06ED}')
}

fn is_arabic_base(c: char) -> bool {
    ('\u{0621}'..='\u{064A}').contains(&c)
}

/// Lam-Alef ligatures: (madda, hamza-above, hamza-below, plain alef)
fn lam_alef(alef: char) -> Option<[u32; 2]> {
    Some(match alef {
        '\u{0622}' => [0xFEF5, 0xFEF6],
        '\u{0623}' => [0xFEF7, 0xFEF8],
        '\u{0625}' => [0xFEF9, 0xFEFA],
        '\u{0627}' => [0xFEFB, 0xFEFC],
        _ => return None,
    })
}

#[derive(PartialEq)]
enum Cls { R, L, N }

fn classify(c: char) -> Cls {
    if is_arabic_base(c)
        || matches!(c, '\u{FE70}'..='\u{FEFF}' | '\u{FB50}'..='\u{FDFF}')
        || is_transparent(c)
    {
        Cls::R
    } else if c.is_alphanumeric() {
        Cls::L
    } else {
        Cls::N
    }
}

/// Shape a logical-order Arabic/Latin mixed string into a visual-order string
/// of presentation forms that egui can paint LTR for correct RTL appearance.
pub fn shape(input: &str) -> String {
    // 1) contextual shaping (logical order)
    let chars: Vec<char> = input.chars().collect();
    let mut shaped: Vec<char> = Vec::with_capacity(chars.len());
    let n = chars.len();
    let mut i = 0;
    while i < n {
        let c = chars[i];
        if is_transparent(c) {
            i += 1;
            continue;
        }
        if !is_arabic_base(c) {
            shaped.push(c);
            i += 1;
            continue;
        }
        // does the previous emitted letter connect forward?
        let prev_join = shaped.last().copied()
            .map(|p| matches!(forms(p), Some((_, _, Join::Dual))))
            .unwrap_or(false);
        // lam-alef ligature
        if c == '\u{0644}' {
            if let Some(next) = chars.get(i + 1).copied() {
                if let Some([iso, fin]) = lam_alef(next) {
                    let code = if prev_join { fin } else { iso };
                    shaped.push(char::from_u32(code).unwrap_or(' '));
                    i += 2;
                    continue;
                }
            }
        }
        let mut k = i + 1;
        while k < n && is_transparent(chars[k]) {
            k += 1;
        }
        // only dual-joining letters connect forward
        let own_dual = matches!(forms(c), Some((_, _, Join::Dual)));
        let next_join = own_dual
            && k < n
            && matches!(forms(chars[k]), Some((_, _, Join::Dual)) | Some((_, _, Join::Right)));
        let Some(([iso, fin], [ini, med], _)) = forms(c) else {
            shaped.push(c);
            i += 1;
            continue;
        };
        let code = match (prev_join, next_join) {
            (true, true) => med,
            (true, false) => fin,
            (false, true) => ini,
            (false, false) => iso,
        };
        shaped.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
        i += 1;
    }

    // 2) simplified bidi: reverse run order; reverse chars inside R runs
    #[derive(PartialEq)]
    enum Kind { R, L }
    struct Run { kind: Kind, s: Vec<char> }
    let mut runs: Vec<Run> = Vec::new();
    let mut lead_n: Vec<char> = Vec::new();
    for c in shaped {
        match classify(c) {
            Cls::N => {
                if let Some(r) = runs.last_mut() { r.s.push(c); } else { lead_n.push(c); }
            }
            kind_cls => {
                let kind = if kind_cls == Cls::R { Kind::R } else { Kind::L };
                let start_new = !matches!(runs.last(), Some(r) if r.kind == kind);
                if start_new { runs.push(Run { kind, s: Vec::new() }); }
                runs.last_mut().unwrap().s.push(c);
            }
        }
    }
    let lead: String = lead_n.iter().collect();
    let body: String = runs.into_iter().rev().map(|r| match r.kind {
        Kind::R => r.s.into_iter().rev().collect::<String>(),
        Kind::L => r.s.into_iter().collect::<String>(),
    }).collect();
    lead + &body
}

/// Shape only if the string contains Arabic letters; otherwise pass through.
pub fn shape_if_arabic(s: &str) -> String {
    if s.chars().any(is_arabic_base) { shape(s) } else { s.to_string() }
}

pub fn is_arabic(c: char) -> bool { is_arabic_base(c) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lam_alef_shapes() {
        assert_eq!(shape("لا"), "\u{FEFB}");
    }
    #[test]
    fn word_forms() {
        let s = shape("كتاب");
        assert_eq!(s.chars().count(), 4);
        assert!(s.chars().all(|c| ('\u{FE70}'..='\u{FEFF}').contains(&c)));
    }
    #[test]
    fn mixed_number_word() {
        let s = shape("تسلسل 01");
        assert!(s.starts_with("01"));
    }
    #[test]
    fn latin_passthrough() {
        assert_eq!(shape("Hello 123"), "Hello 123");
    }
}
