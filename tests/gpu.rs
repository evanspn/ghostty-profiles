//! Runs every bundled shader on the real GPU (wgpu: Metal on a Mac) over a synthetic terminal frame.
//!
//! This is not Ghostty, but it IS the shader code executing: the same Shadertoy-style wrapper
//! (iResolution, iTime, iChannel0), the real parameter header, and real pixels out. It checks that
//! every shader compiles and runs at default and extreme parameter values, that the shaders which
//! promise to leave text alone really do (text pixels come out bit-for-bit as they went in), that none
//! of them reads neighbouring pixels (so none can blur text), and that each one visibly does something.
//!
//! No GPU adapter (for example on a headless CI box) skips the tests with a message; it is never a pass
//! that claims anything.
//!
//! Set GPF_DUMP_DIR to write a PPM image of each shader's output (used for the README's evidence).

use std::collections::BTreeMap;

use ghostty_profiles::shaderparams::{self, Kind, Schema};

const W: u32 = 320;
const H: u32 = 180;

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

// WebGPU's framebuffer origin is top-left; Ghostty's (Shadertoy's) is bottom-left, so flip y here.
const SUFFIX: &str = "\nvoid main() { mainImage(_fragColor, vec2(gl_FragCoord.x, iResolution.y - gl_FragCoord.y)); }\n";

const VERTEX: &str = r#"
@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4<f32>(p[i], 0.0, 1.0);
}
"#;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

fn gpu() -> Option<Gpu> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
    Some(Gpu { device, queue })
}

/// A synthetic terminal: dark background, rows of bright "text" blocks, some dim and colored text.
/// Returned top row first, RGBA.
fn terminal_frame() -> Vec<u8> {
    let mut px = vec![0u8; (W * H * 4) as usize];
    let set = |px: &mut Vec<u8>, x: u32, y: u32, c: [u8; 3]| {
        let i = ((y * W + x) * 4) as usize;
        px[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
    };
    for y in 0..H {
        for x in 0..W {
            set(&mut px, x, y, [0x1c, 0x1c, 0x24]);
        }
    }
    let mut seed = 12345u32;
    let mut rnd = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 16) & 0xffff
    };
    for row in 0..9u32 {
        let y0 = 12 + row * 18;
        let color = match row % 4 {
            0 => [0xe6, 0xe6, 0xe6],
            1 => [0xf2, 0xf2, 0xf2],
            2 => [0xd8, 0xd8, 0xd8],
            _ => [0xff, 0xff, 0xff],
        };
        let mut x = 12u32;
        while x + 8 < W - 12 {
            let glyph_w = 5 + rnd() % 3;
            if rnd() % 5 != 0 {
                for gy in 0..11 {
                    for gx in 0..glyph_w {
                        if (gx + gy + rnd() % 2) % 3 != 0 {
                            set(&mut px, x + gx, y0 + gy, color);
                        }
                    }
                }
            }
            x += glyph_w + 2;
        }
    }
    px
}

fn lum(c: &[u8]) -> f32 {
    (0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32) / 255.0
}

fn render(g: &Gpu, shader_src: &str, input: &[u8], time: f32) -> Vec<u8> {
    let d = &g.device;
    let full = format!("{PREFIX}{shader_src}{SUFFIX}");
    let frag = d.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("fragment"),
        source: wgpu::ShaderSource::Glsl {
            shader: full.into(),
            stage: wgpu::naga::ShaderStage::Fragment,
            defines: &[],
        },
    });
    let vert = d.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("vertex"),
        source: wgpu::ShaderSource::Wgsl(VERTEX.into()),
    });

    // the terminal texture, uploaded bottom row first: Ghostty's iChannel0 has v = 0 at the bottom
    let mut flipped = Vec::with_capacity(input.len());
    for row in (0..H).rev() {
        flipped.extend_from_slice(&input[(row * W * 4) as usize..((row + 1) * W * 4) as usize]);
    }
    let tex = d.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    g.queue.write_texture(
        tex.as_image_copy(),
        &flipped,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(W * 4), rows_per_image: Some(H) },
        wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
    );
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = d.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    let mut globals = [0u8; 64];
    let put = |b: &mut [u8; 64], off: usize, v: f32| b[off..off + 4].copy_from_slice(&v.to_le_bytes());
    put(&mut globals, 0, W as f32);
    put(&mut globals, 4, H as f32);
    put(&mut globals, 8, 1.0);
    put(&mut globals, 12, time);
    put(&mut globals, 16, 1.0 / 60.0);
    let ubuf = d.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    g.queue.write_buffer(&ubuf, 0, &globals);

    let bgl = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let bind = d.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
        ],
    });
    let layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let pipeline = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &vert,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &frag,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let target = d.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let tview = target.create_view(&wgpu::TextureViewDescriptor::default());
    let padded = (W * 4).next_multiple_of(256);
    let out = d.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (padded * H) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut enc = d.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &tview,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, Some(&bind), &[]);
        pass.draw(0..3, 0..1);
    }
    enc.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &out,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(H) },
        },
        wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
    );
    g.queue.submit(Some(enc.finish()));
    let slice = out.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("map"));
    d.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
    let data = slice.get_mapped_range().expect("mapped range");
    let mut result = Vec::with_capacity((W * H * 4) as usize);
    for row in 0..H {
        result.extend_from_slice(&data[(row * padded) as usize..(row * padded + W * 4) as usize]);
    }
    result
}

fn library() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("presets/shaders");
    let mut v: Vec<(String, String)> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read_to_string(e.path()).unwrap()))
        .collect();
    v.sort();
    v
}

fn rendered(src: &str, values: &BTreeMap<String, String>) -> String {
    shaderparams::render(src, values).expect("the shader's annotations are valid")
}

/// Parameter value sets worth running: defaults, every preset, all colors black / white, floats at min / max.
fn variants(schema: &Schema) -> Vec<(String, BTreeMap<String, String>)> {
    let mut out: Vec<(String, BTreeMap<String, String>)> = vec![("defaults".into(), BTreeMap::new())];
    for p in &schema.presets {
        let vals = shaderparams::preset_values(schema, p);
        out.push((
            format!("preset {}", p.name),
            schema.params.iter().zip(vals).map(|(p, v)| (p.name.clone(), v)).collect(),
        ));
    }
    let mk = |f: &dyn Fn(&shaderparams::Param) -> String| -> BTreeMap<String, String> {
        schema.params.iter().map(|p| (p.name.clone(), f(p))).collect()
    };
    out.push((
        "colors black".into(),
        mk(&|p| if p.kind == Kind::Color { "#000000".into() } else { p.default.clone() }),
    ));
    out.push((
        "colors white".into(),
        mk(&|p| if p.kind == Kind::Color { "#ffffff".into() } else { p.default.clone() }),
    ));
    out.push((
        "floats at min".into(),
        mk(&|p| match p.kind {
            Kind::Float { min, .. } => shaderparams::format_number(min),
            Kind::Color => p.default.clone(),
        }),
    ));
    out.push((
        "floats at max".into(),
        mk(&|p| match p.kind {
            Kind::Float { max, .. } => shaderparams::format_number(max),
            Kind::Color => p.default.clone(),
        }),
    ));
    out
}

/// Shaders that promise to leave text alone and to only add light behind it.
fn background_only(name: &str) -> bool {
    !matches!(name, "crt-scanlines.glsl" | "soft-glow.glsl")
}

macro_rules! gpu_or_skip {
    () => {
        match gpu() {
            Some(g) => g,
            None => {
                eprintln!("SKIPPED: no GPU adapter available, nothing was verified by this test");
                return;
            }
        }
    };
}

#[test]
fn every_shader_runs_at_defaults_presets_and_extreme_values() {
    let g = gpu_or_skip!();
    let frame = terminal_frame();
    let mut runs = 0;
    for (name, src) in library() {
        let schema = shaderparams::parse_schema(&src).unwrap_or_else(|e| panic!("{name}: {e}"));
        for (label, values) in variants(&schema) {
            for t in [0.0, 3.7, 41.2] {
                let out = render(&g, &rendered(&src, &values), &frame, t);
                assert_eq!(out.len(), frame.len(), "{name} {label}");
                // the window's alpha channel is never changed by a shader
                assert!(out.chunks(4).all(|p| p[3] == 255), "{name} {label}: alpha changed");
                runs += 1;
            }
        }
    }
    assert!(runs > 150, "{runs} runs");
    eprintln!("{runs} shader runs on the GPU");
}

#[test]
fn background_only_shaders_leave_text_pixels_exactly_as_drawn() {
    let g = gpu_or_skip!();
    let frame = terminal_frame();
    let text: Vec<usize> = (0..(W * H) as usize).filter(|i| lum(&frame[i * 4..i * 4 + 4]) >= 0.82).collect();
    assert!(text.len() > 3000, "the synthetic frame has text: {}", text.len());
    for (name, src) in library().into_iter().filter(|(n, _)| background_only(n)) {
        let schema = shaderparams::parse_schema(&src).unwrap();
        for (label, values) in variants(&schema) {
            for t in [0.0, 2.5, 9.9, 123.4] {
                let out = render(&g, &rendered(&src, &values), &frame, t);
                let bad = text.iter().filter(|&&i| out[i * 4..i * 4 + 4] != frame[i * 4..i * 4 + 4]).count();
                assert_eq!(bad, 0, "{name} [{label}] t={t}: {bad} text pixels were changed");
            }
        }
    }
}

#[test]
fn no_background_only_shader_reads_neighbouring_pixels_so_none_can_blur() {
    let g = gpu_or_skip!();
    let frame = terminal_frame();
    // change one far-away pixel of the input: every other output pixel must be identical, which cannot
    // be true of anything that samples neighbours (a blur, a bloom, a glow of the terminal's own content)
    let mut poked = frame.clone();
    let (px, py) = (200u32, 120u32);
    let at = ((py * W + px) * 4) as usize;
    poked[at..at + 3].copy_from_slice(&[255, 0, 255]);
    for (name, src) in library().into_iter().filter(|(n, _)| background_only(n)) {
        let schema = shaderparams::parse_schema(&src).unwrap();
        let maxed: BTreeMap<String, String> = schema
            .params
            .iter()
            .map(|p| {
                (
                    p.name.clone(),
                    match p.kind {
                        Kind::Float { max, .. } => shaderparams::format_number(max),
                        Kind::Color => p.default.clone(),
                    },
                )
            })
            .collect();
        for values in [BTreeMap::new(), maxed] {
            let src = rendered(&src, &values);
            let a = render(&g, &src, &frame, 5.0);
            let b = render(&g, &src, &poked, 5.0);
            let differing: Vec<usize> =
                (0..(W * H) as usize).filter(|&i| a[i * 4..i * 4 + 4] != b[i * 4..i * 4 + 4]).collect();
            assert_eq!(
                differing,
                vec![(py * W + px) as usize],
                "{name}: the output depends on pixels other than its own ({} differ)",
                differing.len()
            );
        }
    }
}

#[test]
fn soft_glow_only_reads_neighbours_when_its_text_glow_is_turned_on() {
    let g = gpu_or_skip!();
    let frame = terminal_frame();
    let mut poked = frame.clone();
    let at = ((120 * W + 200) * 4) as usize;
    poked[at..at + 3].copy_from_slice(&[255, 255, 255]);
    let src = library().into_iter().find(|(n, _)| n == "soft-glow.glsl").unwrap().1;
    let off = rendered(&src, &BTreeMap::new()); // bloom defaults to 0
    let (a, b) = (render(&g, &off, &frame, 1.0), render(&g, &off, &poked, 1.0));
    let n = (0..(W * H) as usize).filter(|&i| a[i * 4..i * 4 + 4] != b[i * 4..i * 4 + 4]).count();
    assert_eq!(n, 1, "with the glow off it is only a vignette: nothing blurs");
    let on = rendered(&src, &[("bloom".to_string(), "0.3".to_string())].into_iter().collect());
    let (a, b) = (render(&g, &on, &frame, 1.0), render(&g, &on, &poked, 1.0));
    let n = (0..(W * H) as usize).filter(|&i| a[i * 4..i * 4 + 4] != b[i * 4..i * 4 + 4]).count();
    assert!(n > 1, "with the glow on it does read neighbours ({n} pixels differ): that is why it is off by default");
    // the vignette leaves the center untouched
    let centre = (((H / 2) * W + W / 2) * 4) as usize;
    let out = render(&g, &off, &frame, 1.0);
    assert_eq!(out[centre..centre + 4], frame[centre..centre + 4]);
}

#[test]
fn every_effect_actually_draws_something_and_strength_zero_draws_nothing() {
    let g = gpu_or_skip!();
    let frame = terminal_frame();
    for (name, src) in library().into_iter().filter(|(n, _)| background_only(n)) {
        let schema = shaderparams::parse_schema(&src).unwrap();
        let strength = schema.params.iter().find(|p| p.name == "strength").expect("every effect has a strength");
        let Kind::Float { max, .. } = strength.kind else { panic!() };
        // it draws: at its default strength, across a few moments, some background pixels are brighter
        let mut best = 0usize;
        for t in [1.0, 7.0, 19.0, 33.3] {
            let out = render(&g, &rendered(&src, &BTreeMap::new()), &frame, t);
            best = best.max(
                (0..(W * H) as usize)
                    .filter(|&i| out[i * 4..i * 4 + 3].iter().zip(&frame[i * 4..i * 4 + 3]).any(|(o, f)| o > f))
                    .count(),
            );
        }
        assert!(best > 150, "{name}: only {best} pixels changed at its default strength");
        // and at maximum strength it never goes past the background
        let loud: BTreeMap<String, String> =
            [("strength".to_string(), shaderparams::format_number(max))].into_iter().collect();
        let out = render(&g, &rendered(&src, &loud), &frame, 7.0);
        assert!(out.chunks(4).all(|p| p[3] == 255));
        // strength 0 is a pure pass-through
        // (xmb-classic also has a separate background tint, which is not part of "strength")
        let mut off: BTreeMap<String, String> = [("strength".to_string(), "0".to_string())].into_iter().collect();
        if schema.params.iter().any(|p| p.name == "tint") {
            off.insert("tint".into(), "0".into());
        }
        let out = render(&g, &rendered(&src, &off), &frame, 7.0);
        assert!(out == frame, "{name}: strength 0 must return the terminal untouched");
    }
}

#[test]
fn the_effects_move_over_time_and_hold_still_when_time_does() {
    let g = gpu_or_skip!();
    let frame = terminal_frame();
    for (name, src) in library().into_iter().filter(|(n, _)| background_only(n)) {
        let s = rendered(&src, &BTreeMap::new());
        let (a, b, a2) = (render(&g, &s, &frame, 4.0), render(&g, &s, &frame, 6.5), render(&g, &s, &frame, 4.0));
        assert_eq!(a, a2, "{name}: same time, same picture (deterministic)");
        assert_ne!(a, b, "{name}: nothing moved between t=4 and t=6.5");
    }
}

#[test]
fn dump_pictures_when_asked() {
    let Ok(dir) = std::env::var("GPF_DUMP_DIR") else { return };
    let g = gpu_or_skip!();
    let frame = terminal_frame();
    std::fs::create_dir_all(&dir).unwrap();
    let write = |name: &str, px: &[u8]| {
        let mut data = format!("P6\n{W} {H}\n255\n").into_bytes();
        for p in px.chunks(4) {
            data.extend_from_slice(&p[..3]);
        }
        std::fs::write(format!("{dir}/{name}.ppm"), data).unwrap();
    };
    write("_input", &frame);
    for (name, src) in library() {
        let stem = name.trim_end_matches(".glsl");
        write(stem, &render(&g, &rendered(&src, &BTreeMap::new()), &frame, 6.0));
        if let Ok(schema) = shaderparams::parse_schema(&src) {
            for p in &schema.presets {
                let vals = shaderparams::preset_values(&schema, p);
                let map: BTreeMap<String, String> =
                    schema.params.iter().zip(vals).map(|(p, v)| (p.name.clone(), v)).collect();
                write(&format!("{stem}--{}", p.name), &render(&g, &rendered(&src, &map), &frame, 6.0));
            }
        }
    }
}
