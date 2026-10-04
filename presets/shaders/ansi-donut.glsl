// ANSI donut: the classic spinning ASCII donut (donut.c style), drawn entirely inside the shader as a grid of
// text characters. A torus tumbles around two axes, is lit from above, and each character cell shows one glyph of
// the brightness ramp ".,-~:;=!*#$@" (the bitmaps are encoded below, no font is used). Original code.
// It sits behind the terminal text (only plain-background pixels are touched) and nothing samples neighbouring
// pixels. Ghostty custom shader (Shadertoy-style). Tunable per profile.
//
// ORIENTATION: Ghostty's fragCoord has its origin at the TOP-left, so y grows DOWNWARD on screen (unlike
// Shadertoy). This shader computes in y-UP coordinates through gp_yup() (generated into the header): up = +y,
// so the light comes from the top of the window and the donut is right side up. pos_y is measured from the top.
//
// @motion none
// @float opacity 0.8 0.0 1.0 "Opacity"
// @color color #39ff6a "Glyph color"
// @float ansi 0.0 0.0 1.0 "ANSI colors (1 = by brightness)"
// @float strength 0.85 0.0 1.0 "Strength"
// @float scale 0.36 0.1 0.9 "Size (of window height)"
// @float speed 1.0 0.0 4.0 "Spin speed"
// @float spin_a 0.9 -3.0 3.0 "Tumble (around x)"
// @float spin_b 0.45 -3.0 3.0 "Spin (around z)"
// @float pos_x 0.84 0.0 1.0 "Position from the left"
// @float pos_y 0.30 0.0 1.0 "Position from the top"
// @float cell 7 4 16 "Character width (px)"
// @float glyphs 12 4 12 "Characters in the ramp"
// @preset phosphor color=#39ff6a ansi=0
// @preset amber color=#ffb000 ansi=0
// @preset white color=#e8e8e8 ansi=0
// @preset cyan color=#35e0ff ansi=0
// @preset magenta color=#ff4fd8 ansi=0
// @preset ansi-16 color=#e8e8e8 ansi=1

// the ramp glyphs, 5 columns x 7 rows, one int per row (bit 4 = leftmost column): . , - ~ : ; = ! * # $ @
const int GLYPHS[84] = int[84](
    0, 0, 0, 0, 0, 12, 12,
    0, 0, 0, 0, 12, 4, 8,
    0, 0, 0, 31, 0, 0, 0,
    0, 0, 8, 21, 2, 0, 0,
    0, 12, 12, 0, 12, 12, 0,
    0, 12, 12, 0, 12, 4, 8,
    0, 0, 31, 0, 31, 0, 0,
    4, 4, 4, 4, 4, 0, 4,
    0, 4, 21, 14, 21, 4, 0,
    10, 10, 31, 10, 31, 10, 10,
    4, 15, 20, 14, 5, 30, 4,
    14, 17, 23, 21, 23, 16, 14
);

// the torus (major radius 2, minor radius 1) is rotated around x then z; the point is taken back into its frame
float torus(vec3 p, float a, float b) {
    float cb = cos(b), sb = sin(b), ca = cos(a), sa = sin(a);
    p = vec3(cb * p.x + sb * p.y, -sb * p.x + cb * p.y, p.z);   // undo the rotation around z
    p = vec3(p.x, ca * p.y + sa * p.z, -sa * p.y + ca * p.z);   // undo the rotation around x
    vec2 q = vec2(length(p.xz) - 2.0, p.y);
    return length(q) - 1.0;
}

vec3 ansiColor(float l) {
    // blue, cyan, green, yellow, white: the classic ANSI hues by brightness
    vec3 c = mix(vec3(0.25, 0.35, 1.0), vec3(0.2, 0.9, 1.0), smoothstep(0.0, 0.25, l));
    c = mix(c, vec3(0.2, 1.0, 0.35), smoothstep(0.25, 0.5, l));
    c = mix(c, vec3(1.0, 0.9, 0.25), smoothstep(0.5, 0.75, l));
    return mix(c, vec3(1.0), smoothstep(0.78, 1.0, l));
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
    vec2 centre = vec2(P_pos_x, 1.0 - P_pos_y) * iResolution.xy;
    float radius = P_scale * iResolution.y * 0.5;
    vec2 cellSize = vec2(P_cell, P_cell * 2.0);
    vec2 cellId = floor(fc / cellSize);
    vec2 cellCentre = (cellId + 0.5) * cellSize;
    vec2 p = (cellCentre - centre) / radius * 3.0;   // the torus spans about [-3, 3]
    if (length(p) > 3.2) {
        fragColor = term;
        return;
    }

    float a = iTime * P_speed * P_spin_a;
    float b = iTime * P_speed * P_spin_b;
    // march an orthographic ray along +z (the viewer is at -z)
    float t = 3.0;
    bool hit = false;
    for (int i = 0; i < 28; i++) {
        float d = torus(vec3(p, t - 6.0), a, b);
        if (d < 0.01) { hit = true; break; }
        t += d;
        if (t > 9.0) break;
    }
    if (!hit) {
        fragColor = term;
        return;
    }
    vec3 q = vec3(p, t - 6.0);
    vec2 e = vec2(0.02, 0.0);
    vec3 n = normalize(vec3(torus(q + e.xyy, a, b) - torus(q - e.xyy, a, b),
                            torus(q + e.yxy, a, b) - torus(q - e.yxy, a, b),
                            torus(q + e.yyx, a, b) - torus(q - e.yyx, a, b)));
    float lum = clamp(dot(n, normalize(vec3(0.0, 1.0, -1.0))), 0.0, 1.0);

    // which character of the ramp
    float steps = max(floor(P_glyphs + 0.5), 4.0);
    float k = min(floor(lum * steps), steps - 1.0);
    int g = int(floor(k * 11.0 / (steps - 1.0) + 0.5));

    // is this pixel inside a lit dot of that character's 5x7 bitmap (one column of margin on each side)?
    vec2 local = (fc - cellId * cellSize) / cellSize;
    float u = local.x * 6.0 - 0.5;
    float v = (1.0 - local.y) * 8.0 - 0.5;
    float on = 0.0;
    if (u >= 0.0 && u < 5.0 && v >= 0.0 && v < 7.0) {
        int bits = GLYPHS[g * 7 + int(floor(v))];
        on = float((bits >> (4 - int(floor(u)))) & 1);
    }
    vec3 col = mix(P_color, ansiColor(lum), step(0.5, P_ansi));
    fragColor = vec4(term.rgb + col * (0.45 + 0.55 * lum) * on * P_strength * bgMask, term.a);
}
