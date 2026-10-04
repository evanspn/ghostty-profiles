// PS3 XMB-style flowing waves. Subtle additive ribbons over the terminal.
// Tune STRENGTH (overall visibility) and SPEED. Ghostty custom shader (Shadertoy-style).

const float STRENGTH = 0.16;   // 0 = invisible, ~0.3 = strong
const float SPEED    = 0.35;   // flow speed

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

    float t = iTime * SPEED;
    // keep proportions sane on wide windows
    vec2 p = vec2(uv.x * (iResolution.x / iResolution.y) * 0.6, uv.y);

    vec3 ember = vec3(1.00, 0.42, 0.10);   // matches cursor-color #ff6a00
    vec3 red   = vec3(0.89, 0.10, 0.14);   // deep red, near palette 1 #e23636

    float w = 0.0;
    vec3 col = vec3(0.0);
    for (int i = 0; i < 5; i++) {
        float f = float(i);
        float r = ribbon(p, t + f * 0.7, f * 1.7, 0.10 + 0.015 * f, 2.2 + 0.35 * f);
        vec3 c = mix(ember, red, 0.5 + 0.5 * sin(f * 1.3 + t * 0.5 + p.x * 1.5));
        col += c * r * (0.35 + 0.1 * f);
        w += r;
    }

    // fade toward top and bottom so it reads as a band, like the XMB
    float band = smoothstep(0.0, 0.25, uv.y) * smoothstep(1.0, 0.7, uv.y);
    col *= band;

    // dim the effect where the terminal is bright (text) to protect legibility
    float lum = dot(term.rgb, vec3(0.299, 0.587, 0.114));
    float textGuard = 1.0 - smoothstep(0.35, 0.8, lum);

    fragColor = vec4(term.rgb + col * STRENGTH * textGuard, term.a);
}
