//! Reader/writer for Ghostty's native `key = value` config syntax.
//!
//! Comments, blank lines and order are preserved. An entry that was parsed and
//! not touched is written back byte-for-byte; entries made by [`entry`] are
//! written as `key = value`.

/// One line of a config file: a comment/blank (no key) or a `key = value` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    raw: String,
    kv: Option<(String, String)>,
}

impl Line {
    /// The key, for entries.
    pub fn key(&self) -> Option<&str> {
        self.kv.as_ref().map(|(k, _)| k.as_str())
    }

    /// The value, for entries.
    pub fn value(&self) -> Option<&str> {
        self.kv.as_ref().map(|(_, v)| v.as_str())
    }

    pub fn is_entry(&self) -> bool {
        self.kv.is_some()
    }

    /// The text as written (comments keep their exact text).
    pub fn text(&self) -> &str {
        &self.raw
    }
}

/// A comment (or blank) line.
pub fn comment(text: &str) -> Line {
    Line { raw: text.to_string(), kv: None }
}

/// A fresh `key = value` entry.
pub fn entry(key: &str, value: &str) -> Line {
    Line { raw: format!("{key} = {value}"), kv: Some((key.to_string(), value.to_string())) }
}

pub fn parse(text: &str) -> Vec<Line> {
    text.lines()
        .map(|raw| {
            let t = raw.trim();
            if t.is_empty() || t.starts_with('#') {
                return comment(raw);
            }
            match t.split_once('=') {
                Some((k, v)) => Line { raw: raw.to_string(), kv: Some((k.trim().to_string(), v.trim().to_string())) },
                None => comment(raw),
            }
        })
        .collect()
}

pub fn dump(lines: &[Line]) -> String {
    let mut out = String::new();
    for l in lines {
        out.push_str(&l.raw);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# description: demo\n\nbackground = #2c2c2c\npalette = 0=#0a0a0a\npalette = 1=#e23636\n# trailing comment\ncustom-shader = shaders/x.glsl\n";

    #[test]
    fn round_trip_is_byte_identical() {
        assert_eq!(dump(&parse(SAMPLE)), SAMPLE);
    }

    #[test]
    fn irregular_spacing_survives_until_touched() {
        let text = "background=#fff\nfont-size   =   13\n";
        assert_eq!(dump(&parse(text)), text);
        let mut lines = parse(text);
        lines[0] = entry("background", "#000");
        assert_eq!(dump(&lines), "background = #000\nfont-size   =   13\n");
    }

    #[test]
    fn values_may_contain_equals_signs() {
        let lines = parse("palette = 0=#0a0a0a\n");
        assert_eq!(lines[0].key(), Some("palette"));
        assert_eq!(lines[0].value(), Some("0=#0a0a0a"));
    }

    #[test]
    fn comments_blanks_and_garbage_are_not_entries() {
        let lines = parse("# a = b\n\nnot an entry\n");
        assert!(lines.iter().all(|l| !l.is_entry()));
        assert_eq!(dump(&lines), "# a = b\n\nnot an entry\n");
    }
}
