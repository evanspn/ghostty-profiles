// Aurora ribbons: bands of light hanging from the top of the window, drifting sideways.
// A ribbon-style variant of the aurora. Ghostty custom shader (Shadertoy-style).
// Tunable per profile (see xmb-waves.glsl for how).
//
// @color color_a #3dffa0 "Ribbon color"
// @color color_b #7a5cff "Accent"
// @float strength 0.34 0.0 0.5 "Strength"
// @float speed 0.14 0.02 1.0 "Speed"
// @preset borealis color_a=#3dffa0 color_b=#7a5cff strength=0.34
// @preset ice color_a=#7df9ff color_b=#4f7bff strength=0.34
// @preset ember color_a=#ffb347 color_b=#ff4f6a strength=0.18

float band(vec2 p, float t, float seed) {
    float x = p.x * 2.2 + seed;
    float sway = sin(x + t) * 0.5 + sin(x * 2.3 - t * 0.8) * 0.25;
    float y = 0.82 + 0.07 * sway;
    float d = p.y - y;
    // hard lower edge, soft fall-off above it: reads as a hanging ribbon
    float edge = smoothstep(-0.015, 0.0, d);
    float fade = exp(-max(d, 0.0) * 7.0);
    float shimmer = 0.7 + 0.3 * sin(p.x * 48.0 + seed * 5.0 + t * 1.6);
    return edge * fade * shimmer;
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);
    float t = iTime * P_speed;
    vec2 p = vec2(uv.x * (iResolution.x / iResolution.y) * 0.5, uv.y);

    vec3 col = vec3(0.0);
    for (int i = 0; i < 4; i++) {
        float f = float(i);
        float b = band(p + vec2(f * 0.27, 0.0), t + f * 1.3, f * 2.7);
        // each ribbon hangs from a slightly different height
        col += mix(P_color_a, P_color_b, 0.15 + 0.25 * f) * b * (0.55 - 0.1 * f);
    }
    col *= smoothstep(0.35, 0.9, uv.y);

    float lum = dot(term.rgb, vec3(0.299, 0.587, 0.114));
    float textGuard = 1.0 - smoothstep(0.35, 0.8, lum);
    fragColor = vec4(term.rgb + col * P_strength * textGuard, term.a);
}
