// PS3 XMB-style flowing clock (original code), in the mood of moonlight sorcery: the time of day told only with soft light,
// no digits and no glyphs, everything a little out of focus. Three translucent pale-blue moons float on invisible orbits
// around the middle of the window: the big one is the hour, the middle one the minute, the small one the second (it steps
// once a second; `seconds` 0 hides it). Each has a bright soft rim, a dim slowly marbled heart, and bends the waves behind it
// a little. Around them a slow spiral wind of tiny four-point sparkles and a few curved light streaks drifts and twinkles
// (slowly, never flickering), with a few soft bokeh glows. Cool blue-white only; the slow XMB waves shift within it through
// the day: deep navy at night, brighter teal-blue by day, violet at dusk. Sits behind the text (gp_textMask).
//
// WHERE THE TIME COMES FROM: Ghostty gives shaders no wall-clock time (`iDate` is always zero and `iTime` counts from each
// window's own first frame), so ghostty-profiles runs a palette clock while this look is active: each zsh shell stamps
// the time into color 254 of its own terminal once a second, and the generated header reads it (gp_clockStamp, see the
// project README). Without a running stamp, `clock` >= 0 starts the clock at that hour (previews), else `iDate` is used
// (Shadertoy, shaderlab); with none of these the face shows no hands at all, only the markers breathing: it never
// invents a time.
// Ghostty origin top-left: everything is computed y-up through gp_yup().
//
// @motion none
// @float opacity 1.0 0.0 1.0 "Opacity"
// @float strength 0.6 0.0 1.0 "Strength"
// @float clock -1 -1 24 "Start hour, or -1 for live"
// @float size 0.34 0.15 0.48 "Clock size"
// @float cx 0.5 0.1 0.9 "Clock position across"
// @float cy 0.5 0.1 0.9 "Clock position down"
// @float flow 1.0 0.0 2.0 "Wave flow speed"
// @float seconds 1 0 1 "Second sphere"
// @float ambient 1.0 0.0 2.0 "Bokeh glows"
// @float sparkle 1.0 0.0 2.0 "Sparkle"
// @preset demo clock=10.15
// @preset corner clock=-1 size=0.2 cx=0.84 cy=0.24

const float TAU = 6.2831853;

// seconds since local midnight, or -1 when nothing tells the shader the time
float clockSeconds() {
#ifdef GP_HAS_CLOCK
    vec4 st = gp_clockStamp();
    if (st.w > 0.5) return st.x * 3600.0 + st.y * 60.0 + st.z;
#endif
    if (P_clock >= 0.0) return mod(P_clock * 3600.0 + iTime, 86400.0);
    if (iDate.w > 0.0) return iDate.w;
    return -1.0;
}

// the XMB background colour for an hour of the day (0..24): night blue, dawn violet-blue, day cyan, dusk amber, back to night
vec3 dayTint(float h) {
    vec3 night = vec3(0.10, 0.18, 0.52);
    vec3 dawn = vec3(0.30, 0.36, 0.80);
    vec3 day = vec3(0.30, 0.66, 0.90);
    vec3 dusk = vec3(0.46, 0.38, 0.92);
    // smooth, periodic weights (each a soft bump around its hour), normalised so the colour never steps
    float wn = exp(-pow(min(abs(h - 0.0), min(abs(h - 24.0), abs(h - 2.0))) / 3.2, 2.0));
    float wa = exp(-pow((h - 6.5) / 1.8, 2.0));
    float wd = exp(-pow((h - 13.0) / 3.6, 2.0));
    float wu = exp(-pow((h - 19.0) / 1.9, 2.0));
    return (night * wn + dawn * wa + day * wd + dusk * wu) / (wn + wa + wd + wu + 1e-4);
}

float hash21(vec2 p) {
    p = fract(p * vec2(0.3183099, 0.3678794));
    p += dot(p, p.yx + vec2(19.19, 7.77));
    return fract((p.x + p.y) * (p.x * 37.13 + p.y * 17.71 + 1.0));
}

float vnoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + vec2(1.0, 0.0)), f.x), mix(hash21(i + vec2(0.0, 1.0)), hash21(i + vec2(1.0, 1.0)), f.x), f.y);
}

// soft XMB ribbons across the window
float ribbons(vec2 q, float t) {
    float w = 0.0;
    for (int i = 0; i < 4; i++) {
        float f = float(i);
        float y = -0.05 + 0.09 * sin(q.x * (1.7 + 0.3 * f) + t * (0.45 + 0.1 * f) + f * 1.9)
                + 0.04 * sin(q.x * (3.3 + 0.4 * f) - t * 0.3 + f * 4.1);
        float d = abs(q.y - y);
        w += (exp(-d * 30.0) * 0.5 + exp(-d * 6.0) * 0.5) * (0.6 + 0.15 * f);
    }
    return w;
}

// the direction of turn `a` (0..1, clockwise from twelve o'clock)
vec2 dirOf(float a) {
    return vec2(sin(a * TAU), cos(a * TAU));
}

const vec3 MOON = vec3(0.62, 0.78, 1.0);
const vec3 SILVER = vec3(0.86, 0.91, 1.0);
const vec3 VIOLET = vec3(0.70, 0.64, 1.0);

// a translucent moon of radius `rad` at `c`, out of focus: a bright soft rim, a dim heart with a slowly turning marble, a
// wide haze, and a gentle lens that bends whatever is behind it (added to `disp`). `seed` gives each its own marble.
vec3 moon(vec2 v, vec2 c, float rad, float seed, float t, inout vec2 disp) {
    vec2 q = v - c;
    // far away there is nothing left to draw (the haze is below 0.3% four radii out): skip the work
    if (dot(q, q) > 16.0 * rad * rad) return vec3(0.0);
    float d = length(q) / rad;
    disp -= q * 0.25 * exp(-d * d * 1.2);
    float rim = 0.50 * exp(-pow((d - 0.86) / 0.32, 2.0));
    vec2 mq = q / rad * 1.6 + vec2(seed, seed * 0.7);
    float marble = vnoise(mq + vec2(t * 0.04, -t * 0.03)) * 0.6 + vnoise(mq * 2.3 - t * 0.05) * 0.4;
    float heart = (0.10 + 0.10 * marble) * (1.0 - smoothstep(0.55, 1.0, d));
    float haze = 0.24 * exp(-d * 0.95);
    vec3 tone = mix(MOON, VIOLET, 0.5 + 0.5 * sin(t * 0.05 + seed));
    return tone * (rim + haze) + SILVER * heart;
}

// a tiny out-of-focus four-point star of size `sz` at `c`: a soft point with two thin soft arms
float star(vec2 v, vec2 c, float sz) {
    vec2 q = v - c;
    if (dot(q, q) > 36.0 * sz * sz) return 0.0;
    vec2 a = abs(q) / sz;
    float point = exp(-dot(a, a) * 1.4);
    // (the arms are soft and at least a couple of pixels thick, so a moving sparkle never shimmers)
    float arms = exp(-a.x * 1.1 - a.y * a.y * 2.2) + exp(-a.y * 1.1 - a.x * a.x * 2.2);
    return point + 0.35 * arms;
}

// where a hand's moon floats: on an invisible orbit of radius `r` at turn `a`, drifting gently around its place (never along
// the orbit, so the angle it shows stays true)
vec2 handPos(float r, float a, float seed, float t) {
    vec2 d = dirOf(a);
    vec2 side = vec2(d.y, -d.x);
    float bob = 0.035 * r * sin(t * 0.6 + seed) + 0.02 * r * sin(t * 0.37 + seed * 1.7);
    return d * (r + bob) + side * 0.012 * r * sin(t * 0.45 + seed * 0.7);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);
    float free = 1.0 - gp_textMask(fragCoord, term);
    if (free < 0.001 || P_strength <= 0.0) {
        fragColor = term;
        return;
    }
    vec2 fc = gp_yup(fragCoord);
    // one uniform scale (by the height): the face stays round at every aspect
    vec2 p = (fc - 0.5 * iResolution.xy) / iResolution.y;
    vec2 centre = (vec2(P_cx, 1.0 - P_cy) - 0.5) * iResolution.xy / iResolution.y;
    vec2 v = p - centre;
    float R = P_size;

    float s = clockSeconds();
    bool known = s >= 0.0;
    float hours = known ? s / 3600.0 : 21.0;
    vec3 tint = dayTint(hours);
    float px = 1.0 / iResolution.y;

    vec3 col = vec3(0.0);
    vec2 disp = vec2(0.0);
    float ft = iTime * P_flow;

    // the face's bearings: four faint sparkles at 12, 3, 6 and 9, twinkling slowly (no circle is drawn)
    float breathe = known ? 1.0 : 0.75 + 0.25 * sin(iTime * 0.6);
    for (int k = 0; k < 4; k++) {
        float fk = float(k);
        float tw = 0.6 + 0.4 * sin(ft * 0.5 + fk * 1.7);
        col += SILVER * 0.45 * tw * breathe * star(v, R * 1.02 * dirOf(fk * 0.25), max(R * 0.018, 2.2 * px));
    }

    // the spiral wind: sparkles circling the face, faster and denser toward the middle, each on a smooth path (angle
    // grows with time, nothing wraps) and twinkling slowly; at least 2.2 px wide so they glide instead of shimmering.
    // A sparkle only reaches pixels near its own circle, so the rest are skipped before any trigonometry.
    float vr0 = length(v * vec2(1.0, 1.0 / 0.9));
    for (int k = 0; k < 40; k++) {
        float fk = float(k);
        float h1 = fract(fk * 0.6180340 + 0.13), h2 = fract(fk * 0.7548777 + 0.41);
        float rr = R * (0.18 + 1.35 * h1 * h1);
        float sz = max(R * (0.010 + 0.014 * h2), 2.2 * px);
        if (abs(vr0 - rr) > 7.0 * sz) continue;
        float ang = h2 * TAU + ft * 0.09 * (R / rr) + log(rr / R + 0.2) * 1.4;
        vec2 c = rr * vec2(cos(ang), sin(ang)) * vec2(1.0, 0.9);
        if (dot(v - c, v - c) > 36.0 * sz * sz) continue;
        float tw = pow(0.5 + 0.5 * sin(ft * (0.35 + 0.4 * h2) + fk * 2.4), 2.0);
        col += mix(SILVER, MOON, h1) * (0.55 * P_sparkle) * (0.25 + 0.75 * tw) * star(v, c, sz);
    }

    // curved light streaks flowing outward along the spiral arms (soft, short, like wind made of light)
    float vr = length(v), va = atan(v.y, v.x);
    for (int k = 0; k < 3; k++) {
        float fk = float(k);
        // the arm through this radius, and where along it the streak's head is now
        float armAng = log(vr / R + 0.2) * 1.4 + fk * TAU / 3.0 + ft * 0.05;
        float da = abs(mod(va - armAng + TAU * 0.5, TAU) - TAU * 0.5) * vr;
        float head = R * (0.35 + 1.0 * fract(ft * 0.02 + fk * 0.37));
        float along = exp(-pow((vr - head) / (R * 0.2), 2.0));
        col += MOON * (0.16 * P_sparkle) * exp(-pow(da / (R * 0.035), 2.0)) * along;
    }

    // a few soft bokeh glows drifting on slow smooth paths, fading in and out
    for (int k = 0; k < 6; k++) {
        float fk = float(k);
        vec2 c = vec2(0.85 * sin(ft * (0.031 + 0.007 * fk) + fk * 2.1) + 0.25 * sin(ft * 0.053 + fk * 4.3),
                      0.38 * sin(ft * (0.027 + 0.005 * fk) + fk * 1.3) + 0.08 * sin(ft * 0.071 + fk));
        float rad = R * (0.08 + 0.06 * fract(fk * 0.618));
        float fade = 0.5 + 0.5 * sin(ft * (0.11 + 0.02 * fk) + fk * 2.7);
        float d = length(p - c) / rad;
        col += mix(MOON, VIOLET, fract(fk * 0.37)) * (0.10 * P_ambient * fade) * (1.0 - smoothstep(0.7, 1.15, d)) * (0.6 + 0.4 * smoothstep(0.3, 1.0, d));
    }

    if (known) {
        float h12 = mod(s / 3600.0, 12.0) / 12.0;
        float m = mod(s / 60.0, 60.0) / 60.0;
        float sec = mod(s, 60.0) / 60.0;
        // the hour is the big moon near the rim, the minute a smaller one further in, the second a small one inside; they
        // breathe slowly, never below about 70%, so the time stays readable
        float bh = 0.85 + 0.15 * sin(ft * 0.23), bm = 0.85 + 0.15 * sin(ft * 0.29 + 2.0), bs = 0.85 + 0.15 * sin(ft * 0.31 + 4.0);
        vec2 ch = handPos(R * 0.80, h12, 1.0, ft), cm = handPos(R * 0.52, m, 4.0, ft), cs = handPos(R * 0.27, sec, 7.0, ft);
        col += 1.6 * bh * moon(v, ch, R * 0.17, 1.0, ft, disp);
        col += 1.4 * bm * moon(v, cm, R * 0.11, 4.0, ft, disp);
        col += 1.2 * bs * P_seconds * moon(v, cs, R * 0.055, 7.0, ft, disp);
        // each moon has its own little swirl of sparkles, densest close to it (smooth circling, slow twinkle); a group is
        // skipped entirely for pixels nowhere near its moon
        for (int g = 0; g < 3; g++) {
            vec2 cc = g == 0 ? ch : (g == 1 ? cm : cs);
            float rad = g == 0 ? R * 0.17 : (g == 1 ? R * 0.11 : R * 0.055);
            float n = g == 0 ? 11.0 : (g == 1 ? 8.0 : 5.0 * P_seconds);
            vec2 w = v - cc;
            if (dot(w, w) > pow(rad * 2.45 + R * 0.12, 2.0)) continue;
            for (int k = 0; k < 11; k++) {
                float fk = float(k);
                if (fk >= n) break;
                float h1 = fract(fk * 0.6180340 + float(g) * 0.29), h2 = fract(fk * 0.7548777 + float(g) * 0.53);
                float rr = rad * (1.05 + 1.3 * h1 * h1);
                float sz = max(R * (0.008 + 0.010 * h2), 2.2 * px);
                if (abs(length(w * vec2(1.0, 1.0 / 0.85)) - rr) > 7.0 * sz) continue;
                float ang = h2 * TAU + ft * 0.25 * (rad / rr) * (g < 2 ? 1.0 : 1.5);
                vec2 c = rr * vec2(cos(ang), sin(ang) * 0.85);
                float tw = pow(0.5 + 0.5 * sin(ft * (0.4 + 0.5 * h2) + fk * 1.9 + float(g) * 3.1), 2.0);
                col += SILVER * (0.65 * P_sparkle) * (0.3 + 0.7 * tw) * star(w, c, sz);
            }
        }
    }
    // the background: slow ribbons in the colour of the hour, strongest across the middle of the window, seen through
    // the spheres' lenses
    float t = iTime * 0.25 * P_flow;
    vec2 pb = p + disp;
    col += tint * ribbons(pb, t) * 0.17 * (0.35 + 0.65 * exp(-pb.y * pb.y * 6.0));
    // a soft light at the centre of the face
    col += MOON * 0.10 * exp(-length(v) / (R * 0.15));

    fragColor = vec4(term.rgb + col * P_strength * free, term.a);
}
