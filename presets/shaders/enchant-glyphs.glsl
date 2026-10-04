// Enchanting-table inspired: small glowing runes drifting upward behind the text, with a few sparkles.
// The runes are drawn procedurally from simple line strokes on a 3x3 grid of points, in a
// "mysterious alphabet" style. No game fonts, textures or artwork are used. Original code.
// Only dark background pixels are touched (text stays exactly as drawn) and nothing samples
// neighbouring pixels. Ghostty custom shader (Shadertoy-style). Tunable per profile.
//
// @color glyph_a #b36bff "Glyph color"
// @color glyph_b #4df0ff "Glow color"
// @float strength 0.60 0.0 1.0 "Strength"
// @float speed 0.25 0.02 1.5 "Speed"
// @float density 0.30 0.05 0.8 "Density"
// @float size 0.09 0.04 0.25 "Glyph size"
// @preset enchanted glyph_a=#b36bff glyph_b=#4df0ff strength=0.60 density=0.30
// @preset emerald glyph_a=#5dff9a glyph_b=#2bd1ff strength=0.60 density=0.30
// @preset ember glyph_a=#ffa14d glyph_b=#ff5252 strength=0.60 density=0.25
// @preset sparse glyph_a=#b36bff glyph_b=#4df0ff strength=0.50 density=0.12

float hash21(vec2 p) {
    p = fract(p * vec2(0.3183099, 0.3678794));
    p += dot(p, p.yx + vec2(19.19, 7.77));
    return fract((p.x + p.y) * (p.x * 37.13 + p.y * 17.71 + 1.0));
}

// distance from point p to the segment a-b
float seg(vec2 p, vec2 a, vec2 b) {
    vec2 pa = p - a;
    vec2 ba = b - a;
    float l2 = dot(ba, ba);
    float h = l2 > 0.00001 ? clamp(dot(pa, ba) / l2, 0.0, 1.0) : 0.0;
    return length(pa - ba * h);
}

// a node of the 3x3 grid, chosen by a hash value in [0, 1)
vec2 node(float h) {
    vec2 g = vec2(floor(h * 3.0), floor(fract(h * 13.7) * 3.0));
    return g * 0.32 + 0.18;
}

// a rune: four strokes between grid nodes and one dot, in the unit square
float rune(vec2 q, float id) {
    float d = 1.0;
    for (int i = 0; i < 4; i++) {
        float f = float(i);
        vec2 a = node(hash21(vec2(id, f + 1.0)));
        vec2 b = node(hash21(vec2(f + 7.0, id)));
        d = min(d, seg(q, a, b));
    }
    vec2 dotp = node(hash21(vec2(id, 11.0)));
    d = min(d, length(q - dotp) - 0.03);
    float core = 1.0 - smoothstep(0.035, 0.07, d);
    float glow = exp(-d * 10.0) * 0.35;
    return core + glow;
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);
    float lum = dot(term.rgb, vec3(0.299, 0.587, 0.114));
    float bgMask = 1.0 - smoothstep(0.30, 0.60, lum);
    if (P_strength <= 0.0001 || bgMask <= 0.001) {
        fragColor = term;
        return;
    }

    float t = iTime * P_speed;
    float cell = P_size * iResolution.y;
    vec2 p = fragCoord / cell;
    // every column floats at its own pace
    float colId = floor(p.x);
    p.y -= t * (0.5 + hash21(vec2(colId, 3.0)));
    vec2 id = floor(p);
    vec2 q = fract(p);

    float h = hash21(id + 17.0);
    float present = step(1.0 - P_density, h);
    float shape = rune(q, hash21(id + 3.0) * 64.0);
    // each rune fades in and out as it drifts, and shimmers
    float life = 0.5 + 0.5 * sin(t * 4.0 * (0.5 + h) + h * 40.0);
    float shimmer = 0.75 + 0.25 * sin(iTime * (3.0 + 4.0 * h) + h * 20.0);
    float a = present * shape * life * shimmer;
    vec3 c = mix(P_glyph_a, P_glyph_b, hash21(id + 9.0));

    // a few sparkles
    vec2 sp = fragCoord / (cell * 0.35);
    sp.y -= t * 1.4;
    vec2 sid = floor(sp);
    float sh = hash21(sid + 41.0);
    float spark = step(0.985, sh) * (1.0 - smoothstep(0.0, 0.28, length(fract(sp) - 0.5)))
                  * (0.5 + 0.5 * sin(iTime * 6.0 + sh * 90.0));

    vec3 add = c * a + P_glyph_b * spark * 0.8;
    fragColor = vec4(term.rgb + add * P_strength * bgMask, term.a);
}
