// PS4 polygons: slowly drifting translucent low-poly facets over a soft blue glow, in the spirit of the PS4-era
// dynamic themes. Two layers of triangles drift at different speeds (a little parallax), each facet fades in and out
// on its own and a faint shimmer runs through them. Original code.
// Only plain-background pixels are touched (text of any color, cursors and selections stay exactly as drawn) and
// nothing samples neighbouring pixels. Ghostty custom shader (Shadertoy-style). Tunable per profile.
//
// ORIENTATION: Ghostty's fragCoord has its origin at the TOP-left, so y grows DOWNWARD on screen (unlike
// Shadertoy). This shader computes in y-UP coordinates through gp_yup() (generated into the header): up = +y,
// so the glow rises from the bottom of the window. The facets drift without one direction (@motion radial).
//
// @motion radial
// @coverage full
// @float opacity 0.8 0.0 1.0 "Opacity"
// @color light #7db8ff "Facet light"
// @color deep #1f55d6 "Deep glow"
// @float strength 0.26 0.0 0.6 "Strength"
// @float speed 0.25 0.02 1.0 "Speed"
// @float size 120 50 320 "Facet size (px)"
// @float shimmer 0.5 0.0 1.0 "Shimmer"
// @float glow 0.5 0.0 1.0 "Bottom glow"
// @preset ps4-blue light=#7db8ff deep=#1f55d6 strength=0.26 glow=0.5
// @preset twilight light=#b79cff deep=#3a2bb8 strength=0.26 glow=0.5
// @preset ice light=#bdeeff deep=#2a8fd6 strength=0.24 glow=0.4

float hash21(vec2 p) {
    p = fract(p * vec2(0.3183099, 0.3678794));
    p += dot(p, p.yx + vec2(19.19, 7.77));
    return fract((p.x + p.y) * (p.x * 37.13 + p.y * 17.71 + 1.0));
}

// one layer of triangles: brightness of the facet under p and a thin bright edge, both 0..1
vec2 facets(vec2 fc, float size, float t, float seed) {
    vec2 p = fc / size + vec2(0.9, 0.5) * t * (0.35 + 0.2 * seed);
    // a slow wobble of the lattice makes the triangles irregular, like loose low-poly vertices
    p += 0.05 * vec2(sin(p.y * 0.9 + t * 1.3 + seed * 3.0), sin(p.x * 0.8 + t * 1.1 + seed * 5.0));
    vec2 cell = floor(p);
    vec2 f = fract(p);
    float flip = mod(cell.x + cell.y, 2.0);
    // each square is split along one of its diagonals; the diagonal alternates like a checkerboard
    float d = mix(f.x - f.y, f.x + f.y - 1.0, flip);
    float tri = step(0.0, d);
    float h = hash21(cell * 2.0 + tri + seed * 31.0);
    float life = 0.5 + 0.5 * sin(t * (0.9 + h) * 2.0 + h * 40.0);
    float fill = smoothstep(0.25, 0.95, h) * (0.35 + 0.65 * life);
    float edgeDist = min(min(f.x, 1.0 - f.x), min(min(f.y, 1.0 - f.y), abs(d) * 0.7071));
    float edge = (1.0 - smoothstep(0.0, 0.035, edgeDist)) * smoothstep(0.2, 0.9, h);
    return vec2(fill, edge);
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
    vec2 near = facets(fc, P_size, t, 0.0);
    vec2 far = facets(fc + vec2(137.0, 59.0), P_size * 1.9, t * 0.6, 1.0);
    float shimmer = 1.0 + P_shimmer * 0.5 * sin(fc.x * 0.012 + fc.y * 0.008 + iTime * 0.9 * P_speed * 4.0);
    float up = fc.y / iResolution.y;                  // 0 at the bottom of the window, 1 at the top
    vec3 col = P_light * (near.x * 0.55 + near.y * 0.35 + (far.x * 0.30 + far.y * 0.18)) * shimmer;
    col *= 0.45 + 0.55 * (1.0 - 0.6 * up);            // facets are a little stronger toward the bottom
    col += P_deep * P_glow * 0.45 * exp(-up * 2.6);   // the soft blue glow rising from the bottom edge
    fragColor = vec4(term.rgb + col * P_strength * bgMask, term.a);
}
