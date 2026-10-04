// Dusk: purple and pink XMB-style waves, a little lower and wider than xmb-waves.
// Ghostty custom shader (Shadertoy-style). Tunable per profile (see xmb-waves.glsl for how).
//
// @color wave_a #9b5cff "Wave color"
// @color wave_b #ff5fa8 "Accent"
// @float strength 0.18 0.0 0.5 "Strength"
// @float speed 0.28 0.05 1.5 "Speed"
// @preset dusk wave_a=#9b5cff wave_b=#ff5fa8 strength=0.18
// @preset twilight wave_a=#5c6bff wave_b=#c45cff strength=0.18
// @preset rose wave_a=#ff7aa8 wave_b=#ffb27a strength=0.16

float ribbon(vec2 p, float t, float seed, float amp, float freq) {
    float y = 0.42
        + amp        * sin(p.x * freq       + t * 0.8 + seed)
        + amp * 0.60 * sin(p.x * freq * 2.1 - t * 0.5 + seed * 1.7)
        + amp * 0.20 * sin(p.x * freq * 4.3 + t * 0.3 + seed * 3.1);
    float d = abs(p.y - y);
    return exp(-d * 24.0) * 0.5 + exp(-d * 6.0) * 0.5;
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);
    float t = iTime * P_speed;
    vec2 p = vec2(uv.x * (iResolution.x / iResolution.y) * 0.55, uv.y);

    vec3 col = vec3(0.0);
    for (int i = 0; i < 4; i++) {
        float f = float(i);
        float r = ribbon(p, t + f * 0.8, f * 1.4, 0.13 + 0.02 * f, 2.0 + 0.4 * f);
        vec3 c = mix(P_wave_a, P_wave_b, 0.5 + 0.5 * sin(f * 1.1 + t * 0.4 + p.x * 1.2));
        col += c * r * (0.40 + 0.08 * f);
    }
    // a faint glow rising from the horizon
    col += mix(P_wave_a, P_wave_b, 0.5) * exp(-abs(uv.y - 0.1) * 5.0) * 0.25;
    col *= smoothstep(0.0, 0.2, uv.y) * smoothstep(1.0, 0.8, uv.y);

    float lum = dot(term.rgb, vec3(0.299, 0.587, 0.114));
    float textGuard = 1.0 - smoothstep(0.35, 0.8, lum);
    fragColor = vec4(term.rgb + col * P_strength * textGuard, term.a);
}
