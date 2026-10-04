// Aurora: slow curtains of light drifting across the top of the window, BEHIND the text.
// It only brightens pixels that are dark background: pixels of text (and their edges) are left exactly
// as the terminal drew them, and nothing here samples neighbouring pixels, so it can never blur text.
// Ghostty custom shader (Shadertoy-style). Tunable per profile (see xmb-waves.glsl for how).
//
// @color color_a #33ff8c "Curtain color"
// @color color_b #8c4dff "Accent"
// @float strength 0.30 0.0 0.5 "Strength"
// @float speed 0.12 0.02 1.0 "Speed"
// @preset borealis color_a=#33ff8c color_b=#8c4dff strength=0.30
// @preset arctic color_a=#6ef0ff color_b=#4d7dff strength=0.30
// @preset solar color_a=#ffc247 color_b=#ff4d6a strength=0.26

float curtain(vec2 p, float t, float seed) {
    float x = p.x * 2.4 + seed;
    float wave = sin(x + t) * 0.5 + sin(x * 2.1 - t * 0.7) * 0.25 + sin(x * 4.3 + t * 0.4) * 0.125;
    float y = 0.72 + 0.10 * wave;
    float d = p.y - y;
    // sharp lower edge, long soft fade upwards
    float edge = smoothstep(-0.02, 0.0, d);
    float fade = exp(-max(d, 0.0) * 6.0);
    float rays = 0.65 + 0.35 * sin(p.x * 60.0 + seed * 7.0 + t * 2.0);
    return edge * fade * rays * smoothstep(0.0, 0.18, 1.0 - p.y + 0.25);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);

    // only dark background pixels are touched: text stays exactly as drawn
    float lum = dot(term.rgb, vec3(0.299, 0.587, 0.114));
    float bgMask = 1.0 - smoothstep(0.30, 0.60, lum);
    if (P_strength <= 0.0001 || bgMask <= 0.001) {
        fragColor = term;
        return;
    }

    float t = iTime * P_speed;
    vec2 p = vec2(uv.x * (iResolution.x / iResolution.y) * 0.5, uv.y);
    vec3 col = vec3(0.0);
    for (int i = 0; i < 3; i++) {
        float f = float(i);
        float c = curtain(p + vec2(f * 0.31, 0.0), t + f * 1.7, f * 2.9);
        col += mix(P_color_a, P_color_b, 0.5 + 0.5 * sin(f * 2.0 + p.x * 3.0 + t)) * c * (0.55 - 0.12 * f);
    }
    fragColor = vec4(term.rgb + col * P_strength * bgMask, term.a);
}
