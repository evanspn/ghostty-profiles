// Aurora: slow green/violet curtains drifting across the top of the window.
// Additive and guarded so bright text is not washed out. Tune STRENGTH and SPEED.

const float STRENGTH = 0.22;
const float SPEED    = 0.12;

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
    float t = iTime * SPEED;
    vec2 p = vec2(uv.x * (iResolution.x / iResolution.y) * 0.5, uv.y);

    vec3 green  = vec3(0.20, 1.00, 0.55);
    vec3 violet = vec3(0.55, 0.30, 1.00);

    vec3 col = vec3(0.0);
    for (int i = 0; i < 3; i++) {
        float f = float(i);
        float c = curtain(p + vec2(f * 0.31, 0.0), t + f * 1.7, f * 2.9);
        col += mix(green, violet, 0.5 + 0.5 * sin(f * 2.0 + p.x * 3.0 + t)) * c * (0.55 - 0.12 * f);
    }

    float lum = dot(term.rgb, vec3(0.299, 0.587, 0.114));
    float textGuard = 1.0 - smoothstep(0.35, 0.8, lum);

    fragColor = vec4(term.rgb + col * STRENGTH * textGuard, term.a);
}
