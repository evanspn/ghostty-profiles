// Soft vignette: a gentle darkening toward the edges. Static (no iTime), so it works fine without
// custom-shader-animation. Ghostty custom shader (Shadertoy-style). Tunable per profile.
//
// The optional text glow samples neighbouring pixels, which softens text, so it is OFF by default
// and costs nothing while it is off.
//
// @float vignette 0.28 0.0 0.6 "Vignette"
// @float bloom 0.0 0.0 0.3 "Text glow (softens text)"
// @preset vignette vignette=0.28 bloom=0.0
// @preset strong vignette=0.45 bloom=0.0
// @preset glow vignette=0.28 bloom=0.10

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);
    vec3 rgb = term.rgb;

    if (P_bloom > 0.0) {
        // cheap 8-tap blur of the terminal for the glow
        vec2 px = 2.0 / iResolution.xy;
        vec3 blur = vec3(0.0);
        for (int i = 0; i < 8; i++) {
            float a = float(i) * 0.7853982;
            blur += texture(iChannel0, uv + vec2(cos(a), sin(a)) * px * 2.0).rgb;
        }
        blur /= 8.0;
        rgb += max(blur - term.rgb, 0.0) * P_bloom * 6.0;
    }

    vec2 c = uv - 0.5;
    float vig = 1.0 - P_vignette * smoothstep(0.25, 0.75, length(c));
    fragColor = vec4(rgb * vig, term.a);
}
