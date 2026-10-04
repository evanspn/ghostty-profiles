// Pixel rain: chunky, blocky rain streaks falling at a slight angle, with little splash pixels along
// the bottom and an optional thunder flash (off by default). Original code.
// Only dark background pixels are touched (text stays exactly as drawn) and nothing samples
// neighbouring pixels. Ghostty custom shader (Shadertoy-style). Tunable per profile.
//
// @color rain #4aa3ff "Rain color"
// @color splash #bfe3ff "Splash color"
// @float strength 0.70 0.0 1.0 "Strength"
// @float speed 0.90 0.1 3.0 "Speed"
// @float pixel 6 3 16 "Pixel size (px)"
// @float slant 0.25 -0.6 0.6 "Slant"
// @float density 0.35 0.05 0.9 "Density"
// @float thunder 0.0 0.0 1.0 "Thunder flash (0 = off)"
// @preset drizzle rain=#4aa3ff splash=#bfe3ff strength=0.60 speed=0.60 density=0.20 thunder=0
// @preset rain rain=#4aa3ff splash=#bfe3ff strength=0.70 speed=0.90 density=0.35 thunder=0
// @preset storm rain=#3d7dff splash=#d6ecff strength=0.80 speed=1.60 density=0.60 thunder=0.5
// @preset night rain=#6b6bff splash=#c8c8ff strength=0.60 speed=0.90 density=0.35 thunder=0

float hash21(vec2 p) {
    p = fract(p * vec2(0.3183099, 0.3678794));
    p += dot(p, p.yx + vec2(19.19, 7.77));
    return fract((p.x + p.y) * (p.x * 37.13 + p.y * 17.71 + 1.0));
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);
    float lum = dot(term.rgb, vec3(0.299, 0.587, 0.114));
    float bgMask = 1.0 - smoothstep(0.30, 0.60, lum);
    if (P_strength <= 0.0001 || bgMask <= 0.001) {
        fragColor = term;
        return;
    }

    // everything below is computed per chunky pixel, so the rain looks blocky
    vec2 px = floor(fragCoord / P_pixel);
    vec2 grid = floor(iResolution.xy / P_pixel);
    float xs = floor(px.x + px.y * P_slant);
    float h = hash21(vec2(xs, 1.0));
    float on = step(1.0 - P_density, hash21(vec2(xs, 5.0)));
    float cellsPerSec = (6.0 + 10.0 * h) * P_speed;
    float period = grid.y + 20.0 + h * 30.0;
    float head = mod(iTime * cellsPerSec + h * period, period);
    float y = grid.y - px.y;             // rows from the top
    float d = head - y;                  // how far behind the head this pixel is
    float len = 5.0 + floor(h * 9.0);
    float streak = on * step(0.0, d) * step(d, len) * (1.0 - d / len);
    streak = floor(streak * 3.0 + 0.5) / 3.0;   // three chunky brightness steps

    // splashes: a few bright pixels on the bottom two rows right after a drop lands
    float landed = head - grid.y;
    float inWindow = on * step(0.0, landed) * step(landed, 3.0);
    float splash = inWindow * step(px.y, 1.5) * step(0.55, hash21(px + floor(iTime * 10.0)));

    // thunder: rare brief flashes of the whole sky (only when thunder > 0)
    float flash = 0.0;
    if (P_thunder > 0.0) {
        float s = sin(iTime * 0.9 + 1.0) * sin(iTime * 0.37 + 0.5);
        flash = P_thunder * pow(max(s, 0.0), 24.0);
    }

    vec3 add = P_rain * streak + P_splash * splash + P_splash * flash * 0.25;
    fragColor = vec4(term.rgb + add * P_strength * bgMask, term.a);
}
