// Soft glow: a gentle edge vignette plus a faint bloom around bright text.
// Static (no iTime), so it works fine without custom-shader-animation.

const float VIGNETTE = 0.28;
const float BLOOM    = 0.10;

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);

    // cheap 8-tap blur of the terminal for the bloom
    vec2 px = 2.0 / iResolution.xy;
    vec3 blur = vec3(0.0);
    for (int i = 0; i < 8; i++) {
        float a = float(i) * 0.7853982;
        blur += texture(iChannel0, uv + vec2(cos(a), sin(a)) * px * 2.0).rgb;
    }
    blur /= 8.0;
    vec3 glow = max(blur - term.rgb, 0.0) * BLOOM * 6.0;

    vec2 c = uv - 0.5;
    float vig = 1.0 - VIGNETTE * smoothstep(0.25, 0.75, length(c));

    fragColor = vec4((term.rgb + glow) * vig, term.a);
}
