// PS3 XMB-style flowing clock (original code): the time of day told only with light, no digits and no glyphs.
// A clock face drawn in soft glows over slow XMB wave bands: twelve faint marker dots (12, 3, 6 and 9 a little larger), a large
// orb on the outer ring for the hour, a smaller orb on the middle ring for the minute and a spark gliding round the inner ring
// for the second. Each carries a fading tail behind it (a tail never resets, so the top of the hour is not a pop). The colour
// of the waves follows the time of day: deep blue at night, cyan by day, amber at dusk, so morning and evening read at a glance.
// Sits behind the text: only plain-background pixels are touched (gp_textMask).
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
    vec3 night = vec3(0.10, 0.20, 0.62);
    vec3 dawn = vec3(0.36, 0.34, 0.85);
    vec3 day = vec3(0.20, 0.70, 0.95);
    vec3 dusk = vec3(1.00, 0.55, 0.22);
    // smooth, periodic weights (each a soft bump around its hour), normalised so the colour never steps
    float wn = exp(-pow(min(abs(h - 0.0), min(abs(h - 24.0), abs(h - 2.0))) / 3.2, 2.0));
    float wa = exp(-pow((h - 6.5) / 1.8, 2.0));
    float wd = exp(-pow((h - 13.0) / 3.6, 2.0));
    float wu = exp(-pow((h - 19.0) / 1.9, 2.0));
    return (night * wn + dawn * wa + day * wd + dusk * wu) / (wn + wa + wd + wu + 1e-4);
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

// the angle of v measured clockwise from twelve o'clock, 0..1 of a turn
float turnOf(vec2 v) {
    return fract(atan(v.x, v.y) / TAU + 1.0);
}

// a glowing orb at turn `a` on the ring of radius `r`, with a tail of `len` turns fading behind it
float orbWithTail(vec2 v, float r, float a, float rad, float len, float width) {
    vec2 c = r * vec2(sin(a * TAU), cos(a * TAU));
    float d = length(v - c);
    float orb = exp(-pow(d / rad, 2.0)) + 0.35 * exp(-d / (rad * 2.5));
    // the tail: on the ring, between the orb and `len` behind it, brightest next to the orb
    float behind = fract(a - turnOf(v));
    float onRing = exp(-pow((length(v) - r) / width, 2.0));
    float tail = onRing * (1.0 - smoothstep(0.0, len, behind)) * smoothstep(0.0, 0.004, behind);
    return orb + 0.55 * tail;
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
    vec3 glow = mix(tint, vec3(0.85, 0.95, 1.0), 0.45);

    // the background: slow ribbons in the colour of the hour, strongest across the middle of the window
    float t = iTime * 0.25 * P_flow;
    vec3 col = tint * ribbons(p, t) * 0.17 * (0.35 + 0.65 * exp(-p.y * p.y * 6.0));

    // the face: twelve marker dots, the quarters larger
    float a = turnOf(v);
    float k = floor(a * 12.0 + 0.5);
    float ma = k / 12.0;
    vec2 mp = R * vec2(sin(ma * TAU), cos(ma * TAU));
    float big = mod(k, 3.0) < 0.5 ? 1.0 : 0.0;
    float mr = R * (0.022 + 0.014 * big);
    float breathe = known ? 1.0 : 0.75 + 0.25 * sin(iTime * 0.6);
    col += glow * exp(-pow(length(v - mp) / mr, 2.0)) * (0.55 + 0.25 * big) * breathe;
    // a faint ring joining them
    col += tint * 0.10 * exp(-pow((length(v) - R) / (R * 0.012), 2.0));

    if (known) {
        float h12 = mod(s / 3600.0, 12.0) / 12.0;
        float m = mod(s / 60.0, 60.0) / 60.0;
        float sec = mod(s, 60.0) / 60.0;
        // a ripple of light running along each tail toward its orb: the flow
        float ripple = 0.75 + 0.25 * sin(length(v) * 40.0 - iTime * 1.5 * P_flow);
        // the hour is the big bright orb near the rim, the minute a smaller one further in, the second a small spark
        col += glow * 1.9 * orbWithTail(v, R * 0.84, h12, R * 0.13, 0.12, R * 0.045) * ripple;
        col += mix(glow, vec3(1.0), 0.3) * 1.4 * orbWithTail(v, R * 0.58, m, R * 0.075, 0.18, R * 0.026) * ripple;
        col += vec3(0.85, 0.95, 1.0) * 0.6 * orbWithTail(v, R * 0.34, sec, R * 0.035, 0.10, R * 0.012);
    }
    // a soft light at the centre of the face
    col += glow * 0.18 * exp(-length(v) / (R * 0.12));

    fragColor = vec4(term.rgb + col * P_strength * free, term.a);
}
