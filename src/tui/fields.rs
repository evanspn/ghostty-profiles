//! The editable settings of the Edit tab and how their values are validated.
//!
//! Validation is a pure function of (field, text), so the rules can be tested
//! without a terminal.

use crate::profile::{Profile, normalize_hex};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// `#rrggbb` (also accepts `#rgb` and a missing `#`).
    Hex,
    /// One of the 16 ANSI palette colors.
    Palette(u8),
    Text,
    /// Several values of a repeatable key, written comma-separated.
    List,
    Number {
        min: f64,
        max: f64,
    },
    /// Whole number or percentage, may be negative: `2`, `-1`, `10%`.
    Spacing,
    /// `8` or `8,4`.
    Padding,
    Enum(&'static [&'static str]),
    /// A file path; the file is copied into the profile.
    Image,
}

#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub kind: Kind,
}

const BOOL: &[&str] = &["true", "false"];
const FIT: &[&str] = &["contain", "cover", "stretch", "none"];
const POSITION: &[&str] = &[
    "center",
    "top-left",
    "top-center",
    "top-right",
    "center-left",
    "center-right",
    "bottom-left",
    "bottom-center",
    "bottom-right",
];
const CURSOR: &[&str] = &["block", "bar", "underline", "block_hollow"];

const fn f(group: &'static str, key: &'static str, label: &'static str, kind: Kind) -> Field {
    Field { key, label, group, kind }
}

const STATIC_BEFORE_PALETTE: [Field; 6] = [
    f("Colors", "background", "background", Kind::Hex),
    f("Colors", "foreground", "foreground", Kind::Hex),
    f("Colors", "cursor-color", "cursor color", Kind::Hex),
    f("Colors", "cursor-text", "cursor text", Kind::Hex),
    f("Colors", "selection-background", "selection bg", Kind::Hex),
    f("Colors", "selection-foreground", "selection fg", Kind::Hex),
];

const PALETTE_LABELS: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "bright black",
    "bright red",
    "bright green",
    "bright yellow",
    "bright blue",
    "bright magenta",
    "bright cyan",
    "bright white",
];

const STATIC_AFTER_PALETTE: [Field; 19] = [
    f("Text", "font-family", "font family", Kind::List),
    f("Text", "font-size", "font size", Kind::Number { min: 4.0, max: 200.0 }),
    f("Text", "font-thickness", "font thickness", Kind::Number { min: 0.0, max: 1000.0 }),
    f("Text", "font-feature", "font features", Kind::List),
    f("Text", "adjust-cell-width", "cell width", Kind::Spacing),
    f("Text", "adjust-cell-height", "cell height", Kind::Spacing),
    f("Text", "adjust-font-baseline", "font baseline", Kind::Spacing),
    f("Cursor", "cursor-style", "style", Kind::Enum(CURSOR)),
    f("Cursor", "cursor-style-blink", "blink", Kind::Enum(BOOL)),
    f("Cursor", "cursor-opacity", "opacity", Kind::Number { min: 0.0, max: 1.0 }),
    f("Window", "background-opacity", "opacity", Kind::Number { min: 0.0, max: 1.0 }),
    f("Window", "background-blur", "blur radius", Kind::Number { min: 0.0, max: 100.0 }),
    f("Window", "window-padding-x", "padding x", Kind::Padding),
    f("Window", "window-padding-y", "padding y", Kind::Padding),
    f("Image", "background-image", "image file", Kind::Image),
    f("Image", "background-image-opacity", "image opacity", Kind::Number { min: 0.0, max: 1.0 }),
    f("Image", "background-image-fit", "image fit", Kind::Enum(FIT)),
    f("Image", "background-image-position", "image position", Kind::Enum(POSITION)),
    f("Image", "background-image-repeat", "image repeat", Kind::Enum(BOOL)),
];

/// Every editable field, in display order (palette rows sit between colors and text).
pub fn all_fields() -> Vec<Field> {
    let mut v: Vec<Field> = STATIC_BEFORE_PALETTE.to_vec();
    for (i, label) in PALETTE_LABELS.iter().enumerate() {
        v.push(Field { key: "palette", label, group: "Palette", kind: Kind::Palette(i as u8) });
    }
    v.extend(STATIC_AFTER_PALETTE);
    v
}

/// The value a field currently has in the profile, as shown/edited.
pub fn current_value(p: &Profile, field: &Field) -> Option<String> {
    match field.kind {
        Kind::Palette(n) => p.palette().get(&n).cloned(),
        Kind::List => {
            let v = p.get_all(field.key);
            (!v.is_empty()).then(|| v.join(", "))
        }
        _ => p.get(field.key).filter(|v| !v.is_empty()),
    }
}

/// Check `input` for `field`. Returns the canonical text to store (`None` means "unset"),
/// or a message saying what is wrong.
pub fn validate(field: &Field, input: &str) -> Result<Option<String>, String> {
    let t = input.trim();
    if t.is_empty() {
        return Ok(None);
    }
    match field.kind {
        Kind::Hex | Kind::Palette(_) => {
            normalize_hex(t).map(Some).ok_or_else(|| "not a color: use #rrggbb (or #rgb)".to_string())
        }
        Kind::Text | Kind::Image => Ok(Some(t.to_string())),
        Kind::List => {
            let parts: Vec<&str> = t.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
            if parts.is_empty() { Ok(None) } else { Ok(Some(parts.join(", "))) }
        }
        Kind::Number { min, max } => match t.parse::<f64>() {
            Ok(n) if n.is_finite() && n >= min && n <= max => Ok(Some(t.to_string())),
            Ok(_) => Err(format!("must be between {min} and {max}")),
            Err(_) => Err("not a number".to_string()),
        },
        Kind::Spacing => {
            let body = t.strip_suffix('%').unwrap_or(t);
            let digits = body.strip_prefix('-').unwrap_or(body);
            if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                Ok(Some(t.to_string()))
            } else {
                Err("use a whole number or a percentage, e.g. 2, -1 or 10%".to_string())
            }
        }
        Kind::Padding => {
            let ok = t.split(',').count() <= 2
                && t.split(',').all(|p| !p.trim().is_empty() && p.trim().chars().all(|c| c.is_ascii_digit()));
            if ok { Ok(Some(t.replace(' ', ""))) } else { Err("use a number like 8, or two like 8,4".to_string()) }
        }
        Kind::Enum(options) => {
            let lower = t.to_lowercase();
            options
                .iter()
                .find(|o| **o == lower)
                .map(|o| Some((*o).to_string()))
                .ok_or_else(|| format!("one of: {}", options.join(", ")))
        }
    }
}

/// Write an already-validated value into the profile (`None` unsets it).
pub fn store_value(p: &mut Profile, field: &Field, value: Option<&str>) {
    match (field.kind, value) {
        (Kind::Palette(n), Some(v)) => p.set_palette_color(n, v),
        (Kind::Palette(n), None) => {
            let mut pal = p.palette();
            pal.remove(&n);
            p.set_palette(&pal);
            if pal.is_empty() {
                p.remove("palette");
            }
        }
        (Kind::List, Some(v)) => {
            let items: Vec<String> = v.split(", ").map(str::to_string).collect();
            p.set_all(field.key, &items);
        }
        (_, Some(v)) => p.set(field.key, v),
        (_, None) => p.remove(field.key),
    }
}

/// Next value for an Enum field (wraps; an unset field starts at the first option).
pub fn cycle(field: &Field, current: Option<&str>, forward: bool) -> Option<&'static str> {
    let Kind::Enum(options) = field.kind else { return None };
    let n = options.len();
    let i = current.and_then(|c| options.iter().position(|o| *o == c));
    let next = match (i, forward) {
        (None, true) => 0,
        (None, false) => n - 1,
        (Some(i), true) => (i + 1) % n,
        (Some(i), false) => (i + n - 1) % n,
    };
    Some(options[next])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(key: &str) -> Field {
        *all_fields().iter().find(|f| f.key == key).unwrap()
    }

    #[test]
    fn there_is_a_field_for_each_of_the_16_palette_colors() {
        let pal: Vec<_> = all_fields().into_iter().filter(|f| matches!(f.kind, Kind::Palette(_))).collect();
        assert_eq!(pal.len(), 16);
        assert_eq!(pal[15].kind, Kind::Palette(15));
    }

    #[test]
    fn colors_are_validated_and_normalized() {
        let bg = field("background");
        assert_eq!(validate(&bg, "#F60"), Ok(Some("#ff6600".into())));
        assert_eq!(validate(&bg, " 2C2C2C "), Ok(Some("#2c2c2c".into())));
        assert!(validate(&bg, "#12345").is_err());
        assert!(validate(&bg, "orange").is_err());
        assert_eq!(validate(&bg, ""), Ok(None));
    }

    #[test]
    fn numbers_spacing_padding_and_enums() {
        let op = field("background-opacity");
        assert_eq!(validate(&op, "0.5"), Ok(Some("0.5".into())));
        assert!(validate(&op, "1.5").is_err());
        assert!(validate(&op, "abc").is_err());
        assert!(validate(&op, "NaN").is_err());
        let sp = field("adjust-cell-height");
        for ok in ["2", "-1", "10%", "-5%"] {
            assert!(validate(&sp, ok).is_ok(), "{ok}");
        }
        for bad in ["%", "1.5", "--1", "a"] {
            assert!(validate(&sp, bad).is_err(), "{bad}");
        }
        let pad = field("window-padding-x");
        assert_eq!(validate(&pad, "8, 4"), Ok(Some("8,4".into())));
        assert!(validate(&pad, "8,4,2").is_err());
        let fit = field("background-image-fit");
        assert_eq!(validate(&fit, "COVER"), Ok(Some("cover".into())));
        assert!(validate(&fit, "zoom").is_err());
    }

    #[test]
    fn lists_round_trip_through_the_profile() {
        let mut p = Profile::new("t", std::path::Path::new("/nonexistent"));
        let ff = field("font-family");
        store_value(&mut p, &ff, validate(&ff, "JetBrains Mono, Symbols Nerd Font").unwrap().as_deref());
        assert_eq!(p.get_all("font-family"), vec!["JetBrains Mono", "Symbols Nerd Font"]);
        assert_eq!(current_value(&p, &ff).as_deref(), Some("JetBrains Mono, Symbols Nerd Font"));
        store_value(&mut p, &ff, None);
        assert!(p.get_all("font-family").is_empty());
    }

    #[test]
    fn palette_fields_store_and_unset() {
        let mut p = Profile::new("t", std::path::Path::new("/nonexistent"));
        let c3 = all_fields().into_iter().find(|f| f.kind == Kind::Palette(3)).unwrap();
        store_value(&mut p, &c3, Some("#ff6a00"));
        assert_eq!(p.get_all("palette"), vec!["3=#ff6a00"]);
        assert_eq!(current_value(&p, &c3).as_deref(), Some("#ff6a00"));
        store_value(&mut p, &c3, None);
        assert!(p.get_all("palette").is_empty());
    }

    #[test]
    fn enums_cycle_both_ways_and_wrap() {
        let st = field("cursor-style");
        assert_eq!(cycle(&st, None, true), Some("block"));
        assert_eq!(cycle(&st, None, false), Some("block_hollow"));
        assert_eq!(cycle(&st, Some("block_hollow"), true), Some("block"));
        assert_eq!(cycle(&st, Some("block"), false), Some("block_hollow"));
        assert_eq!(cycle(&field("background"), None, true), None);
    }
}
