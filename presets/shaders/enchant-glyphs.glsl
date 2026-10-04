// Standard Galactic Alphabet glyphs, as shown on the Minecraft enchanting table; original stroke data, no Mojang assets.
//
// Enchanting-table inspired: strings of glowing glyphs (the 26 letters of the Standard Galactic Alphabet, drawn
// from hand-built stroke data below) drifting UP behind the text, with a few sparkles. The words are random
// letters; the same screen position always spells the same letters (no randomness beyond a fixed hash).
// Only plain-background pixels are touched (text of any color, cursors and selections stay exactly as drawn)
// and nothing blends colors with neighbouring pixels. Ghostty custom shader (Shadertoy-style). Tunable per profile.
//
// ORIENTATION: Ghostty's fragCoord has its origin at the TOP-left, so y grows DOWNWARD on screen (unlike
// Shadertoy). This shader computes in y-UP coordinates through gp_yup() (generated into the header): up = +y,
// so RISING = +y = up the screen (the glyphs float up on purpose).
//
// @motion up
// @float opacity 0.6 0.0 1.0 "Opacity"
// @color glyph_a #b36bff "Glyph color"
// @color glyph_b #4df0ff "Glow color"
// @float strength 0.60 0.0 1.0 "Strength"
// @float speed 0.25 0.02 1.5 "Speed"
// @float density 0.12 0.03 0.6 "Density"
// @float size 0.09 0.04 0.25 "Glyph size"
// @preset enchanted glyph_a=#b36bff glyph_b=#4df0ff strength=0.60 density=0.12
// @preset emerald glyph_a=#5dff9a glyph_b=#2bd1ff strength=0.60 density=0.12
// @preset ember glyph_a=#ffa14d glyph_b=#ff5252 strength=0.60 density=0.10
// @preset sparse glyph_a=#b36bff glyph_b=#4df0ff strength=0.50 density=0.05

float hash21(vec2 p) {
    p = fract(p * vec2(0.3183099, 0.3678794));
    p += dot(p, p.yx + vec2(19.19, 7.77));
    return fract((p.x + p.y) * (p.x * 37.13 + p.y * 17.71 + 1.0));
}

// ---- SGA glyph data (begin) ----
// Standard Galactic Alphabet glyphs, as shown on the Minecraft enchanting table: original stroke data, no Mojang assets.
// Each letter A-Z is a few segments on a 12x12 grid (y up); a zero-length segment is a dot. SGA_W is the glyph width.
const vec4 SGA_SEG[81] = vec4[81](
    vec4(1.5, 1.5, 3.5, 1.5), vec4(3.5, 1.5, 3.5, 7.8), vec4(3.5, 7.8, 5.0, 10.0),
    vec4(5.0, 10.0, 7.0, 10.6), vec4(7.0, 10.6, 9.0, 8.9), vec4(9.0, 8.9, 10.3, 7.2),
    vec4(5.5, 11.0, 5.5, 8.0), vec4(5.5, 8.0, 9.6, 3.4), vec4(1.5, 1.5, 10.5, 1.5),
    vec4(1.5, 10.8, 1.5, 10.8), vec4(1.5, 7.6, 1.5, 7.6), vec4(4.5, 1.8, 4.5, 5.2),
    vec4(1.5, 10.5, 10.5, 10.5), vec4(1.5, 6.4, 10.5, 1.6), vec4(1.5, 1.5, 1.5, 10.5),
    vec4(1.5, 1.5, 10.5, 1.5), vec4(10.5, 10.5, 10.5, 10.5), vec4(1.5, 10.5, 10.5, 10.5),
    vec4(1.5, 5.5, 1.5, 5.5), vec4(4.9, 5.5, 6.1, 5.5), vec4(10.5, 5.5, 10.5, 5.5),
    vec4(7.5, 1.5, 7.5, 10.5), vec4(1.5, 6.5, 7.5, 6.5), vec4(1.5, 10.5, 10.5, 10.5),
    vec4(1.5, 5.5, 10.5, 5.5), vec4(6.0, 1.5, 6.0, 5.5), vec4(2.0, 8.6, 2.0, 10.4),
    vec4(2.0, 1.6, 2.0, 3.4), vec4(2.0, 10.5, 2.0, 10.5), vec4(2.0, 5.4, 2.0, 6.6),
    vec4(2.0, 1.5, 2.0, 1.5), vec4(6.0, 1.5, 6.0, 10.5), vec4(1.5, 6.5, 1.5, 6.5),
    vec4(10.5, 6.5, 10.5, 6.5), vec4(1.5, 1.5, 1.5, 10.5), vec4(7.5, 8.5, 7.5, 8.5),
    vec4(7.5, 2.5, 7.5, 2.5), vec4(10.5, 1.5, 10.5, 10.5), vec4(1.5, 1.5, 10.5, 1.5),
    vec4(1.5, 9.5, 1.5, 9.5), vec4(1.5, 8.6, 1.5, 10.4), vec4(8.5, 7.8, 8.5, 10.4),
    vec4(8.0, 6.8, 1.8, 0.9), vec4(1.5, 10.6, 6.5, 10.6), vec4(6.5, 10.6, 8.4, 9.2),
    vec4(8.4, 9.2, 8.4, 7.8), vec4(8.4, 7.8, 6.6, 6.2), vec4(6.6, 6.2, 1.8, 0.9),
    vec4(2.0, 10.2, 2.0, 10.9), vec4(2.0, 1.4, 2.0, 8.4), vec4(8.0, 3.6, 8.0, 10.8),
    vec4(8.0, 0.8, 8.0, 1.3), vec4(5.0, 10.7, 7.0, 10.7), vec4(1.5, 6.5, 10.5, 6.5),
    vec4(10.5, 1.5, 10.5, 6.5), vec4(1.5, 1.5, 10.5, 1.5), vec4(1.5, 10.5, 1.5, 10.5),
    vec4(8.5, 10.5, 8.5, 10.5), vec4(1.5, 1.5, 1.5, 1.5), vec4(8.5, 1.5, 8.5, 1.5),
    vec4(1.5, 7.0, 1.5, 10.6), vec4(4.5, 1.4, 4.5, 5.4), vec4(1.5, 10.5, 10.5, 10.5),
    vec4(10.5, 5.5, 10.5, 10.5), vec4(10.5, 1.5, 10.5, 1.7), vec4(2.5, 8.5, 2.5, 8.5),
    vec4(8.5, 8.5, 8.5, 8.5), vec4(1.5, 3.5, 9.5, 3.5), vec4(6.0, 9.2, 6.0, 10.8),
    vec4(1.5, 6.5, 10.5, 6.5), vec4(1.5, 1.5, 10.5, 1.5), vec4(4.0, 9.5, 5.0, 9.5),
    vec4(1.5, 4.5, 2.4, 4.5), vec4(7.6, 4.5, 8.5, 4.5), vec4(1.5, 10.5, 1.5, 10.5),
    vec4(10.4, 10.5, 5.3, 1.5), vec4(1.5, 1.5, 1.5, 10.5), vec4(6.5, 1.5, 6.5, 10.5),
    vec4(4.0, 10.6, 5.0, 10.6), vec4(1.5, 1.5, 1.5, 7.6), vec4(8.5, 1.5, 8.5, 7.6)
);
const int SGA_START[27] = int[27](0, 6, 9, 12, 14, 17, 21, 23, 26, 28, 31, 34, 37, 40, 43, 48, 52, 56, 60, 62, 65, 68, 71, 74, 76, 78, 81);
const float SGA_W[26] = float[26](12.0, 12.0, 6.0, 12.0, 12.0, 12.0, 9.0, 12.0, 4.0, 4.0, 12.0, 9.0, 12.0, 10.0, 10.0, 10.0, 12.0, 10.0, 6.0, 12.0, 11.0, 12.0, 10.0, 12.0, 8.0, 10.0);

float sga_seg(vec2 p, vec2 a, vec2 b) {
    vec2 pa = p - a;
    vec2 ba = b - a;
    float l2 = dot(ba, ba);
    float h = l2 > 0.00001 ? clamp(dot(pa, ba) / l2, 0.0, 1.0) : 0.0;
    return length(pa - ba * h);
}

// distance, in grid units, from the point u (y up, 12x12 grid) to the strokes of letter 0..25 (0 = A)
float sga_dist(vec2 u, int letter) {
    float w = SGA_W[letter];
    vec2 p = vec2(u.x - (12.0 - w) * 0.5, u.y);
    float d = 99.0;
    for (int i = SGA_START[letter]; i < SGA_START[letter + 1]; i++) {
        vec4 s = SGA_SEG[i];
        d = min(d, sga_seg(p, s.xy, s.zw));
    }
    return d;
}
// ---- SGA glyph data (end) ----

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);
    // 1.0 only on plain terminal background: text of any color, the cursor, selections and a thin fringe
    // around them are masked out (gp_textMask is generated into the header)
    float bgMask = 1.0 - gp_textMask(fragCoord, term);
    if (P_strength <= 0.0001 || bgMask <= 0.001) {
        fragColor = term;
        return;
    }

    vec2 fc = gp_yup(fragCoord);
    float t = iTime * P_speed;
    float cell = P_size * iResolution.y;
    vec2 p = fc / cell;
    // everything rises together (the pattern moves down in y-up space, so the glyphs go up); each row of
    // words also slides slowly sideways at its own pace
    p.y -= t * 0.6;
    float row = floor(p.y);
    p.x += (hash21(vec2(row, 17.0)) - 0.5) * t * 0.8;
    vec2 id = floor(p);
    vec2 q = fract(p);

    // words: blocks of 8 cells; a block sometimes holds a word of 3 to 7 letters
    float blk = floor(id.x / 8.0);
    float posInBlk = id.x - blk * 8.0;
    float hb = hash21(vec2(blk, row + 31.0));
    float hasWord = step(1.0 - P_density * 1.5, hb);
    float start = floor(hash21(vec2(blk, row + 7.0)) * 2.0);
    float len = 3.0 + floor(hash21(vec2(blk + 5.0, row)) * 5.0);
    float inWord = hasWord * step(start, posInBlk) * step(posInBlk, start + len - 1.0);

    int letter = int(min(floor(hash21(id + 3.0) * 26.0), 25.0));
    vec2 u = (q - vec2(0.14)) / 0.72 * 12.0;
    float d = sga_dist(u, letter);
    float core = 1.0 - smoothstep(1.15, 1.85, d);
    float glow = exp(-d * 0.45) * 0.30;
    // a word fades in and out as it floats, and each glyph shimmers
    float life = 0.55 + 0.45 * sin(t * 3.0 + hb * 40.0);
    float shimmer = 0.8 + 0.2 * sin(iTime * (3.0 + 4.0 * hash21(id)) + hash21(id + 1.0) * 20.0);
    float a = inWord * (core + glow) * life * shimmer;
    vec3 c = mix(P_glyph_a, P_glyph_b, 0.25 + 0.5 * hash21(vec2(blk, row + 9.0)) + 0.25 * core);

    // a few sparkles
    vec2 sp = fc / (cell * 0.35);
    sp.y -= t * 1.4;
    vec2 sid = floor(sp);
    float sh = hash21(sid + 41.0);
    float spark = step(0.992, sh) * (1.0 - smoothstep(0.0, 0.28, length(fract(sp) - 0.5)))
                  * (0.5 + 0.5 * sin(iTime * 6.0 + sh * 90.0));

    vec3 add = c * a + P_glyph_b * spark * 0.8;
    fragColor = vec4(term.rgb + add * P_strength * bgMask, term.a);
}
