//! SubRip (.srt) subtitle import → Title clips.

use std::path::Path;

/// One subtitle cue: (start_s, end_s, text).
pub type Cue = (f64, f64, String);

fn ts_to_secs(s: &str) -> Option<f64> {
    // "00:01:02,500" or "00:01:02.500"
    let s = s.trim().replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m, sec) = match parts.len() {
        3 => (parts[0].parse::<f64>().ok()?, parts[1].parse::<f64>().ok()?, parts[2].parse::<f64>().ok()?),
        2 => (0.0, parts[0].parse::<f64>().ok()?, parts[1].parse::<f64>().ok()?),
        _ => return None,
    };
    Some(h * 3600.0 + m * 60.0 + sec)
}

/// Parse SRT text into cues. Tolerant of CRLF, BOM and minor format drift.
pub fn parse_srt(text: &str) -> Vec<Cue> {
    let text = text.trim_start_matches('\u{feff}');
    let mut cues = Vec::new();
    let mut block: Vec<String> = Vec::new();
    for line in text.lines().map(|l| l.trim_end_matches('\r')) {
        if line.trim().is_empty() {
            if !block.is_empty() { push_cue(&block, &mut cues); block.clear(); }
        } else {
            block.push(line.to_string());
        }
    }
    if !block.is_empty() { push_cue(&block, &mut cues); }
    cues.retain(|(a, b, t)| b > a && !t.is_empty());
    cues
}

fn push_cue(block: &[String], out: &mut Vec<Cue>) {
    // find the timing line
    for (i, l) in block.iter().enumerate() {
        if l.contains("-->") {
            let mut it = l.split("-->");
            let a = ts_to_secs(it.next().unwrap_or("")) ;
            let b = ts_to_secs(it.next().unwrap_or(""));
            if let (Some(a), Some(b)) = (a, b) {
                let text = block[i + 1..].join("\n");
                // strip trivial HTML tags
                let clean: String = strip_tags(&text);
                out.push((a, b, clean));
            }
            return;
        }
    }
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

pub fn parse_srt_file(path: &Path) -> Result<Vec<Cue>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    // many .srt files ship as UTF-16 on Windows — sniff and convert
    let text = if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let utf16: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&utf16)
    } else if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let utf16: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&utf16)
    } else {
        String::from_utf8_lossy(&bytes).to_string()
    };
    Ok(parse_srt(&text))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_basic_srt() {
        let srt = "1\n00:00:01,000 --> 00:00:03,500\nHello <b>world</b>\n\n2\n00:00:04,000 --> 00:00:06,000\nSecond line";
        let cues = parse_srt(srt);
        assert_eq!(cues.len(), 2);
        assert!((cues[0].0 - 1.0).abs() < 1e-6);
        assert!((cues[0].1 - 3.5).abs() < 1e-6);
        assert_eq!(cues[0].2, "Hello world");
    }
}
