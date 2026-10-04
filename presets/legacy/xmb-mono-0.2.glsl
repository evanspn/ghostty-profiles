// Mono: grayscale XMB-style waves that take their color from one tint.
// Ghostty custom shader (Shadertoy-style). Tunable per profile (see xmb-waves.glsl for how).
//
// @color tint #ffffff "Tint"
// @float strength 0.12 0.0 0.5 "Strength"
// @float speed 0.30 0.05 1.5 "Speed"
// @preset mono tint=#ffffff strength=0.12
// @preset warm tint=#ffd9a8 strength=0.14
// @preset cool tint=#b8d4ff strength=0.14

float ribbon(vec2 p, float t, float seed, float amp, float freq) {
    float y = 0.5
        + amp        * sin(p.x * freq       + t * 0.9 + seed)
        + amp * 0.55 * sin(p.x * freq * 1.9 - t * 0.6 + seed * 2.3);
    float d = abs(p.y - y);
    return exp(-d * 30.0) * 0.55 + exp(-d * 8.0) * 0.45;
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);
    float t = iTime * P_speed;
    vec2 p = vec2(uv.x * (iResolution.x / iResolution.y) * 0.6, uv.y);

    float w = 0.0;
    for (int i = 0; i < 5; i++) {
        float f = float(i);
        w += ribbon(p, t + f * 0.7, f * 1.7, 0.10 + 0.015 * f, 2.2 + 0.35 * f) * (0.35 + 0.1 * f);
    }
    w *= smoothstep(0.0, 0.25, uv.y) * smoothstep(1.0, 0.7, uv.y);

    float lum = dot(term.rgb, vec3(0.299, 0.587, 0.114));
    float textGuard = 1.0 - smoothstep(0.35, 0.8, lum);
    fragColor = vec4(term.rgb + P_tint * w * P_strength * textGuard, term.a);
}
