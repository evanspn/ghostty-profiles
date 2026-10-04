// Fireflies: warm dots drifting on slow loops, each pulsing on its own rhythm. Original code.
// Only dark background pixels are touched (text stays exactly as drawn); the dots are soft glows
// drawn behind the text, and nothing samples neighbouring pixels. Ghostty custom shader
// (Shadertoy-style). Tunable per profile.
//
// @color glow #ffe27a "Glow color"
// @color core #fff6c8 "Core color"
// @float strength 0.90 0.0 1.0 "Strength"
// @float speed 0.35 0.0 2.0 "Drift speed"
// @float density 0.25 0.02 0.8 "Density"
// @float size 0.012 0.005 0.04 "Dot size"
// @preset fireflies glow=#ffe27a core=#fff6c8 strength=0.90 density=0.25
// @preset lanterns glow=#ff9a4d core=#ffe0b0 strength=0.80 density=0.18 size=0.02
// @preset spirits glow=#7dffd2 core=#e0fff4 strength=0.80 density=0.22

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

    float cell = 0.14 * iResolution.y;
    vec2 p = fragCoord / cell;
    vec2 base = floor(p);
    float t = iTime * P_speed;
    float rad = P_size * iResolution.y / cell;
    vec3 add = vec3(0.0);
    for (int j = -1; j <= 1; j++) {
        for (int i = -1; i <= 1; i++) {
            vec2 id = base + vec2(float(i), float(j));
            float h = hash21(id + 7.0);
            if (h < 1.0 - P_density * 2.0) {
                continue;
            }
            vec2 home = id + vec2(0.5) + 0.3 * vec2(hash21(id + 1.0) - 0.5, hash21(id + 2.0) - 0.5);
            vec2 pos = home + 0.35 * vec2(sin(t * (0.6 + hash21(id + 3.0)) + h * 30.0),
                                          cos(t * (0.5 + hash21(id + 4.0)) + h * 50.0));
            float d = length(p - pos);
            float pulse = 0.35 + 0.65 * (0.5 + 0.5 * sin(iTime * (0.8 + 1.5 * h) + h * 60.0));
            float halo = exp(-(d * d) / (rad * rad * 6.0));
            float core = exp(-(d * d) / (rad * rad * 0.6));
            add += (P_glow * halo + P_core * core) * pulse;
        }
    }
    fragColor = vec4(term.rgb + add * P_strength * bgMask * 0.6, term.a);
}
