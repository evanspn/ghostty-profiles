// PS3 music visualizer, rebuilt from screen recordings of the real thing (original code, procedural, no textures).
// A playlist of calm landscapes you glide through, crossfading forever: rolling green hills with radial blur, a dark valley whose
// far crests glow cyan, a low flight over still water, dark slate with long glossy streaks, a saturated colour wash, and a soft
// tunnel of light. Each scene lasts `scene_period` seconds with a `fade` second crossfade; `scene` locks one for a preview;
// `playlist` is the order as digits (123456 = all six; 36 = water and tunnel only).
// Sits behind the text: only plain-background pixels are drawn (`maxlum` keeps bright scenes dim enough to read over).
// Ghostty origin top-left: the sky is at the top (small y); everything is computed y-up through gp_yup().
// The beat is simulated; the `// AUDIO:` line marks where a smoothed 0..1 audio level would go.
//
// @motion none
// @coverage full
// @float opacity 1.0 0.0 1.0 "Opacity"
// @float strength 1.0 0.0 1.0 "Strength"
// @float scene 0 0 6 "Scene (0 playlist, 1-6 lock)"
// @float playlist 123456 1 666666 "Playlist order (digits)"
// @float scene_period 32 3 120 "Seconds per scene"
// @float fade 4 1 10 "Crossfade seconds"
// @float speed 0.5 0.0 2.0 "Flight speed"
// @float pulse 0.35 0.0 1.0 "Beat strength"
// @float tempo 72 30 180 "Tempo (BPM)"
// @float glow 1.0 0.0 2.0 "Glow"
// @float maxlum 0.6 0.1 1.0 "Max brightness (legibility)"
// @color sky #08387c "Valley sky"
// @color rim #46d8ff "Valley rim light"
// @color ground #04101c "Valley ground"
// @preset ps3-classic scene=0
// @preset hills scene=1
// @preset valley scene=2
// @preset water scene=3
// @preset silk scene=4
// @preset wash scene=5
// @preset tunnel scene=6
// @preset midnight scene=2 sky=#04184a rim=#6a8cff ground=#02060f

float hash21(vec2 p) {
    p = fract(p * vec2(0.3183099, 0.3678794));
    p += dot(p, p.yx + vec2(19.19, 7.77));
    return fract((p.x + p.y) * (p.x * 37.13 + p.y * 17.71 + 1.0));
}

float vnoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + vec2(1.0, 0.0)), f.x),
               mix(hash21(i + vec2(0.0, 1.0)), hash21(i + vec2(1.0, 1.0)), f.x), f.y);
}

float fbm2(vec2 p) { return vnoise(p) * 0.65 + vnoise(p * 2.07 + 11.3) * 0.35; }
float lumOf(vec3 c) { return dot(c, vec3(0.299, 0.587, 0.114)); }

// ---- 1: rolling green hills with radial motion blur -----------------------------------------------------
float hillsH(vec2 q) {
    float h = 5.0 * vnoise(q * vec2(0.05, 0.035)) + 1.6 * vnoise(q * 0.14 + 7.0);
    // a round mound every so often
    vec2 cell = vec2(floor(q.x / 46.0), floor(q.y / 70.0));
    vec2 ctr = (cell + vec2(0.3 + 0.4 * hash21(cell), 0.3 + 0.4 * hash21(cell + 4.0))) * vec2(46.0, 70.0);
    float d = length((q - ctr) * vec2(1.0, 0.7));
    h += 6.0 * hash21(cell + 9.0) * exp(-d * d / 110.0);
    return h;
}

vec3 hills(vec2 p, float z, float t, float audio) {
    vec2 g = vec2(1.2 * sin(t * 0.11), z);
    // glide a couple of units above the highest ground just ahead, so the camera never dips into a hill
    float ground = max(hillsH(g), max(hillsH(g + vec2(0.0, 5.0)), hillsH(g + vec2(0.0, 10.0))));
    vec3 ro = vec3(g.x, ground + 2.3 + 0.2 * sin(t * 0.3), z);
    float horizon = 0.0;
    vec3 rd = normalize(vec3(p.x, p.y - horizon, 1.0));
    float tt = 0.5;
    bool hit = false;
    for (int i = 0; i < 40; i++) {
        vec3 pos = ro + rd * tt;
        float d = pos.y - hillsH(pos.xz);
        if (d < 0.004 * tt) { hit = true; break; }
        tt += clamp(d * 0.75 + 0.03 * tt, 0.05, 9.0);
        if (tt > 200.0 || (rd.y > 0.0 && pos.y > 13.0)) break;
    }
    vec3 skyLow = vec3(0.30, 0.64, 0.60);
    vec3 skyHigh = vec3(0.16, 0.50, 0.55);
    vec3 col = mix(skyLow, skyHigh, clamp((p.y - horizon) * 2.5, 0.0, 1.0));
    float ang = atan(p.y - horizon, p.x);
    float r = length(vec2(p.x, p.y - horizon));
    // faint radial rays in the sky
    col *= 0.985 + 0.03 * vnoise(vec2(ang * 18.0, t * 0.05));
    if (!hit && rd.y < 0.0) { hit = true; tt = 200.0; }
    if (hit) {
        vec3 pos = ro + rd * min(tt, 200.0);
        float e = 0.1 + tt * 0.02;
        vec3 n = normalize(vec3(hillsH(pos.xz - vec2(e, 0.0)) - hillsH(pos.xz + vec2(e, 0.0)), 2.0 * e,
                                hillsH(pos.xz - vec2(0.0, e)) - hillsH(pos.xz + vec2(0.0, e))));
        float diff = clamp(0.35 + 0.8 * dot(n, normalize(vec3(-0.4, 0.8, -0.3))), 0.0, 1.0);
        vec3 dark = vec3(0.05, 0.16, 0.05);
        vec3 light = vec3(0.38, 0.56, 0.17);
        float h = hillsH(pos.xz);
        vec3 grass = mix(dark, light, clamp(diff * 0.85 + 0.25 * h / 3.0, 0.0, 1.0));
        // radial streaks: the ground smears along the lines running out from the vanishing point
        float streak = vnoise(vec2(ang * 20.0, log(r + 0.05) * 0.9 + z * 0.02));
        grass *= 0.86 + 0.20 * streak + 0.18 * (fbm2(pos.xz * 0.30) - 0.5);
        // the dark wedge of shadow running down from the horizon
        grass *= mix(1.0, 0.42, exp(-p.x * p.x * 7.0) * smoothstep(0.0, 0.12, horizon - p.y));
        float fog = 1.0 - exp(-tt * 0.016);
        col = mix(grass, skyLow * 0.98, fog * 0.92);
    }
    col *= 1.0 + 0.12 * audio;
    return col;
}

// ---- 2: the dark valley with cyan-lit crests --------------------------------------------------------------
float valleyC(float z) { return 0.5 * sin(z * 0.02) + 0.25 * sin(z * 0.051 + 1.3); }

float valleyH(vec2 q, float rough) {
    float x = q.x - valleyC(q.y);
    // walls rise from the floor over a few units to a plateau, higher on the left than the right, so the skyline is a V that
    // runs into the vanishing point instead of a bowl
    float wallL = 5.2 + 2.0 * vnoise(vec2(q.y * 0.05, 3.0));
    float wallR = 3.6 + 2.0 * vnoise(vec2(q.y * 0.045, 9.0));
    float wall = x < 0.0 ? wallL : wallR;
    float w = smoothstep(0.3, 8.5, abs(x));
    float h = wall * (w * w * (1.5 - 0.5 * w));
    h += 1.2 * vnoise(q * vec2(0.11, 0.08)) * smoothstep(1.0, 8.0, abs(x));
    // a rough, crumbly crest along the top of the walls
    h += rough * 1.7 * (vnoise(q * vec2(0.55, 0.45)) - 0.5) * smoothstep(3.0, 8.0, abs(x));
    return h;
}

vec3 valley(vec2 p, float z, float t, float audio) {
    vec3 ro = vec3(valleyC(z) + 0.9 * sin(t * 0.23) + 0.5 * sin(t * 0.51), 1.2 + 0.1 * sin(t * 0.37), z);
    vec3 rd = normalize(vec3(p.x + 0.04 * sin(t * 0.31), p.y + 0.03, 1.0));
    float tt = 0.3;
    float minClear = 1e3;
    bool hit = false;
    for (int i = 0; i < 30; i++) {
        vec3 pos = ro + rd * tt;
        float d = pos.y - valleyH(pos.xz, 0.6);
        // only the walls make a glowing silhouette, not a ray skimming the far floor
        if (pos.y > 1.8) minClear = min(minClear, d / tt);
        if (d < 0.003 * tt) { hit = true; break; }
        tt += clamp(d * 0.6 + 0.03 * tt, 0.02, 8.0);
        if (tt > 160.0 || (rd.y > 0.0 && pos.y > 10.0)) break;
    }
    float skyT = clamp(rd.y * 3.0 + 0.2, 0.0, 1.0);
    vec3 col = mix(P_sky * 1.5, P_sky * 0.75, pow(skyT, 0.6));
    float haze = 1.0 - exp(-tt * 0.022);
    if (hit) {
        vec3 pos = ro + rd * tt;
        float e = 0.04 + tt * 0.01;
        vec3 n = normalize(vec3(valleyH(pos.xz - vec2(e, 0.0), 0.6) - valleyH(pos.xz + vec2(e, 0.0), 0.6), 2.0 * e,
                                valleyH(pos.xz - vec2(0.0, e), 0.6) - valleyH(pos.xz + vec2(0.0, e), 0.6)));
        float diff = 0.5 + 0.5 * n.y;
        float fres = pow(1.0 - clamp(dot(n, -rd), 0.0, 1.0), 3.0);
        vec3 lit = P_ground * (0.35 + 0.9 * diff);
        float high = smoothstep(2.2, 5.0, pos.y);
        float farGlow = high * smoothstep(8.0, 36.0, tt);
        float wallH = smoothstep(1.2, 3.8, pos.y);
        lit += P_rim * P_glow * (0.9 + 0.7 * audio) * wallH * (fres * smoothstep(8.0, 40.0, tt) * 1.7 + farGlow * 0.7);
        lit += P_rim * 0.20 * smoothstep(0.6, 3.4, pos.y) * (0.3 + 0.7 * haze);
        col = mix(lit, col, pow(haze, 2.2) * 0.35);
    }
    // a ray that skims the far floor without landing is the dark road running to the vanishing point, not sky
    if (!hit && rd.y < 0.0) {
        col = P_ground * 1.4;
        hit = true;
    }
    float halo = exp(-max(minClear, 0.0) * 45.0) * (hit ? 0.0 : 1.0);
    col += P_rim * P_glow * halo * 0.75 * (1.0 + 0.6 * audio);
    return col;
}

// ---- 3: calm water seen from just above the surface -------------------------------------------------------
float waveNoise(vec2 q, float t) { return vnoise(q * 0.7 + vec2(t * 0.08, 0.0)); }

vec3 water(vec2 p, float z, float t, float audio) {
    float camY = 1.1;
    vec3 rd = normalize(vec3(p.x + 0.02 * sin(t * 0.2), p.y + 0.13, 1.0));
    vec3 skyHor = vec3(0.70, 0.74, 0.76);
    vec3 skyTop = vec3(0.58, 0.65, 0.70);
    vec3 sun = normalize(vec3(0.15, 0.32, 1.0));
    float up = max(rd.y, 0.0);
    vec3 sky = mix(skyHor, skyTop, pow(clamp(up * 2.2, 0.0, 1.0), 0.7));
    if (rd.y >= -0.002) return sky * min(1.0, 0.7 * P_maxlum / max(lumOf(sky), 1e-3));
    float tt = camY / -rd.y;
    vec2 q = vec2(rd.x * tt, rd.z * tt + z);
    // ripples radiating from a point ahead, travelling toward the camera
    vec2 c0 = vec2(0.0, z + 11.0);
    float rr = length(q - c0);
    float ring = sin(rr * 2.4 - t * 1.1) * exp(-rr * 0.05);
    float att = 1.0 / (1.0 + tt * 0.06);
    // the slope of the water: two broad swells (analytic), one noise (finite difference) and the rings
    vec2 ph1 = vec2(0.9, 0.4) * 1.3, ph2 = vec2(-0.5, 0.85) * 2.1;
    float c1 = cos(dot(q, ph1) + t * 0.9), c2 = cos(dot(q, ph2) - t * 0.7);
    float e = 0.12;
    float n0 = waveNoise(q, t);
    vec2 slope = 0.06 * c1 * ph1 + 0.04 * c2 * ph2
               + 0.16 * vec2(waveNoise(q + vec2(e, 0.0), t) - n0, waveNoise(q + vec2(0.0, e), t) - n0) / e
               + 0.35 * ring * (q - c0) / max(rr, 0.1) * 0.12;
    vec3 n = normalize(vec3(-slope.x * att, 1.0, -slope.y * att));
    vec3 refl = reflect(rd, n);
    refl.y = abs(refl.y);
    vec3 rsky = mix(skyHor, skyTop, pow(clamp(refl.y * 2.2, 0.0, 1.0), 0.7));
    rsky += vec3(1.0, 0.98, 0.92) * pow(max(dot(refl, sun), 0.0), 300.0) * (0.9 + audio);
    float fres = 0.04 + 0.96 * pow(1.0 - clamp(dot(n, -rd), 0.0, 1.0), 5.0);
    vec3 deep = vec3(0.50, 0.52, 0.60);
    // soft caustic streaks just under the surface
    float caus = n0 * n0 * n0;
    vec3 body = deep + vec3(0.10, 0.22, 0.26) * caus * att;
    vec3 col = mix(body, rsky, clamp(fres, 0.0, 1.0));
    float fog = 1.0 - exp(-tt * 0.04);
    col = mix(col, skyHor, fog * 0.8);
    // the palest scene: keep it dimmer than the others so light terminal text still reads over it
    return col * min(1.0, 0.7 * P_maxlum / max(lumOf(col), 1e-3));
}

// ---- 4: dark slate with long glossy streaks -----------------------------------------------------------------
vec3 silk(vec2 p, float t, float audio) {
    vec2 q = p;
    q.y += 0.025 * sin(q.x * 2.4 + t * 0.18) + 0.015 * sin(q.x * 5.1 - t * 0.27);
    vec3 col = mix(vec3(0.13, 0.19, 0.25), vec3(0.03, 0.14, 0.30), clamp(q.y * 1.4 + 0.45, 0.0, 1.0));
    col += vec3(0.03, 0.05, 0.07) * fbm2(p * 2.0 + t * 0.02);
    for (int k = 0; k < 7; k++) {
        float fk = float(k);
        float y0 = -0.5 + 0.17 * fk + 0.08 * sin(t * 0.07 + fk * 2.3) + 0.07 * (hash21(vec2(fk, 3.0)) - 0.5);
        float w = 0.014 + 0.022 * hash21(vec2(fk, 9.0));
        float line = exp(-pow((q.y - y0) / w, 2.0));
        float xs = q.x * (0.9 + 0.5 * hash21(vec2(fk, 5.0))) + t * (0.015 + 0.01 * fk) * (mod(fk, 2.0) < 1.0 ? 1.0 : -1.0) + fk * 3.7;
        float seg = smoothstep(0.35, 0.8, vnoise(vec2(xs * 1.4, fk * 5.1)));
        float flare = 1.0 + 0.8 * audio * hash21(vec2(fk, floor(t * 2.0)));
        col += vec3(0.80, 0.88, 0.95) * line * seg * 0.7 * flare;
        // the soft glow around each streak
        col += vec3(0.30, 0.40, 0.50) * exp(-pow((q.y - y0) / (w * 5.0), 2.0)) * seg * 0.22;
    }
    return col;
}

// ---- 5: a saturated wash of huge soft shapes ---------------------------------------------------------------
vec3 washPalette(float s, float t) {
    vec3 violet = vec3(0.38, 0.22, 1.0);
    vec3 blue = vec3(0.02, 0.10, 1.0);
    vec3 cyan = vec3(0.10, 0.85, 0.95);
    float k = 0.5 + 0.5 * sin(t * 0.07 + s * 3.0);
    return mix(mix(violet, blue, smoothstep(0.0, 0.6, k)), cyan, smoothstep(0.7, 1.0, k + 0.2 * (s - 0.5)));
}

vec3 wash(vec2 p, float t, float audio) {
    vec2 q = p * 1.0;
    q += 0.30 * vec2(sin(q.y * 1.5 + t * 0.11), cos(q.x * 1.3 - t * 0.09));
    float s = fbm2(q * 1.0 + vec2(t * 0.02, 0.0));
    float side = dot(q, normalize(vec2(0.7, 0.55))) + 0.5 * (s - 0.5) + 0.12 * sin(t * 0.05);
    float lit = smoothstep(-0.95, 0.45, side);
    vec3 base = washPalette(s, t);
    // soft lavender sheen where the surface turns toward the light
    vec3 col = base * (0.18 + 0.95 * lit) + vec3(0.38, 0.45, 0.95) * pow(smoothstep(0.1, 0.8, lit), 3.0) * 0.35 * (s + 0.3);
    // cooler glossy streaks along the lower right
    float streak = pow(vnoise(vec2((p.x * 0.7 + p.y * 1.0) * 6.0, (p.x - p.y) * 0.8 + t * 0.03)), 2.5);
    col += vec3(0.10, 0.70, 0.85) * streak * smoothstep(0.0, 0.6, p.x - p.y * 0.4) * 0.55;
    // black swallows the top, with a crumbly fibrous edge where a shape meets it
    float edge = p.y + 0.30 * (fbm2(p * 5.0 + t * 0.03) - 0.5) + 0.05 * (vnoise(p * 30.0) - 0.5);
    col = mix(col, vec3(0.005, 0.005, 0.04), smoothstep(0.30, 0.46, edge));
    return col * (1.0 + 0.15 * audio);
}

// ---- 6: a soft tunnel of light ---------------------------------------------------------------------------------
vec3 tunnel(vec2 p, float z, float t, float audio) {
    vec2 c = p - vec2(0.06 * sin(t * 0.21), 0.04 * cos(t * 0.17));
    float r = length(c);
    float a = atan(c.y, c.x) + 0.05 * t;
    vec2 dir = vec2(cos(a), sin(a));                   // the angle as a point on a circle: no seam
    float depth = 0.35 / (r + 0.08) + z * 0.5;
    float n = fbm2(dir * 1.6 + vec2(depth * 0.9, depth * 0.5));
    float n2 = fbm2(dir * 2.4 + vec2(5.0 - depth * 0.6, depth * 0.8));
    vec3 deep = vec3(0.02, 0.03, 0.16);
    vec3 mid = vec3(0.05, 0.28, 0.62);
    vec3 hi = vec3(0.35, 0.88, 0.95);
    vec3 col = mix(deep, mid, smoothstep(0.30, 0.70, n)) + hi * pow(n2, 5.0) * 0.45;
    // light at the end of the tunnel, and soft walls fading toward the edge of the screen
    col += hi * 0.8 * exp(-r * 4.5) * (1.0 + 0.5 * audio);
    col *= smoothstep(1.15, 0.25, r) * 0.85 + 0.15;
    return col;
}

// ---- the playlist ----------------------------------------------------------------------------------------------------
int digitAt(float pl, int k, int nd) {
    float d = floor((floor(pl + 0.5) + 0.01) / pow(10.0, float(nd - 1 - k)));
    return int(d - 10.0 * floor(d / 10.0));
}

vec3 renderScene(int id, vec2 p, float ts, float t, float audio) {
    float z = (ts + float(id) * 37.0) * (P_speed * 2.2) + audio * 0.15;
    if (id == 1) return hills(p, z, t, audio);
    if (id == 2) return valley(p, z, t, audio);
    if (id == 3) return water(p, z, t, audio);
    if (id == 4) return silk(p, t, audio);
    if (id == 5) return wash(p, t, audio);
    return tunnel(p, z * 0.35, t, audio);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);
    float free = 1.0 - gp_textMask(fragCoord, term);
    if (free < 0.999 || P_strength <= 0.0) {
        fragColor = term;
        return;
    }
    vec2 fc = gp_yup(fragCoord);                      // y up: the sky is at the top of the window
    vec2 p = (fc - 0.5 * iResolution.xy) / iResolution.y;
    float phase = iTime * P_tempo / 60.0;
    float beat = pow(0.5 + 0.5 * cos(6.2831853 * phase), 8.0);
    beat *= 0.25 + 0.75 * pow(0.5 + 0.5 * sin(phase * 0.47), 4.0);
    // AUDIO: replace beat above with a smoothed audio-level uniform (0..1).
    float audio = P_pulse * beat;

    vec3 col;
    if (P_scene >= 0.5) {
        col = renderScene(int(P_scene + 0.5), p, iTime, iTime, audio);
    } else {
        int nd = max(1, min(6, int(floor(log(max(P_playlist, 1.0)) / 2.302585) + 1.0)));
        float slotF = floor(iTime / P_scene_period);
        float into = iTime - slotF * P_scene_period;
        int k = int(mod(slotF, float(nd)));
        int idA = clamp(digitAt(P_playlist, k, nd), 1, 6);
        col = renderScene(idA, p, into + slotF * 11.0, iTime, audio);
        float fadeStart = P_scene_period - P_fade;
        if (into > fadeStart) {
            int idB = clamp(digitAt(P_playlist, int(mod(slotF + 1.0, float(nd))), nd), 1, 6);
            float w = smoothstep(0.0, 1.0, (into - fadeStart) / P_fade);
            col = mix(col, renderScene(idB, p, (into - P_scene_period) + (slotF + 1.0) * 11.0, iTime, audio), w);
        }
    }
    float L = lumOf(col);
    col *= min(1.0, P_maxlum / max(L, 1e-3));
    fragColor = vec4(mix(term.rgb, col, P_strength), term.a);
}
