//! Every bundled shader must compile. Ghostty shows no error when a custom
//! shader fails (the window just looks unchanged), so this is the safety net:
//! each library shader is wrapped the way Ghostty wraps Shadertoy-style code and
//! parsed + validated with naga.

use naga::ShaderStage;
use naga::front::glsl::{Frontend, Options};
use naga::valid::{Capabilities, ValidationFlags, Validator};

const PREFIX: &str = r#"#version 450 core
layout(set = 0, binding = 0) uniform Globals {
    vec3 iResolution;
    float iTime;
    float iTimeDelta;
    int iFrame;
    vec4 iMouse;
    vec4 iDate;
};
layout(set = 0, binding = 1) uniform texture2D iChannel0_tex;
layout(set = 0, binding = 2) uniform sampler iChannel0_smp;
#define iChannel0 sampler2D(iChannel0_tex, iChannel0_smp)
layout(location = 0) out vec4 _fragColor;
"#;

const SUFFIX: &str = "\nvoid main() { mainImage(_fragColor, gl_FragCoord.xy); }\n";

fn compile(name: &str, src: &str) -> Result<(), String> {
    let full = format!("{PREFIX}{src}{SUFFIX}");
    let module = Frontend::default()
        .parse(&Options::from(ShaderStage::Fragment), &full)
        .map_err(|e| format!("{name}: parse: {e:?}"))?;
    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|e| format!("{name}: validate: {e:?}"))?;
    Ok(())
}

fn shaders() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("presets/shaders");
    let mut v: Vec<(String, String)> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read_to_string(e.path()).unwrap()))
        .collect();
    v.sort();
    v
}

#[test]
fn every_bundled_shader_compiles() {
    let all = shaders();
    assert!(all.len() >= 4, "{}", all.len());
    let errors: Vec<String> = all.iter().filter_map(|(n, s)| compile(n, s).err()).collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn the_checker_itself_catches_a_broken_shader() {
    assert!(compile("bad", "void mainImage(out vec4 c, in vec2 p) { c = vec4(undefined_name); }").is_err());
    assert!(compile("bad2", "void mainImage(out vec4 c, in vec2 p) { c = vec4(1.0) }").is_err());
    assert!(
        compile("good", "void mainImage(out vec4 c, in vec2 p) { c = texture(iChannel0, p / iResolution.xy); }")
            .is_ok()
    );
}
