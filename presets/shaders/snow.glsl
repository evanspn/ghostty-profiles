// Snow: soft flakes falling in three layers, swaying as they go. Original code.
// Only dark background pixels are touched (text stays exactly as drawn); the flakes are soft discs
// drawn behind the text, and nothing samples neighbouring pixels, so it can never blur text.
// Ghostty custom shader (Shadertoy-style). Tunable per profile.
//
// @color flake #ffffff "Flake color"
// @color tint #cfe3ff "Tint"
// @float strength 0.80 0.0 1.0 "Strength"
// @float speed 0.40 0.0 2.0 "Fall speed"
// @float density 0.50 0.05 1.0 "Density"
// @float size 0.35 0.1 0.5 "Flake size"
// @preset snowfall flake=#ffffff tint=#cfe3ff strength=0.80 density=0.50
// @preset blizzard flake=#ffffff tint=#b8d4ff strength=0.90 speed=1.20 density=0.90
// @preset ash flake=#bdbdbd tint=#8a8a8a strength=0.70 speed=0.30 density=0.40

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
    for (int i = 0; i < 3; i++) {
        float f = float(i);
        float scale = 9.0 + 11.0 * f;
        vec2 p = fragCoord / iResolution.y * scale;
        p.y += iTime * P_speed * (0.5 + 0.35 * f) * scale * 0.2;     // falling: the pattern moves down
        p.x += sin(p.y * 0.8 + f * 2.0 + iTime * 0.4) * 0.35;          // swaying
        vec2 id = floor(p);
        vec2 q = fract(p);
        float present = step(1.0 - P_density * 0.6, hash21(id + f * 17.0));
        vec2 pos = vec2(0.25 + 0.5 * hash21(id + 3.0), 0.25 + 0.5 * hash21(id + 8.0));
        float r = P_size * (0.55 + 0.45 * hash21(id + 5.0)) * (1.0 - 0.2 * f);
        float disc = 1.0 - smoothstep(r * 0.4, r, length(q - pos));
        vec3 c = mix(P_flake, P_tint, hash21(id + 11.0));
        add += c * disc * present * (1.0 - 0.2 * f);
    }
    fragColor = vec4(term.rgb + add * P_strength * bgMask, term.a);
}
