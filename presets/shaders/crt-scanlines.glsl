// CRT scanlines: faint horizontal lines, a touch of vignette and a very slow brightness flicker.
// Unlike the effects that sit behind the text, this one modulates every pixel (that is what a CRT does),
// so it softens text a little by design. Ghostty custom shader (Shadertoy-style). Tunable per profile.
//
// @motion none
// @coverage full
// @float opacity 1.0 0.0 1.0 "Opacity"
// @float strength 0.18 0.0 0.5 "Line depth"
// @float flicker 0.012 0.0 0.05 "Flicker"
// @float pitch 3.0 2.0 8.0 "Line pitch (px)"
// @preset crt strength=0.18 flicker=0.012 pitch=3
// @preset subtle strength=0.10 flicker=0.0 pitch=3
// @preset heavy strength=0.30 flicker=0.02 pitch=4

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);

    float line = 0.5 + 0.5 * sin(fragCoord.y * 6.2831853 / P_pitch);
    float dark = 1.0 - P_strength * line;

    float flick = 1.0 + P_flicker * sin(iTime * 37.0) * sin(iTime * 2.3);

    vec2 c = uv - 0.5;
    float vig = 1.0 - 0.35 * dot(c, c) * 2.0;

    fragColor = vec4(term.rgb * dark * flick * vig, term.a);
}
