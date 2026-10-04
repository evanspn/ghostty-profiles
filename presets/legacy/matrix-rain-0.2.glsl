// Matrix rain: columns of falling glyph-like blocks with a bright head and a fading trail, tintable.
// The glyphs are random 3x5 dot patterns (not any real font). Original code.
// Only dark background pixels are touched (text stays exactly as drawn) and nothing samples
// neighbouring pixels. Ghostty custom shader (Shadertoy-style). Tunable per profile.
//
// @color glyph #00ff41 "Glyph color"
// @color head #d6ffe0 "Head color"
// @float strength 0.60 0.0 1.0 "Strength"
// @float speed 0.80 0.1 3.0 "Speed"
// @float density 0.40 0.05 0.9 "Density"
// @float size 0.024 0.012 0.06 "Glyph size"
// @preset matrix glyph=#00ff41 head=#d6ffe0 strength=0.60 density=0.40
// @preset cyber glyph=#22e3d1 head=#d8fffb strength=0.60 density=0.40
// @preset amber glyph=#ffb000 head=#fff0c0 strength=0.60 density=0.40
// @preset red glyph=#ff3b3b head=#ffd6d6 strength=0.55 density=0.35

float hash21(vec2 p) {
    p = fract(p * vec2(0.3183099, 0.3678794));
    p += dot(p, p.yx + vec2(19.19, 7.77));
    return fract((p.x + p.y) * (p.x * 37.13 + p.y * 17.71 + 1.0));
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

    float cell = P_size * iResolution.y;
    vec2 g = fragCoord / vec2(cell * 0.6, cell);
    float colId = floor(g.x);
    float rows = floor(iResolution.y / cell);
    float rowFromTop = rows - floor(g.y) - 1.0;

    float h = hash21(vec2(colId, 1.0));
    float on = step(1.0 - P_density, hash21(vec2(colId, 5.0)));
    float rowsPerSec = (3.0 + 8.0 * h) * P_speed;
    float period = rows + 12.0 + h * 20.0;
    float head = mod(iTime * rowsPerSec + h * period, period);
    float behind = head - rowFromTop;
    float len = 8.0 + floor(h * 14.0);
    float trail = on * step(0.0, behind) * step(behind, len) * (1.0 - behind / len);
    float isHead = on * step(abs(behind), 0.5);

    // a random 3x5 pattern per cell that changes now and then
    vec2 lg = fract(g);
    float cx = floor(lg.x * 3.0);
    float cy = floor(lg.y * 5.0);
    float tick = floor(iTime * 2.0 + hash21(vec2(colId, rowFromTop)) * 5.0);
    float bit = step(0.5, hash21(vec2(colId * 7.0 + cx, rowFromTop * 13.0 + cy + tick)));
    float inside = step(0.12, lg.x) * step(lg.x, 0.88) * step(0.1, lg.y) * step(lg.y, 0.9);
    float glyph = bit * inside;

    vec3 c = mix(P_glyph, P_head, isHead);
    fragColor = vec4(term.rgb + c * glyph * max(trail, isHead) * P_strength * bgMask, term.a);
}
