// The original PS3 XMB look: slow white and blue ribbons over a deep blue gradient.
// Ghostty custom shader (Shadertoy-style). Tunable per profile (see xmb-waves.glsl for how).
//
// @color ribbon #dfeaff "Ribbon"
// @color glow #3b82f6 "Glow"
// @color bg_top #0c2a6b "Gradient top"
// @color bg_bottom #020817 "Gradient bottom"
// @float tint 0.30 0.0 0.8 "Background tint"
// @float strength 0.20 0.0 0.5 "Strength"
// @float speed 0.18 0.02 1.0 "Speed"
// @preset classic ribbon=#dfeaff glow=#3b82f6 bg_top=#0c2a6b bg_bottom=#020817 tint=0.30 strength=0.20
// @preset midnight ribbon=#9db4ff glow=#2a3cff bg_top=#050a2a bg_bottom=#000208 tint=0.40 strength=0.18
// @preset daybreak ribbon=#fff1d6 glow=#ff9a5c bg_top=#3a2a6b bg_bottom=#110a1f tint=0.30 strength=0.20

float ribbon(vec2 p, float t, float seed, float amp, float freq) {
    float y = 0.5
        + amp        * sin(p.x * freq       + t + seed)
        + amp * 0.50 * sin(p.x * freq * 1.7 - t * 0.7 + seed * 2.1);
    float d = abs(p.y - y);
    return exp(-d * 40.0) * 0.6 + exp(-d * 9.0) * 0.4;
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);
    float lum = dot(term.rgb, vec3(0.299, 0.587, 0.114));

    // the gradient only shows through where the terminal is dark, so text is untouched
    vec3 grad = mix(P_bg_bottom, P_bg_top, uv.y);
    float dark = 1.0 - smoothstep(0.10, 0.45, lum);
    vec3 base = mix(term.rgb, grad, P_tint * dark);

    float t = iTime * P_speed;
    vec2 p = vec2(uv.x * (iResolution.x / iResolution.y) * 0.5, uv.y);
    vec3 col = vec3(0.0);
    for (int i = 0; i < 3; i++) {
        float f = float(i);
        float r = ribbon(p, t + f * 0.9, f * 1.9, 0.12 + 0.03 * f, 1.6 + 0.3 * f);
        col += mix(P_ribbon, P_glow, 0.35 + 0.3 * f) * r * (0.5 - 0.1 * f);
    }
    col *= smoothstep(0.0, 0.2, uv.y) * smoothstep(1.0, 0.75, uv.y);

    float textGuard = 1.0 - smoothstep(0.35, 0.8, lum);
    fragColor = vec4(base + col * P_strength * textGuard, term.a);
}
