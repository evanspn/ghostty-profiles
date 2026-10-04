// CRT scanlines: faint horizontal lines, a touch of curvature-free vignette and a
// very slow brightness flicker. Text stays legible: lines only darken between rows.
// Tune STRENGTH (line depth) and FLICKER (0 disables).

const float STRENGTH = 0.18;
const float FLICKER  = 0.012;
const float PITCH    = 3.0;   // scanline period in pixels

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);

    float line = 0.5 + 0.5 * sin(fragCoord.y * 6.2831853 / PITCH);
    float dark = 1.0 - STRENGTH * line;

    float flick = 1.0 + FLICKER * sin(iTime * 37.0) * sin(iTime * 2.3);

    vec2 c = uv - 0.5;
    float vig = 1.0 - 0.35 * dot(c, c) * 2.0;

    fragColor = vec4(term.rgb * dark * flick * vig, term.a);
}
