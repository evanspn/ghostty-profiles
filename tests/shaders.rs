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
fn every_bundled_shader_compiles_at_defaults_presets_and_extreme_values() {
    use ghostty_profiles::shaderparams::{self, Kind};
    use std::collections::BTreeMap;
    let all = shaders();
    assert!(all.len() >= 12, "{}", all.len());
    let mut errors = Vec::new();
    let mut compiled = 0;
    for (name, src) in &all {
        let schema = shaderparams::parse_schema(src).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut sets: Vec<BTreeMap<String, String>> = vec![BTreeMap::new()];
        for p in &schema.presets {
            let v = shaderparams::preset_values(&schema, p);
            sets.push(schema.params.iter().zip(v).map(|(p, v)| (p.name.clone(), v)).collect());
        }
        let pick = |f: &dyn Fn(&shaderparams::Param) -> String| -> BTreeMap<String, String> {
            schema.params.iter().map(|p| (p.name.clone(), f(p))).collect()
        };
        sets.push(pick(&|p| if p.kind == Kind::Color { "#000000".into() } else { p.default.clone() }));
        sets.push(pick(&|p| if p.kind == Kind::Color { "#ffffff".into() } else { p.default.clone() }));
        sets.push(pick(&|p| match p.kind {
            Kind::Float { min, .. } => shaderparams::format_number(min),
            Kind::Color => p.default.clone(),
        }));
        sets.push(pick(&|p| match p.kind {
            Kind::Float { max, .. } => shaderparams::format_number(max),
            Kind::Color => p.default.clone(),
        }));
        for (i, values) in sets.iter().enumerate() {
            let rendered = shaderparams::render(src, values).unwrap();
            if let Err(e) = compile(&format!("{name} (variant {i})"), &rendered) {
                errors.push(e);
            }
            compiled += 1;
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    assert!(compiled > 100, "{compiled}");
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
