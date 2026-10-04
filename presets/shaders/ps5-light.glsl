// PS5 light: the calm, restrained feel of a console home screen: a few wide soft beams of white-blue light that
// sway very slowly, with a handful of tiny particles floating up through them. An optional iridescent tint
// shifts the beams through pearly pastel hues. Original code.
// Only plain-background pixels are touched (text of any color, cursors and selections stay exactly as drawn) and
// nothing samples neighbouring pixels. Ghostty custom shader (Shadertoy-style). Tunable per profile.
//
// ORIENTATION: Ghostty's fragCoord has its origin at the TOP-left, so y grows DOWNWARD on screen (unlike
// Shadertoy). This shader computes in y-UP coordinates through gp_yup() (generated into the header): up = +y,
// so the particles float UP the screen and the beams are brightest in the middle of the window.
//
// @motion radial
// @coverage full
// @float opacity 0.8 0.0 1.0 "Opacity"
// @color light #d6e6ff "Beam light"
// @color deep #3a63ff "Deep blue"
// @float strength 0.34 0.0 0.8 "Strength"
// @float speed 0.20 0.02 1.0 "Speed"
// @float tint 0.30 0.0 1.0 "Iridescent tint"
// @float particles 0.35 0.0 1.0 "Particles"
// @preset home light=#d6e6ff deep=#3a63ff strength=0.34 tint=0.30 particles=0.35
// @preset pearl light=#ffffff deep=#7a8cff strength=0.32 tint=0.75 particles=0.35
// @preset midnight light=#9fbcff deep=#1f3dd0 strength=0.34 tint=0.0 particles=0.25

float hash11(float n) { return fract(sin(n * 127.1) * 43758.5453); }

// one soft beam, 0..1: a wide gaussian across x around a slowly swaying, slightly tilted centre line
float beam(vec2 p, float t, float i, float aspect) {
    float cx = (i + 0.5) * 0.25 * aspect + 0.10 * (hash11(i + 3.0) - 0.5) + 0.10 * sin(t * 0.55 + i * 2.1) + (p.y - 0.5) * (0.5 - hash11(i + 9.0)) * 0.9;
    float w = 0.11 + 0.08 * hash11(i + 5.0);
    float d = (p.x - cx) / w;
    float body = exp(-d * d);
    float env = smoothstep(0.0, 0.35, p.y) * smoothstep(1.0, 0.55, p.y);
    float pulse = 0.65 + 0.35 * sin(t * 0.8 + i * 1.7);
    return body * env * pulse;
}

// a few tiny glowing dots rising through a column of the window
float dots(vec2 fc, float colWidth, float t, float seed, float density) {
    float id = floor(fc.x / colWidth);
    float h = hash11(id + seed * 17.0);
    if (h > density) return 0.0;
    float h2 = hash11(id * 1.7 + seed);
    float x = (id + 0.25 + 0.5 * h2) * colWidth + 8.0 * sin(t * 1.3 + h * 40.0);
    float y = fract(h2 * 5.0 + t * (0.025 + 0.03 * h)) * iResolution.y;
    float d = length(fc - vec2(x, y));
    float twinkle = 0.6 + 0.4 * sin(t * 3.0 + h * 60.0);
    return exp(-d * d / 14.0) * twinkle;
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);
    float bgMask = 1.0 - gp_textMask(fragCoord, term);
    if (P_strength <= 0.0001 || bgMask <= 0.001) {
        fragColor = term;
        return;
    }

    vec2 fc = gp_yup(fragCoord);
    float t = iTime * P_speed;
    vec2 p = vec2(fc.x / iResolution.y, fc.y / iResolution.y);
    float up = fc.y / iResolution.y;

    float beams = 0.0;
    for (int i = 0; i < 4; i++) {
        beams += beam(p, t, float(i), iResolution.x / iResolution.y) * (0.8 - 0.12 * float(i));
    }
    // pearly pastel hues that drift slowly across the window
    vec3 iri = 0.62 + 0.38 * cos(6.2832 * (vec3(0.0, 0.33, 0.67) + p.x * 0.55 + t * 0.12));
    vec3 beamColor = mix(P_light, P_light * iri, P_tint);
    vec3 col = beamColor * beams * 1.2;
    col += P_deep * 0.10 * exp(-up * 2.4) * (0.7 + 0.3 * sin(t * 0.6));     // a faint glow from the bottom edge
    float d = dots(fc, 70.0, t, 1.0, P_particles) + 0.7 * dots(fc, 110.0, t * 0.7, 2.0, P_particles);
    col += mix(P_light, vec3(1.0), 0.5) * d * 1.6 * step(0.0001, P_particles);
    fragColor = vec4(term.rgb + col * P_strength * bgMask, term.a);
}
