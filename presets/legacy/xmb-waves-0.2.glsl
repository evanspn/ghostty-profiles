// PS3 XMB-style flowing waves: subtle additive ribbons over the terminal.
// Ghostty custom shader (Shadertoy-style). The colors and numbers are tunable per profile:
// ghostty-profiles reads the @ lines below and writes the P_* constants at the top of the
// profile's copy of this file. Presets are named sets of values.
//
// @color wave_a #ff6b1a "Wave color"
// @color wave_b #e31a24 "Accent"
// @float strength 0.16 0.0 0.5 "Strength"
// @float speed 0.35 0.05 1.5 "Speed"
// @preset ember wave_a=#ff6b1a wave_b=#e31a24 strength=0.16
// @preset ocean wave_a=#2fa8ff wave_b=#4f5bff strength=0.18
// @preset forest wave_a=#3ddc84 wave_b=#1f8f5a strength=0.16
// @preset sakura wave_a=#ff9ec7 wave_b=#ff5f9e strength=0.16
// @preset mono wave_a=#ffffff wave_b=#9a9a9a strength=0.14

float ribbon(vec2 p, float t, float seed, float amp, float freq) {
    float y = 0.5
        + amp        * sin(p.x * freq        + t * 0.9 + seed)
        + amp * 0.55 * sin(p.x * freq * 1.9  - t * 0.6 + seed * 2.3)
        + amp * 0.25 * sin(p.x * freq * 3.7  + t * 0.4 + seed * 4.1);
    float d = abs(p.y - y);
    // soft glowing core plus a wide haze
    return exp(-d * 28.0) * 0.55 + exp(-d * 7.0) * 0.45;
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);

    float t = iTime * P_speed;
    // keep proportions sane on wide windows
    vec2 p = vec2(uv.x * (iResolution.x / iResolution.y) * 0.6, uv.y);

    float w = 0.0;
    vec3 col = vec3(0.0);
    for (int i = 0; i < 5; i++) {
        float f = float(i);
        float r = ribbon(p, t + f * 0.7, f * 1.7, 0.10 + 0.015 * f, 2.2 + 0.35 * f);
        vec3 c = mix(P_wave_a, P_wave_b, 0.5 + 0.5 * sin(f * 1.3 + t * 0.5 + p.x * 1.5));
        col += c * r * (0.35 + 0.1 * f);
        w += r;
    }

    // fade toward top and bottom so it reads as a band, like the XMB
    float band = smoothstep(0.0, 0.25, uv.y) * smoothstep(1.0, 0.7, uv.y);
    col *= band;

    // dim the effect where the terminal is bright (text) to protect legibility
    float lum = dot(term.rgb, vec3(0.299, 0.587, 0.114));
    float textGuard = 1.0 - smoothstep(0.35, 0.8, lum);

    fragColor = vec4(term.rgb + col * P_strength * textGuard, term.a);
}
