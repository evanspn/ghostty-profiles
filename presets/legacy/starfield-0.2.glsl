// Starfield: three layers of stars drifting sideways at different speeds (parallax), with an optional
// warp that stretches them into streaks. Original code.
// Only dark background pixels are touched (text stays exactly as drawn) and nothing samples
// neighbouring pixels. Ghostty custom shader (Shadertoy-style). Tunable per profile.
//
// @color star #ffffff "Star color"
// @color tint #8ab4ff "Tint"
// @float strength 0.80 0.0 1.0 "Strength"
// @float speed 0.15 0.0 1.5 "Drift speed"
// @float density 0.40 0.05 1.0 "Density"
// @float warp 0.0 0.0 1.0 "Warp streaks (0 = off)"
// @preset deep-space star=#ffffff tint=#8ab4ff strength=0.80 density=0.40 warp=0
// @preset hyperdrive star=#ffffff tint=#7fa8ff strength=0.80 speed=0.8 density=0.40 warp=0.9
// @preset warm star=#fff1d6 tint=#ffb27a strength=0.70 density=0.35 warp=0

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

    vec3 add = vec3(0.0);
    float stretch = 1.0 + P_warp * 9.0;
    for (int i = 0; i < 3; i++) {
        float f = float(i);
        float scale = 16.0 + 20.0 * f;
        vec2 p = fragCoord / iResolution.y * scale;
        p.x += iTime * P_speed * (0.6 + 0.7 * f) * scale * 0.25;   // nearer layers drift faster
        vec2 id = floor(p);
        vec2 q = fract(p);
        float present = step(1.0 - P_density * 0.35, hash21(id + f * 31.0));
        vec2 pos = vec2(0.2 + 0.6 * hash21(id + 5.0), 0.2 + 0.6 * hash21(id + 9.0));
        float dx = (q.x - pos.x) / stretch;
        float dy = q.y - pos.y;
        float r = 0.03 + 0.05 * hash21(id + 13.0);
        float shape = exp(-(dx * dx + dy * dy) / (r * r));
        float twinkle = 0.7 + 0.3 * sin(iTime * (2.0 + 3.0 * hash21(id + 2.0)) + hash21(id) * 30.0);
        vec3 c = mix(P_star, P_tint, hash21(id + 21.0));
        add += c * shape * present * twinkle * (1.0 - 0.25 * f);
    }
    fragColor = vec4(term.rgb + add * P_strength * bgMask, term.a);
}
