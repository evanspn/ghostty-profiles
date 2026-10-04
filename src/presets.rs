//! Presets and themes compiled into the binary: no runtime dependency on where
//! (or whether) the Ghostty app bundle is installed.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use include_dir::{Dir, include_dir};

use crate::profile::{CONF_NAME, SHADER_KEY};

static PRESETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/presets");
static THEMES: Dir = include_dir!("$CARGO_MANIFEST_DIR/themes");

fn sub(name: &str) -> Option<&'static Dir<'static>> {
    PRESETS.get_dir(name)
}

/// Names of the bundled profiles, sorted.
pub fn profile_names() -> Vec<String> {
    let mut v: Vec<String> = sub("profiles")
        .map(|d| d.dirs().filter_map(|p| p.path().file_name()).map(|n| n.to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

pub fn profile_conf(name: &str) -> Option<&'static str> {
    sub("profiles")?
        .get_dir(format!("profiles/{name}"))?
        .get_file(format!("profiles/{name}/{CONF_NAME}"))?
        .contents_utf8()
}

/// Names (`foo.glsl`) of the bundled shader library, sorted.
pub fn shader_names() -> Vec<String> {
    let mut v: Vec<String> = sub("shaders")
        .map(|d| d.files().filter_map(|f| f.path().file_name()).map(|n| n.to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

pub fn shader_source(name: &str) -> Option<&'static str> {
    PRESETS.get_file(format!("shaders/{name}"))?.contents_utf8()
}

/// Shader copies that earlier releases installed into profiles, byte for byte. A profile whose copy
/// still matches one of these (so the user never changed it) is upgraded to the current library
/// version, which is tunable and, for `aurora`, no longer a wash over the whole window.
const LEGACY: [(&str, &str); 17] = [
    ("xmb-waves.glsl", include_str!("../presets/legacy/xmb-waves-0.1.glsl")),
    ("aurora.glsl", include_str!("../presets/legacy/aurora-0.1.glsl")),
    ("soft-glow.glsl", include_str!("../presets/legacy/soft-glow-0.1.glsl")),
    ("xmb-waves.glsl", include_str!("../presets/legacy/xmb-waves-0.2.glsl")),
    ("xmb-mono.glsl", include_str!("../presets/legacy/xmb-mono-0.2.glsl")),
    ("xmb-classic.glsl", include_str!("../presets/legacy/xmb-classic-0.2.glsl")),
    ("xmb-dusk.glsl", include_str!("../presets/legacy/xmb-dusk-0.2.glsl")),
    ("aurora.glsl", include_str!("../presets/legacy/aurora-0.2.glsl")),
    ("xmb-aurora-ribbons.glsl", include_str!("../presets/legacy/xmb-aurora-ribbons-0.2.glsl")),
    ("enchant-glyphs.glsl", include_str!("../presets/legacy/enchant-glyphs-0.2.glsl")),
    ("pixel-rain.glsl", include_str!("../presets/legacy/pixel-rain-0.2.glsl")),
    ("matrix-rain.glsl", include_str!("../presets/legacy/matrix-rain-0.2.glsl")),
    ("starfield.glsl", include_str!("../presets/legacy/starfield-0.2.glsl")),
    ("snow.glsl", include_str!("../presets/legacy/snow-0.2.glsl")),
    ("fireflies.glsl", include_str!("../presets/legacy/fireflies-0.2.glsl")),
    ("crt-scanlines.glsl", include_str!("../presets/legacy/crt-scanlines-0.2.glsl")),
    ("soft-glow.glsl", include_str!("../presets/legacy/soft-glow-0.2.glsl")),
];

/// The current library version of `file_name` if `text` is an untouched copy of an older release.
pub fn upgrade_legacy_shader(file_name: &str, text: &str) -> Option<&'static str> {
    LEGACY
        .iter()
        .find(|(name, old)| *name == file_name && old.trim() == text.trim())
        .and_then(|(name, _)| shader_source(name))
}

/// Is `body` (a shader without its generated header) exactly a bundled shader, or an older release of one?
/// Such a file is a generated copy, not something the user wrote.
pub fn is_known_shader_body(file_name: &str, body: &str) -> bool {
    shader_source(file_name).is_some_and(|s| s.trim() == body.trim())
        || LEGACY.iter().any(|(n, old)| *n == file_name && old.trim() == body.trim())
}

/// Write a bundled profile into `dest`: its `profile.conf`, plus a copy of every
/// shader it references from the library into `dest/shaders/`.
pub fn install_profile(name: &str, dest: &Path) -> Result<()> {
    let conf = profile_conf(name).with_context(|| format!("no bundled profile '{name}'"))?;
    fs::create_dir_all(dest)?;
    crate::store::atomic_write(&dest.join(CONF_NAME), conf)?;
    for l in crate::confparse::parse(conf) {
        if l.key() == Some(SHADER_KEY)
            && let Some(rel) = l.value()
            && let Some(file) = rel.strip_prefix("shaders/")
        {
            let src = shader_source(file).with_context(|| format!("preset '{name}' needs missing shader {file}"))?;
            fs::create_dir_all(dest.join("shaders"))?;
            crate::store::atomic_write(&dest.join(rel), src)?;
            // a preset may also ship the values for its shader's parameters
            let sidecar = crate::profile::sidecar_rel(rel);
            if let Some(f) = PRESETS.get_file(format!("profiles/{name}/{sidecar}"))
                && let Some(text) = f.contents_utf8()
            {
                crate::store::atomic_write(&dest.join(&sidecar), text)?;
            }
        }
    }
    // write each shader's parameter header (defaults, or the values the preset shipped)
    crate::profile::Profile::load(dest)?.render_shaders()?;
    Ok(())
}

/// Every bundled theme as (name, file contents), in the order they are stored.
pub fn bundled_themes() -> impl Iterator<Item = (String, &'static str)> {
    THEMES.files().filter_map(|f| {
        let name = f.path().file_name()?.to_string_lossy().into_owned();
        Some((name, f.contents_utf8()?))
    })
}
