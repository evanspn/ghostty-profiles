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

// ORIENTATION. This matches Ghostty, not Shadertoy. Ghostty's own shader prefix (it is embedded as text in the
// Ghostty binary) calls `mainImage(_fragColor, gl_FragCoord.xy)` with `gl_FragCoord` coming from Metal's
// position, whose origin is the TOP-left: fragCoord.y grows DOWNWARD, row 0 of a read-back image is
// fragCoord.y = 0, and `texture(iChannel0, fragCoord / iResolution.xy)` needs no flip. (Shadertoy is the
// other way round, with y up.) The bundled shaders that have a direction compute in y-up space through the
// `gp_yup()` helper that the parameter header defines.
const SUFFIX: &str = "\nvoid main() { mainImage(_fragColor, gl_FragCoord.xy); }\n";

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

/// The SHD palette (the operator's real terminal colors): 0..7, then the bright 8..15, including the dim
/// `#4a4a4a` (8) and the near-black `#0a0a0a` (0) that a brightness cut-off cannot tell from the background.
const PALETTE: [[u8; 3]; 16] = [
    [0x0a, 0x0a, 0x0a],
    [0xe2, 0x36, 0x36],
    [0x6f, 0xcb, 0x3d],
    [0xff, 0x6a, 0x00],
    [0xcc, 0x55, 0x00],
    [0xff, 0x2e, 0x63],
    [0x22, 0xe3, 0xd1],
    [0xd8, 0xd8, 0xd8],
    [0x4a, 0x4a, 0x4a],
    [0xff, 0x5c, 0x5c],
    [0x8e, 0xe8, 0x5c],
    [0xff, 0xb3, 0x47],
    [0xff, 0x92, 0x48],
    [0xff, 0x5c, 0x8a],
    [0x5c, 0xf5, 0xe3],
    [0xff, 0xff, 0xff],
];
/// The terminal background of the test profile (what `P_bg` is told).
const TEST_BG: (u8, u8, u8) = (0x2c, 0x2c, 0x2c);

struct TestFrame {
    rgba: Vec<u8>,
    /// Every pixel the terminal drew something on: text (including its anti-aliased edge pixels), the cursor,
    /// the selection and inverse-video blocks.
    ink: Vec<bool>,
    /// Pixels inside an inverse-video block that are exactly the background color (the "hole" of a glyph drawn
    /// in the background color). A color mask cannot tell these from plain background; they are reported, not asserted.
    hole: Vec<bool>,
    /// Pixels in the filled blocks (selection, inverse video, cursor): large flat areas.
    fill: Vec<bool>,
}

fn hash2(x: u32, y: u32) -> f32 {
    let mut n = x.wrapping_mul(374761393).wrapping_add(y.wrapping_mul(668265263));
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    ((n ^ (n >> 16)) & 0xffff) as f32 / 65535.0
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A photo-like background: smooth gradients plus soft low-frequency blotches, no flat color anywhere.
fn photo_pixel(x: u32, y: u32) -> [u8; 3] {
    let (fx, fy) = (x as f32, y as f32);
    let cell = 28.0;
    let (cx, cy) = ((fx / cell).floor(), (fy / cell).floor());
    let (tx, ty) = (fx / cell - cx, fy / cell - cy);
    let n = |dx: f32, dy: f32| hash2((cx + dx) as u32, (cy + dy) as u32);
    let (tx, ty) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let blotch =
        (n(0.0, 0.0) * (1.0 - tx) + n(1.0, 0.0) * tx) * (1.0 - ty) + (n(0.0, 1.0) * (1.0 - tx) + n(1.0, 1.0) * tx) * ty;
    let base = 50.0 + 70.0 * (0.5 + 0.5 * (fx / 97.0 + fy / 61.0).sin());
    let v = base + (blotch - 0.5) * 50.0;
    [(v * 1.0) as u8, (v * 0.92) as u8, (v * 0.85) as u8]
}

/// A realistic synthetic terminal: text in ALL 16 palette colors (anti-aliased), an inverse-video block, a selection
/// highlight with dark text, and a cursor, on either the plain background or a photo-like textured one.
fn realistic_frame(w: u32, h: u32, textured: bool) -> TestFrame {
    let mut rgba = vec![255u8; (w * h * 4) as usize];
    let mut ink = vec![false; (w * h) as usize];
    let mut hole = vec![false; (w * h) as usize];
    let mut fillmask = vec![false; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let c = if textured { photo_pixel(x, y) } else { [TEST_BG.0, TEST_BG.1, TEST_BG.2] };
            let i = ((y * w + x) * 4) as usize;
            rgba[i..i + 3].copy_from_slice(&c);
        }
    }
    let (cw, ch) = (8u32, 14u32);
    let rows = (h - 8) / ch;
    let cols = (w - 16) / cw;
    let (mx, my) = (8u32, 4u32);
    let mut fill = |rgba: &mut Vec<u8>, ink: &mut Vec<bool>, x0: u32, y0: u32, rw: u32, rh: u32, c: [u8; 3]| {
        for y in y0..(y0 + rh).min(h) {
            for x in x0..(x0 + rw).min(w) {
                let i = (y * w + x) as usize;
                rgba[i * 4..i * 4 + 3].copy_from_slice(&c);
                ink[i] = true;
                fillmask[i] = true;
            }
        }
    };
    // the blocks the terminal fills before drawing text over them
    let sel = (5u32, 6u32, 36u32, 2u32); // col, row, cols, rows
    let inv = (10u32, 3u32, 24u32, 1u32);
    fill(&mut rgba, &mut ink, mx + sel.0 * cw, my + sel.1 * ch, sel.2 * cw, sel.3 * ch, PALETTE[3]);
    fill(&mut rgba, &mut ink, mx + inv.0 * cw, my + inv.1 * ch, inv.2 * cw, inv.3 * ch, PALETTE[7]);
    let in_rect =
        |c: u32, r: u32, rc: (u32, u32, u32, u32)| c >= rc.0 && c < rc.0 + rc.2 && r >= rc.1 && r < rc.1 + rc.3;
    for r in 0..rows {
        for c in 0..cols {
            if hash2(c, r + 99) < 0.18 {
                continue; // a gap between words
            }
            // text color: cycle through every palette entry; selection text is dark, inverse text is the background
            let mut color = PALETTE[((r * 7 + c / 5) % 16) as usize];
            if in_rect(c, r, sel) {
                color = PALETTE[0];
            } else if in_rect(c, r, inv) {
                color = [TEST_BG.0, TEST_BG.1, TEST_BG.2];
            }
            // a random 5x7 glyph bitmap
            let mut bits = [[0f32; 5]; 7];
            for (gy, row) in bits.iter_mut().enumerate() {
                for (gx, b) in row.iter_mut().enumerate() {
                    *b = if hash2(c * 5 + gx as u32, r * 7 + gy as u32) < 0.55 { 1.0 } else { 0.0 };
                }
            }
            for py in 0..ch {
                for px in 0..cw {
                    let (x, y) = (mx + c * cw + px, my + r * ch + py);
                    if x >= w || y >= h {
                        continue;
                    }
                    // bilinear over the bitmap, then a smoothstep: soft anti-aliased edges
                    let gx = (px as f32 + 0.5) / cw as f32 * 5.0 - 0.5;
                    let gy = (py as f32 + 0.5) / ch as f32 * 7.0 - 0.5;
                    let (x0, y0) = (gx.floor(), gy.floor());
                    let at = |xx: f32, yy: f32| -> f32 {
                        if xx < 0.0 || yy < 0.0 || xx > 4.0 || yy > 6.0 { 0.0 } else { bits[yy as usize][xx as usize] }
                    };
                    let (fx, fy) = (gx - x0, gy - y0);
                    let v = (at(x0, y0) * (1.0 - fx) + at(x0 + 1.0, y0) * fx) * (1.0 - fy)
                        + (at(x0, y0 + 1.0) * (1.0 - fx) + at(x0 + 1.0, y0 + 1.0) * fx) * fy;
                    let cov = smoothstep(0.3, 0.7, v);
                    if cov <= 0.0 {
                        continue;
                    }
                    let i = (y * w + x) as usize;
                    for k in 0..3 {
                        let old = rgba[i * 4 + k] as f32;
                        rgba[i * 4 + k] = (old + (color[k] as f32 - old) * cov).round() as u8;
                    }
                    ink[i] = true;
                    if in_rect(c, r, inv) && rgba[i * 4..i * 4 + 3] == [TEST_BG.0, TEST_BG.1, TEST_BG.2] {
                        hole[i] = true;
                    }
                }
            }
        }
    }
    // the cursor
    fill(&mut rgba, &mut ink, mx + 3 * cw, my + 9 * ch, cw, ch, PALETTE[3]);
    TestFrame { rgba, ink, hole, fill: fillmask }
}

/// Pixels within `n` pixels (Chebyshev distance) of `mask`, the mask itself excluded.
fn dilate(mask: &[bool], w: u32, h: u32, n: i32) -> Vec<bool> {
    let (w, h) = (w as i32, h as i32);
    let mut out = vec![false; mask.len()];
    for y in 0..h {
        for x in 0..w {
            if mask[(y * w + x) as usize] {
                continue;
            }
            'o: for dy in -n..=n {
                for dx in -n..=n {
                    let (xx, yy) = (x + dx, y + dy);
                    if xx >= 0 && yy >= 0 && xx < w && yy < h && mask[(yy * w + xx) as usize] {
                        out[(y * w + x) as usize] = true;
                        break 'o;
                    }
                }
            }
        }
    }
    out
}

/// The plain-background realistic frame at the default test size (kept for the older checks).
fn terminal_frame() -> Vec<u8> {
    realistic_frame(W, H, false).rgba
}

fn lum(c: &[u8]) -> f32 {
    (0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32) / 255.0
}

fn render(g: &Gpu, shader_src: &str, input: &[u8], time: f32) -> Vec<u8> {
    render_sized(g, shader_src, input, W, H, time)
}

fn render_sized(g: &Gpu, shader_src: &str, input: &[u8], w: u32, h: u32, time: f32) -> Vec<u8> {
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

    // the terminal texture, top row first: v = 0 is the top, the same as fragCoord.y = 0
    let flipped = input.to_vec();
    let tex = d.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
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
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = d.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    let mut globals = [0u8; 64];
    let put = |b: &mut [u8; 64], off: usize, v: f32| b[off..off + 4].copy_from_slice(&v.to_le_bytes());
    put(&mut globals, 0, w as f32);
    put(&mut globals, 4, h as f32);
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
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let tview = target.create_view(&wgpu::TextureViewDescriptor::default());
    let padded = (w * 4).next_multiple_of(256);
    let out = d.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (padded * h) as u64,
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
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    g.queue.submit(Some(enc.finish()));
    let slice = out.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("map"));
    d.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
    let data = slice.get_mapped_range().expect("mapped range");
    let mut result = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        result.extend_from_slice(&data[(row * padded) as usize..(row * padded + w * 4) as usize]);
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

fn context(image: bool) -> shaderparams::RenderContext {
    shaderparams::RenderContext { opacity_scale: 1.0, background: TEST_BG, background_image: image }
}

fn rendered(src: &str, values: &BTreeMap<String, String>) -> String {
    shaderparams::render_ctx(src, values, &context(false)).expect("the shader's annotations are valid")
}

fn rendered_image(src: &str, values: &BTreeMap<String, String>) -> String {
    shaderparams::render_ctx(src, values, &context(true)).expect("the shader's annotations are valid")
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
    // crt-scanlines modulates every pixel by design; soft-glow's optional text glow (off by default) reads
    // neighbours, so it is checked separately (its vignette is background-only)
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

/// How much an effect changed the pixels in `mask`: (how many pixels differ by more than one level, the largest
/// change in any channel, in 0..255 levels).
fn leakage(out: &[u8], input: &[u8], mask: &[bool]) -> (usize, i32) {
    let mut count = 0;
    let mut worst = 0;
    for (i, m) in mask.iter().enumerate() {
        if !*m {
            continue;
        }
        let d = (0..3).map(|k| (out[i * 4 + k] as i32 - input[i * 4 + k] as i32).abs()).max().unwrap_or(0);
        worst = worst.max(d);
        if d > 1 {
            count += 1;
        }
    }
    (count, worst)
}

#[test]
fn no_effect_touches_text_of_any_palette_color_nor_a_fringe_around_it_on_the_plain_background() {
    let g = gpu_or_skip!();
    let (w, h) = (480u32, 270u32);
    let tf = realistic_frame(w, h, false);
    let hard: Vec<bool> = tf.ink.iter().zip(&tf.hole).map(|(i, h)| *i && !*h).collect();
    // the ink you can actually see (not the faintest anti-aliasing), and the 1 px / 2 px fringes around it
    let visible: Vec<bool> = (0..hard.len())
        .map(|i| {
            hard[i]
                && (0..3).any(|k| (tf.rgba[i * 4 + k] as i32 - [TEST_BG.0, TEST_BG.1, TEST_BG.2][k] as i32).abs() > 15)
        })
        .collect();
    let ring1 = dilate(&visible, w, h, 1);
    let ring2: Vec<bool> = dilate(&visible, w, h, 2).iter().zip(&ring1).map(|(a, b)| *a && !*b).collect();
    assert!(hard.iter().filter(|b| **b).count() > 20_000, "the frame has plenty of text");
    // the frame really contains the colors a luma cut-off cannot protect
    let dim = PALETTE[8];
    assert!(tf.rgba.chunks(4).any(|p| p[..3] == dim), "dim #4a4a4a text is in the frame");
    assert!(tf.rgba.chunks(4).any(|p| p[..3] == PALETTE[0]), "near-black text is in the frame");
    for (name, src) in library().into_iter().filter(|(n, _)| background_only(n)) {
        let schema = shaderparams::parse_schema(&src).unwrap();
        for (label, values) in variants(&schema) {
            for t in [0.0, 2.5, 9.9, 123.4] {
                let out = render_sized(&g, &rendered(&src, &values), &tf.rgba, w, h, t);
                let (n, worst) = leakage(&out, &tf.rgba, &hard);
                assert_eq!(n, 0, "{name} [{label}] t={t}: the effect changed {n} text pixels (up to {worst}/255)");
                assert!(worst <= 1, "{name} [{label}] t={t}: text pixels changed by {worst}/255");
                let (_, ring_worst) = leakage(&out, &tf.rgba, &ring1);
                assert!(
                    ring_worst <= 1,
                    "{name} [{label}] t={t}: the 1 px fringe around visible text changed by {ring_worst}/255"
                );
                // the 2 px fringe may taper, but it must be clearly dimmer than open background: compare the light
                // added there with the light the same shader adds to the same pixels of an empty screen
                let empty = bg_frame(w, h);
                let free = render_sized(&g, &rendered(&src, &values), &empty, w, h, t);
                let (mut near, mut open) = (0i64, 0i64);
                for (i, m) in ring2.iter().enumerate() {
                    if *m {
                        for k in 0..3 {
                            near += (out[i * 4 + k] as i64 - tf.rgba[i * 4 + k] as i64).max(0);
                            open += (free[i * 4 + k] as i64 - empty[i * 4 + k] as i64).max(0);
                        }
                    }
                }
                assert!(
                    open == 0 || near * 10 <= open * 8,
                    "{name} [{label}] t={t}: the 2 px fringe is not tapering ({near} vs {open})"
                );
            }
        }
    }
}

#[test]
fn on_a_textured_background_with_an_image_text_is_still_protected_where_it_stands_out() {
    let g = gpu_or_skip!();
    let (w, h) = (480u32, 270u32);
    let tf = realistic_frame(w, h, true);
    // ink that stands out from the picture (a color mask cannot see text that is nearly the color of the picture
    // under it; that limit is reported by the leakage table, not asserted here)
    let contrast = |i: usize| -> f32 {
        let (x, y) = ((i as u32 % w) as i32, (i as u32 / w) as i32);
        let mut best = 0.0f32;
        for (dx, dy) in [(14, 0), (-14, 0), (0, 14), (0, -14)] {
            let (xx, yy) = ((x + dx).clamp(0, w as i32 - 1) as u32, (y + dy).clamp(0, h as i32 - 1) as u32);
            let r = photo_pixel(xx, yy);
            let d = (0..3).map(|k| (tf.rgba[i * 4 + k] as f32 - r[k] as f32).abs()).fold(0.0, f32::max) / 255.0;
            best = best.max(d);
        }
        best
    };
    // text on the picture, not the interiors of the big flat blocks (selection, inverse video, cursor): a flat block
    // looks like a patch of picture to a color mask, which is a stated limit
    let strong: Vec<bool> =
        (0..tf.ink.len()).map(|i| tf.ink[i] && !tf.hole[i] && !tf.fill[i] && contrast(i) > 0.25).collect();
    assert!(strong.iter().filter(|b| **b).count() > 5_000);
    for (name, src) in library().into_iter().filter(|(n, _)| background_only(n)) {
        let schema = shaderparams::parse_schema(&src).unwrap();
        for (label, values) in
            variants(&schema).into_iter().filter(|(l, _)| matches!(l.as_str(), "defaults" | "numbers at max"))
        {
            for t in [1.0, 8.5] {
                let out = render_sized(&g, &rendered_image(&src, &values), &tf.rgba, w, h, t);
                let (n, worst) = leakage(&out, &tf.rgba, &strong);
                // a pixel or two may sit on the soft edge of the mask's threshold; anything more is a leak
                assert!(
                    n * 200 <= strong.iter().filter(|b| **b).count() && worst <= 40,
                    "{name} [{label}] t={t}: {n} high-contrast text pixels changed (up to {worst}/255) on a textured background"
                );
            }
        }
    }
}

#[test]
fn effects_still_show_on_a_textured_background_with_an_image() {
    let g = gpu_or_skip!();
    let (w, h) = (480u32, 270u32);
    // the picture alone, no text: the effect must not simply disappear in image mode
    let mut tf = realistic_frame(w, h, true);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            tf.rgba[i..i + 3].copy_from_slice(&photo_pixel(x, y));
        }
    }
    for (name, src) in library().into_iter().filter(|(n, _)| background_only(n)) {
        let out = render_sized(&g, &rendered_image(&src, &BTreeMap::new()), &tf.rgba, w, h, 9.0);
        let changed = out.chunks(4).zip(tf.rgba.chunks(4)).filter(|(o, i)| o[..3] != i[..3]).count();
        eprintln!("{name:<24} changes {changed} pixels of the picture at its defaults");
        assert!(changed > 30, "{name}: nothing is drawn over a background image");
    }
}

#[test]
#[ignore]
fn print_text_leakage_before_and_after() {
    let g = gpu_or_skip!();
    let (w, h) = (480u32, 270u32);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("presets/legacy");
    let plain = realistic_frame(w, h, false);
    let photo = realistic_frame(w, h, true);
    let text = |tf: &TestFrame| -> Vec<bool> { tf.ink.iter().zip(&tf.hole).map(|(i, h)| *i && !*h).collect() };
    eprintln!("{:<22} {:>34} {:>34}", "shader", "BEFORE (text px changed / max)", "AFTER (text px changed / max)");
    for (name, src) in library() {
        let stem = name.trim_end_matches(".glsl");
        let measure = |s: &str, tf: &TestFrame, image: bool| -> (usize, i32) {
            let r = if image { rendered_image(s, &BTreeMap::new()) } else { rendered(s, &BTreeMap::new()) };
            let mut worst = (0usize, 0i32);
            for t in [1.0, 5.0, 9.9, 23.0] {
                let out = render_sized(&g, &r, &tf.rgba, w, h, t);
                let (n, m) = leakage(&out, &tf.rgba, &text(tf));
                worst = (worst.0.max(n), worst.1.max(m));
            }
            worst
        };
        let old = std::fs::read_to_string(dir.join(format!("{stem}-0.2.glsl"))).ok();
        let (on, om) = old.as_deref().map(|o| measure(o, &plain, false)).unwrap_or((0, -1));
        let (nn, nm) = measure(&src, &plain, false);
        let (pn, pm) = measure(&src, &photo, true);
        eprintln!(
            "{stem:<22} {on:>10} px / {om:>3}/255 {nn:>10} px / {nm:>3}/255   (textured bg + image mode: {pn} px / {pm}/255)"
        );
    }
}

#[test]
fn no_background_only_shader_blurs_text_it_only_ever_suppresses_its_own_effect_near_it() {
    let g = gpu_or_skip!();
    // an empty screen with ONE bright pixel on it. The effect's mask looks at neighbours (that is how it knows
    // text is near), but only to switch the effect off: no pixel may ever pick up the poked pixel's COLOR.
    let (w, h) = (320u32, 180u32);
    let frame = bg_frame(w, h);
    let mut poked = frame.clone();
    let (px, py) = (200u32, 120u32);
    let at = ((py * w + px) * 4) as usize;
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
            let a = render_sized(&g, &src, &frame, w, h, 5.0);
            let b = render_sized(&g, &src, &poked, w, h, 5.0);
            let mut differing = 0;
            for y in 0..h {
                for x in 0..w {
                    let i = ((y * w + x) * 4) as usize;
                    if a[i..i + 4] == b[i..i + 4] {
                        continue;
                    }
                    differing += 1;
                    let (dx, dy) = ((x as i32 - px as i32).abs(), (y as i32 - py as i32).abs());
                    assert!(
                        dx <= 2 && dy <= 2,
                        "{name}: ({x},{y}) changed, {dx},{dy} px from the poked pixel: the mask reaches too far"
                    );
                    if (x, y) != (px, py) {
                        // a neighbour either keeps its own input (the effect was switched off there) or is exactly what
                        // it was without the poked pixel: it never contains any of the poked pixel's color
                        assert!(
                            b[i..i + 4] == frame[i..i + 4] || b[i..i + 4] == a[i..i + 4],
                            "{name}: ({x},{y}) was blended with a neighbour"
                        );
                    }
                }
            }
            assert!(differing <= 25, "{name}: {differing} pixels changed");
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
        assert!(best > 15, "{name}: only {best} pixels changed at its default strength");
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

// ---- the numeric orientation / coverage / opacity checks --------------------------------------

/// A plain dark screen (no text): the effect layer on its own.
fn bg_frame(w: u32, h: u32) -> Vec<u8> {
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..w * h {
        px.extend_from_slice(&[TEST_BG.0, TEST_BG.1, TEST_BG.2, 255]);
    }
    px
}

/// Light the shader added, per pixel (never negative): what the effect layer looks like.
fn effect_layer(out: &[u8], input: &[u8]) -> Vec<f32> {
    out.chunks(4).zip(input.chunks(4)).map(|(o, i)| (lum(o) - lum(i)).max(0.0)).collect()
}

/// The dominant shift, in whole pixels, between two effect layers: the (dx, dy) offset (of the second relative
/// to the first) with the highest correlation. Screen space: the row index grows DOWNWARD, so a positive `dy`
/// means the effect moved down and a positive `dx` means right. `main` is the axis being judged (its range is
/// `range`); the other axis gets a small tolerance, because slanted rain and sway move sideways a little.
/// Returns (dx, dy, peak correlation, correlation at the opposite offset).
fn dominant_shift(a: &[f32], b: &[f32], w: usize, h: usize, vertical: bool, range: i32) -> (i32, i32, f32, f32) {
    let side = 8;
    let corr = |dx: i32, dy: i32| -> f32 {
        let mut sum = 0.0f64;
        let mut n = 0usize;
        for y in 0..h as i32 {
            let yy = y + dy;
            if yy < 0 || yy >= h as i32 {
                continue;
            }
            for x in 0..w as i32 {
                let xx = x + dx;
                if xx < 0 || xx >= w as i32 {
                    continue;
                }
                let v = a[y as usize * w + x as usize];
                if v > 0.0 {
                    sum += (v * b[yy as usize * w + xx as usize]) as f64;
                }
                n += 1;
            }
        }
        (sum / n.max(1) as f64) as f32
    };
    let (mut best, mut best_c) = ((0, 0), corr(0, 0));
    let (rx, ry) = if vertical { (side, range) } else { (range, side) };
    for dy in -ry..=ry {
        for dx in -rx..=rx {
            let c = corr(dx, dy);
            if c > best_c {
                best_c = c;
                best = (dx, dy);
            }
        }
    }
    (best.0, best.1, best_c, corr(-best.0, -best.1))
}

#[test]
fn declared_motion_matches_the_motion_measured_on_the_gpu_in_screen_space() {
    use ghostty_profiles::shaderparams::Motion;
    let g = gpu_or_skip!();
    let (w, h) = (320u32, 180u32);
    let bg = bg_frame(w, h);
    // (shader, seconds between the two frames, search range in pixels)
    let dt_for = |name: &str| match name {
        "pixel-rain.glsl" => (0.25f32, 30),
        "matrix-rain.glsl" => (0.8, 60),
        "snow.glsl" => (0.3, 20),
        "enchant-glyphs.glsl" => (0.8, 30),
        "starfield.glsl" => (0.3, 20),
        _ => (0.25, 20),
    };
    let mut checked = Vec::new();
    for (name, src) in library() {
        let schema = shaderparams::parse_schema(&src).unwrap();
        let Some(motion) = schema.motion else { panic!("{name}: every bundled shader declares its @motion") };
        let (vertical, want) = match motion {
            Motion::Down => (true, 1),
            Motion::Up => (true, -1),
            Motion::Left => (false, -1),
            Motion::Right => (false, 1),
            Motion::Radial | Motion::None => continue,
        };
        let (dt, range) = dt_for(&name);
        let s = rendered(&src, &BTreeMap::new());
        let (t0, t1) = (7.0f32, 7.0 + dt);
        let a = effect_layer(&render_sized(&g, &s, &bg, w, h, t0), &bg);
        let b = effect_layer(&render_sized(&g, &s, &bg, w, h, t1), &bg);
        assert!(a.iter().any(|v| *v > 0.02), "{name}: nothing is drawn at t={t0}");
        let (dx, dy, peak, opposite) = dominant_shift(&a, &b, w as usize, h as usize, vertical, range);
        let shift = if vertical { dy } else { dx };
        eprintln!(
            "{name}: declared {} -> measured shift dx {dx:+}, dy {dy:+} px (rows grow downward, columns rightward) over {dt}s",
            motion.name()
        );
        assert!(
            shift != 0 && shift.signum() == want,
            "{name}: declared '{}' but the effect moved {shift:+} px (rows grow downward; columns grow rightward)",
            motion.name()
        );
        assert!(peak > opposite * 1.2, "{name}: the direction is not clear-cut (peak {peak}, opposite {opposite})");
        checked.push(name);
    }
    for n in ["pixel-rain.glsl", "matrix-rain.glsl", "snow.glsl", "enchant-glyphs.glsl", "starfield.glsl"] {
        assert!(checked.iter().any(|c| c == n), "{n} was not measured");
    }
}

/// (mean light added over the background, fraction of pixels that got noticeably brighter), the worst over a few moments.
fn coverage(g: &Gpu, src: &str, w: u32, h: u32) -> (f32, f32) {
    let bg = bg_frame(w, h);
    let (mut mean_max, mut frac_max) = (0.0f32, 0.0f32);
    for t in [3.0, 11.0, 26.0, 47.0] {
        let e = effect_layer(&render_sized(g, src, &bg, w, h, t), &bg);
        let mean = e.iter().sum::<f32>() / e.len() as f32;
        let frac = e.iter().filter(|v| **v > 0.10).count() as f32 / e.len() as f32;
        mean_max = mean_max.max(mean);
        frac_max = frac_max.max(frac);
    }
    (mean_max, frac_max)
}

const MAX_MEAN_ADDED: f32 = 0.06;
const MAX_BRIGHT_COVERAGE: f32 = 0.08;

#[test]
fn particle_effects_do_not_block_the_screen_at_their_defaults() {
    let g = gpu_or_skip!();
    for (name, src) in library() {
        let schema = shaderparams::parse_schema(&src).unwrap();
        let (mean, frac) = coverage(&g, &rendered(&src, &BTreeMap::new()), 640, 360);
        eprintln!(
            "{name:<24} mean added light {:>5.2}%  bright-pixel coverage {:>5.2}%  {}",
            mean * 100.0,
            frac * 100.0,
            if schema.coverage_full { "(@coverage full: exempt)" } else { "" }
        );
        if schema.coverage_full {
            continue;
        }
        assert!(
            mean < MAX_MEAN_ADDED,
            "{name}: adds {:.1}% light on average (budget {}%)",
            mean * 100.0,
            MAX_MEAN_ADDED * 100.0
        );
        assert!(
            frac < MAX_BRIGHT_COVERAGE,
            "{name}: {:.1}% of the screen is lit (budget {}%)",
            frac * 100.0,
            MAX_BRIGHT_COVERAGE * 100.0
        );
    }
}

#[test]
fn the_coverage_budget_catches_a_shader_that_floods_the_screen() {
    let g = gpu_or_skip!();
    let flood = "void mainImage(out vec4 c, in vec2 p) { vec4 t = texture(iChannel0, p / iResolution.xy); c = vec4(t.rgb + vec3(0.3), t.a); }";
    let (mean, frac) = coverage(&g, flood, 320, 180);
    assert!(mean > MAX_MEAN_ADDED && frac > MAX_BRIGHT_COVERAGE, "{mean} {frac}");
}

#[test]
#[ignore]
fn print_coverage_before_and_after() {
    let g = gpu_or_skip!();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("presets/legacy");
    eprintln!("{:<22} {:>22} {:>22}", "shader", "before (mean / lit)", "after (mean / lit)");
    for (name, src) in library() {
        let stem = name.trim_end_matches(".glsl");
        let old = std::fs::read_to_string(dir.join(format!("{stem}-0.2.glsl")))
            .ok()
            .and_then(|o| shaderparams::render(&o, &BTreeMap::new()).ok());
        let (om, of) = old.map(|o| coverage(&g, &o, 640, 360)).unwrap_or((f32::NAN, f32::NAN));
        let (nm, nf) = coverage(&g, &rendered(&src, &BTreeMap::new()), 640, 360);
        eprintln!("{stem:<22} {:>9.2}% / {:>6.2}% {:>9.2}% / {:>6.2}%", om * 100.0, of * 100.0, nm * 100.0, nf * 100.0);
    }
}

#[test]
fn every_bundled_shader_has_an_opacity_that_scales_the_effect_and_never_the_text() {
    let g = gpu_or_skip!();
    let frame = terminal_frame();
    let text: Vec<usize> = (0..(W * H) as usize).filter(|i| lum(&frame[i * 4..i * 4 + 4]) >= 0.82).collect();
    for (name, src) in library() {
        let schema = shaderparams::parse_schema(&src).unwrap();
        assert!(shaderparams::has_opacity(&schema), "{name} has no opacity parameter");
        assert_eq!(schema.params[0].name, "opacity", "{name}: opacity is the first parameter (the top row in the TUI)");
        let with =
            |o: &str| -> BTreeMap<String, String> { [("opacity".to_string(), o.to_string())].into_iter().collect() };
        // opacity 0: the terminal exactly as it was
        let none = render(&g, &rendered(&src, &with("0")), &frame, 9.0);
        assert!(none == frame, "{name}: opacity 0 must be a pass-through");
        if !background_only(&name) {
            continue;
        }
        // text is never touched at any opacity
        for o in ["0.25", "0.6", "1"] {
            let out = render(&g, &rendered(&src, &with(o)), &frame, 9.0);
            assert_eq!(
                text.iter().filter(|&&i| out[i * 4..i * 4 + 4] != frame[i * 4..i * 4 + 4]).count(),
                0,
                "{name} at opacity {o}"
            );
        }
        // half the opacity adds about half the light
        let bg = bg_frame(320, 180);
        let strong = [
            ("opacity".to_string(), "1".to_string()),
            (
                "strength".to_string(),
                shaderparams::format_number(match schema.params.iter().find(|p| p.name == "strength").unwrap().kind {
                    Kind::Float { max, .. } => max,
                    _ => 1.0,
                }),
            ),
        ]
        .into_iter()
        .collect::<BTreeMap<_, _>>();
        let mut half = strong.clone();
        half.insert("opacity".into(), "0.5".into());
        let mean = |v: &BTreeMap<String, String>| -> f32 {
            let e = effect_layer(&render_sized(&g, &rendered(&src, v), &bg, 320, 180, 12.0), &bg);
            e.iter().sum::<f32>() / e.len() as f32
        };
        let (full, h) = (mean(&strong), mean(&half));
        if full > 0.002 {
            let ratio = h / full;
            assert!((0.35..0.65).contains(&ratio), "{name}: half opacity added {:.0}% of the light", ratio * 100.0);
        }
    }
}

#[test]
fn xmb_waves_looks_exactly_as_it_did_at_its_defaults() {
    let g = gpu_or_skip!();
    let old = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("presets/legacy/xmb-waves-0.2.glsl"),
    )
    .unwrap();
    let old = rendered(&old, &BTreeMap::new());
    let new = library().into_iter().find(|(n, _)| n == "xmb-waves.glsl").unwrap().1;
    let new = rendered(&new, &BTreeMap::new());
    assert!(
        new.contains("gp_effect") && new.contains("P_opacity = 1.000000"),
        "the new version goes through the opacity wrapper at 1.0"
    );
    // over the plain background the output is bit-for-bit what it was (the new mask only ever removes light
    // from text and its fringe, never from background)
    let empty = bg_frame(W, H);
    for t in [0.0, 1.3, 7.7, 42.0] {
        assert!(
            render(&g, &old, &empty, t) == render(&g, &new, &empty, t),
            "xmb-waves differs from the previous version on plain background at t={t}"
        );
    }
    // and on a screen full of text the text is now untouched where the old version washed it
    let tf = realistic_frame(W, H, false);
    let hard: Vec<bool> = tf.ink.iter().zip(&tf.hole).map(|(i, h)| *i && !*h).collect();
    let before = leakage(&render(&g, &old, &tf.rgba, 7.7), &tf.rgba, &hard);
    let after = leakage(&render(&g, &new, &tf.rgba, 7.7), &tf.rgba, &hard);
    eprintln!("xmb-waves text leakage: before {before:?} after {after:?}");
    assert!(after.0 == 0 && before.0 > 0);
    // also over a plain background at a larger size
    let bg = bg_frame(640, 360);
    assert!(render_sized(&g, &old, &bg, 640, 360, 5.5) == render_sized(&g, &new, &bg, 640, 360, 5.5));
}

#[test]
fn the_profile_wide_effects_opacity_multiplies_each_shaders_own() {
    let g = gpu_or_skip!();
    let bg = bg_frame(320, 180);
    let src = library().into_iter().find(|(n, _)| n == "fireflies.glsl").unwrap().1;
    let own: BTreeMap<String, String> = [("opacity".to_string(), "0.8".to_string())].into_iter().collect();
    let ctx = |scale: f64| shaderparams::RenderContext { opacity_scale: scale, ..context(false) };
    let scaled = shaderparams::render_ctx(&src, &own, &ctx(0.5)).unwrap();
    let direct: BTreeMap<String, String> = [("opacity".to_string(), "0.4".to_string())].into_iter().collect();
    let a = render_sized(&g, &scaled, &bg, 320, 180, 12.0);
    let b = render_sized(&g, &rendered(&src, &direct), &bg, 320, 180, 12.0);
    assert!(a == b, "0.8 x 0.5 must equal 0.4");
    assert!(scaled.contains("P_opacity = 0.400000"), "{scaled}");
    // a master of 0 silences every effect; 1 changes nothing
    let off = shaderparams::render_ctx(&src, &own, &ctx(0.0)).unwrap();
    assert!(render_sized(&g, &off, &bg, 320, 180, 12.0) == bg);
    assert!(
        render_sized(&g, &shaderparams::render_ctx(&src, &own, &ctx(1.0)).unwrap(), &bg, 320, 180, 12.0)
            == render_sized(&g, &rendered(&src, &own), &bg, 320, 180, 12.0)
    );
}

#[test]
fn the_previous_versions_really_did_fall_upward_in_ghostty_which_is_what_the_operator_saw() {
    // The 0.2 shaders were written as if fragCoord's y pointed UP. In Ghostty it points down, so rain, matrix and
    // snow fell towards the TOP of the screen. This reproduces that on the GPU, so the measurement above is
    // known to be able to tell the two directions apart.
    let g = gpu_or_skip!();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("presets/legacy");
    let bg = bg_frame(320, 180);
    for (stem, dt, range) in [("pixel-rain", 0.25f32, 30), ("matrix-rain", 0.8, 60), ("snow", 0.3, 20)] {
        let old = std::fs::read_to_string(dir.join(format!("{stem}-0.2.glsl"))).unwrap();
        let s = shaderparams::render(&old, &BTreeMap::new()).unwrap();
        let a = effect_layer(&render_sized(&g, &s, &bg, 320, 180, 7.0), &bg);
        let b = effect_layer(&render_sized(&g, &s, &bg, 320, 180, 7.0 + dt), &bg);
        let (dx, dy, _, _) = dominant_shift(&a, &b, 320, 180, true, range);
        eprintln!("previous {stem}: dx {dx:+}, dy {dy:+}");
        assert!(dy < 0, "{stem} 0.2 should have moved UP the screen (dy < 0), measured {dy:+}");
    }
}
