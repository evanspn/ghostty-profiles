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
// @float speed 1.0 0.0 3.0 "Flight speed"
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

// A soft limit on how bright a pixel may get: below the knee nothing changes, above it brightness is compressed toward `top`
// (never flattened to one value), so bright scenes keep their contrast while light text still reads over them.
vec3 softLimit(vec3 c, float top) {
    float L = lumOf(c);
    float k = 0.6 * top;
    float L2 = L < k ? L : k + (L - k) / (1.0 + (L - k) / (top - k));
    return c * (L2 / max(L, 1e-4));
}

// ---- 1: rolling green hills with radial motion blur -----------------------------------------------------
// lod: 0 = full detail near the camera; fewer octaves with distance (they would only shimmer there)
float hillsH(vec2 q, float lod) {
    float h = 3.8 * vnoise(q * vec2(0.020, 0.016)) + 1.9 * vnoise(q * 0.09 + 7.0) * (1.0 - smoothstep(150.0, 400.0, lod)) + 0.5 * vnoise(q * 0.4) * (1.0 - smoothstep(30.0, 90.0, lod));
    // a round mound every so often
    vec2 cell = vec2(floor(q.x / 60.0), floor(q.y / 90.0));
    vec2 ctr = (cell + vec2(0.4 + 0.2 * hash21(cell), 0.4 + 0.2 * hash21(cell + 4.0))) * vec2(60.0, 90.0);
    float d = length((q - ctr) * vec2(1.0, 0.7));
    // compact support (exactly zero 22 units out, inside the cell): the ground has no step at the cell borders
    float mw = max(0.0, 1.0 - d * d / 484.0);
    h += 4.0 * hash21(cell + 9.0) * mw * mw;
    return h;
}

vec3 hills(vec2 p, float z, float t, float audio) {
    vec2 g = vec2(1.6 * sin(t * 0.13), z);
    // The camera height is a smooth function of position: the average of the broad terrain over a long stretch around the camera (the
    // rolling land carries it up and down gently), never a raw sample of the ground (a bump would make the whole view jump).
    float avg = 0.0;
    for (int i = 0; i < 5; i++) avg += hillsH(g + vec2(0.0, -12.0 + 12.0 * float(i)), 1e4) * 0.2;
    // and never closer to the ground just ahead than a metre: a smooth maximum, so there is no kink either
    float here = hillsH(g + vec2(0.0, 2.0), 1e4) + 1.2;
    float base = avg + 2.6;
    float camY = 0.5 * (base + here + sqrt((base - here) * (base - here) + 1.5)) + 0.25 * sin(t * 0.3);
    vec3 ro = vec3(g.x, camY, z);
    float horizon = 0.17;                                   // the horizon sits in the upper third of the frame
    vec3 rd = normalize(vec3(p.x * 1.45, (p.y - horizon) * 1.45, 1.0));   // a wide field of view
    float tt = 0.4;
    bool hit = false;
    bool open = false;
    for (int i = 0; i < 48; i++) {
        vec3 pos = ro + rd * tt;
        float d = pos.y - hillsH(pos.xz, tt);
        if (d < 0.004 * tt) { hit = true; break; }
        // steps grow with distance, so the march reaches a true horizon in few steps
        tt += clamp(d * 0.6 + 0.045 * tt, 0.05, 40.0);
        if (tt > 260.0 || (rd.y > 0.0 && pos.y > 24.0)) { open = true; break; }
    }
    // a ray that used up its steps without leaving the terrain is far ground too (hazed like the rest), never a slit of sky
    if (!hit && !open) hit = true;
    vec3 skyLow = vec3(0.30, 0.64, 0.60);
    vec3 skyHigh = vec3(0.14, 0.48, 0.55);
    vec3 col = mix(skyLow, skyHigh, clamp((p.y - horizon) * 2.2, 0.0, 1.0));
    float ang = atan(p.y - horizon, p.x);
    float r = length(vec2(p.x, p.y - horizon));
    col *= 0.985 + 0.03 * vnoise(vec2(ang * 18.0, t * 0.05));
    // a ray that runs out of steps below the horizon is far ground: it is fog, the colour of the sky at the horizon
    if (!hit && rd.y < 0.0) { hit = true; tt = 260.0; }
    if (hit) {
        vec3 pos = ro + rd * min(tt, 260.0);
        float e = 0.12 + tt * 0.02;
        vec3 n = normalize(vec3(hillsH(pos.xz - vec2(e, 0.0), tt) - hillsH(pos.xz + vec2(e, 0.0), tt), 2.0 * e,
                                hillsH(pos.xz - vec2(0.0, e), tt) - hillsH(pos.xz + vec2(0.0, e), tt)));
        float diff = clamp(0.35 + 0.8 * dot(n, normalize(vec3(-0.4, 0.8, -0.3))), 0.0, 1.0);
        vec3 dark = vec3(0.04, 0.14, 0.04);
        vec3 light = vec3(0.40, 0.58, 0.17);
        float h = hillsH(pos.xz, tt);
        vec3 grass = mix(dark, light, clamp(diff * 0.85 + 0.2 * h / 9.0, 0.0, 1.0));
        // near-field texture that rushes past: grass tufts and drifts at three scales
        float nearK = 1.0 - smoothstep(10.0, 70.0, tt);
        float tex = vnoise(pos.xz * 1.6) * 0.5 + vnoise(pos.xz * 0.5 + 3.0) * 0.35 + vnoise(pos.xz * 4.0) * 0.15;
        grass *= 0.86 + 0.24 * mix(0.5, tex, nearK);
        float streak = vnoise(vec2(ang * 20.0, log(r + 0.05) * 0.9 + z * 0.02));
        grass *= 0.94 + 0.08 * streak;
        grass *= mix(1.0, 0.7, exp(-p.x * p.x * 6.0) * smoothstep(0.0, 0.14, horizon - p.y));
        // atmospheric haze: by a few hundred metres the ground is the colour of the sky at the horizon, with no edge
        // (a power law: clear near the camera, hazy by 60 m, gone by 150 m; the far crests the march cannot resolve are already sky)
        float fog = 1.0 - exp(-pow(tt / 65.0, 1.7));
        col = mix(grass, skyLow, fog);
    }
    col *= 1.0 + 0.12 * audio;
    return col;
}

// ---- 2: the dark valley with cyan-lit crests --------------------------------------------------------------
// the valley winds in a long S, so its far end is always round a bend and lost in haze, never a visible end
float valleyC(float z) { return 7.0 * sin(z * 0.025) + 3.0 * sin(z * 0.067 + 1.3); }

float valleyH(vec2 q, float rough) {
    float x = q.x - valleyC(q.y);
    // walls rise from the floor over a few units to a plateau, higher on the left than the right, so the skyline is a V that
    // runs into the vanishing point instead of a bowl
    float wallL = 4.6 + 2.0 * vnoise(vec2(q.y * 0.05, 3.0));
    float wallR = 3.4 + 2.0 * vnoise(vec2(q.y * 0.045, 9.0));
    float wall = x < 0.0 ? wallL : wallR;
    float w = smoothstep(0.4, 5.5, abs(x));
    float h = wall * (w * w * (1.5 - 0.5 * w));
    h += 1.2 * vnoise(q * vec2(0.11, 0.08)) * smoothstep(1.0, 8.0, abs(x));
    // a rough, crumbly crest along the top of the walls
    h += rough * 1.7 * (vnoise(q * vec2(0.55, 0.45)) - 0.5) * smoothstep(3.0, 8.0, abs(x));
    return h;
}

vec3 valley(vec2 p, float z, float t, float audio) {
    vec3 ro = vec3(valleyC(z) + 0.7 * sin(t * 0.23) + 0.4 * sin(t * 0.51), 0.85 + 0.1 * sin(t * 0.37), z);
    // look along the valley a little way ahead, so the bends sweep past rather than the walls running into the camera
    float yaw = 0.8 * (valleyC(z + 30.0) - valleyC(z)) / 30.0;
    vec3 rd = normalize(vec3(p.x * 1.4 + yaw + 0.04 * sin(t * 0.31), (p.y - 0.10) * 1.4, 1.0));
    float tt = 0.3;
    float haloAcc = 0.0;
    bool hit = false;
    for (int i = 0; i < 34; i++) {
        vec3 pos = ro + rd * tt;
        float d = pos.y - valleyH(pos.xz, 0.6);
        // only the walls make a glowing silhouette, not a ray skimming the far floor
        haloAcc = max(haloAcc, exp(-max(d / tt, 0.0) * 45.0) * smoothstep(1.0, 3.2, pos.y));
        if (d < 0.003 * tt) { hit = true; break; }
        tt += clamp(d * 0.6 + 0.025 * tt, 0.02, 40.0);
        if (tt > 900.0 || (rd.y > 0.0 && pos.y > 10.0)) break;
    }
    vec3 col = mix(P_sky * 1.0, P_sky * 0.7, smoothstep(0.0, 0.5, rd.y));
    // distance haze fades to the sky colour AT the horizon, which is also the colour of the sky just above it
    vec3 skyCol = P_sky * 1.0;
    float haze = 1.0 - exp(-tt * 0.025);
    if (hit) {
        vec3 pos = ro + rd * tt;
        float e = 0.04 + tt * 0.01;
        vec3 n = normalize(vec3(valleyH(pos.xz - vec2(e, 0.0), 0.6) - valleyH(pos.xz + vec2(e, 0.0), 0.6), 2.0 * e,
                                valleyH(pos.xz - vec2(0.0, e), 0.6) - valleyH(pos.xz + vec2(0.0, e), 0.6)));
        float diff = 0.5 + 0.5 * n.y;
        float fres = pow(1.0 - clamp(dot(n, -rd), 0.0, 1.0), 3.0);
        // rock texture on the near walls and the floor rushing under the camera
        float rock = vnoise(pos.xz * 1.3) * 0.6 + vnoise(pos.xz * 3.7 + 5.0) * 0.4;
        vec3 lit = (P_ground * 2.2 + P_rim * 0.04) * (0.35 + 0.9 * diff) * (0.5 + 1.0 * mix(0.5, rock, 1.0 - smoothstep(6.0, 40.0, tt)));
        lit += P_rim * 0.10 * pow(rock, 2.0) * (1.0 - smoothstep(4.0, 30.0, tt)) * smoothstep(0.3, 2.5, pos.y);
        float high = smoothstep(2.2, 5.0, pos.y);
        float farGlow = high * smoothstep(8.0, 36.0, tt);
        float wallH = smoothstep(1.2, 3.8, pos.y);
        lit += P_rim * P_glow * (0.9 + 0.7 * audio) * wallH * (fres * smoothstep(8.0, 40.0, tt) * 1.7 + farGlow * 0.7);
        lit += P_rim * 0.20 * smoothstep(0.6, 3.4, pos.y) * (0.3 + 0.7 * haze);
        // distant walls and floor melt into the sky colour: no far edge
        col = mix(lit, skyCol, haze);
    }
    // beyond the walls, the far end of the valley is three hazy ridges one behind another (never a flat patch of sky or floor):
    // the nearer the ridge, the darker, and the nearest is exactly the colour the far floor fades into
    vec3 floorDark = (P_ground * 2.2 + P_rim * 0.04) * 0.9;
    vec3 ridgeCol0 = mix(floorDark, skyCol, 0.96);
    vec3 ridgeCol1 = mix(floorDark, skyCol, 0.92);
    vec3 ridgeCol2 = mix(floorDark, skyCol, 0.87);
    if (!hit) {
        float u0 = rd.x * 3.0 + ro.x * 0.004;
        float u1 = rd.x * 4.5 + ro.x * 0.007 + 11.0;
        float u2 = rd.x * 6.5 + ro.x * 0.012 + 23.0;
        float top0 = 0.075 * (0.4 + vnoise(vec2(u0, 1.0)));
        float top1 = 0.050 * (0.4 + vnoise(vec2(u1, 2.0)));
        float top2 = 0.026 * (0.4 + vnoise(vec2(u2, 3.0)));
        col = mix(col, ridgeCol0, smoothstep(0.004, -0.004, rd.y - top0));
        col = mix(col, ridgeCol1, smoothstep(0.004, -0.004, rd.y - top1));
        col = mix(col, ridgeCol2, smoothstep(0.004, -0.004, rd.y - top2));
    }
    // a ray that skims the far floor without landing is the dark road running to the vanishing point, not sky
    bool farFloor = false;
    if (!hit && rd.y < 0.0) {
        farFloor = true;
        // far floor the march did not reach: the same dark floor, hazed by its (flat-ground) distance, so it joins the nearer hits seamlessly
        float dist = max(tt, 0.85 / max(-rd.y, 1e-3));
        col = mix(floorDark, ridgeCol2, 1.0 - exp(-dist * 0.025));
        // and exactly at the horizon it is the nearest ridge, so there is no line where the ground stops
        col = mix(ridgeCol2, col, smoothstep(0.0, 0.06, -rd.y));
        hit = true;
    }
    // the glow along the far crests carries on into the far floor and fades out below the horizon, so it has no edge there either
    float halo = haloAcc * ((hit && !farFloor) ? 0.0 : 1.0) * (1.0 - smoothstep(0.0, 0.06, -rd.y));
    col += P_rim * P_glow * halo * 0.75 * (1.0 + 0.6 * audio);
    // a soft glow of light hanging in the haze right at the horizon, so the far end of the valley is lit, not a dark patch
    col += P_rim * P_glow * 0.16 * exp(-abs(rd.y) * 16.0) * (hit && !farFloor ? 0.0 : 1.0);
    return col;
}

// ---- 3: water seen from just above the surface -----------------------------------------------------------------
float waveNoise(vec2 q, float t) { return vnoise(q * 0.55 + vec2(t * 0.1, 0.0)); }

vec3 waterSky(float y, vec3 sunDir, vec3 dir) {
    vec3 hor = vec3(0.80, 0.90, 0.95);
    vec3 top = vec3(0.16, 0.42, 0.78);
    vec3 c = mix(hor, top, pow(clamp(y * 1.8, 0.0, 1.0), 0.55));
    c += vec3(1.0, 0.95, 0.80) * pow(max(dot(dir, sunDir), 0.0), 60.0) * 0.9;
    return c;
}

vec3 water(vec2 p, float z, float t, float audio) {
    // slow and soft: half the forward speed of the other scenes, the wave clock at half rate, a little higher above the water
    z *= 0.5;
    t *= 0.5;
    float camY = 1.3;
    vec3 rd = normalize(vec3(p.x * 1.4 + 0.02 * sin(t * 0.2), (p.y - 0.17) * 1.4, 1.0));
    vec3 sun = normalize(vec3(0.05, 0.22, 1.0));
    if (rd.y >= -0.002) return waterSky(rd.y, sun, rd);
    float tt = camY / -rd.y;
    vec2 q = vec2(rd.x * tt, rd.z * tt + z);
    // ripple rings spreading from a point ahead, and wave trains travelling toward the camera
    vec2 c0 = vec2(0.0, z + 9.0);
    float rr = length(q - c0);
    float ring = sin(rr * 2.2 - t * 1.6) * exp(-rr * 0.045);
    float att = 1.0 / (1.0 + tt * 0.025);
    vec2 ph1 = vec2(0.7, 0.55) * 1.5, ph2 = vec2(-0.6, 0.8) * 2.4, ph3 = vec2(0.1, 1.0) * 3.4;
    float c1 = cos(dot(q, ph1) + t * 1.3), c2 = cos(dot(q, ph2) - t * 1.1), c3 = cos(dot(q, ph3) + t * 1.7);
    float e = 0.12;
    float n0 = waveNoise(q, t);
    vec2 slope = 0.17 * c1 * ph1 + 0.11 * c2 * ph2 + 0.05 * c3 * ph3
               + 0.5 * vec2(waveNoise(q + vec2(e, 0.0), t) - n0, waveNoise(q + vec2(0.0, e), t) - n0) / e
               + 0.28 * ring * (q - c0) / max(rr, 0.1);
    vec3 n = normalize(vec3(-slope.x * att, 1.0, -slope.y * att));
    vec3 refl = reflect(rd, n);
    refl.y = abs(refl.y);
    vec3 rsky = waterSky(refl.y, sun, refl);
    float fres = 0.04 + 0.90 * pow(1.0 - clamp(dot(n, -rd), 0.0, 1.0), 3.5);
    // the water itself: deep teal-blue near the camera, lighter where the light gets through the crests
    float crest = clamp(0.5 + 3.0 * (n0 - 0.5), 0.0, 1.0);
    vec3 body = mix(vec3(0.01, 0.16, 0.24), vec3(0.04, 0.40, 0.50), crest * att);
    vec3 col = mix(body, rsky, clamp(fres, 0.0, 1.0));
    // glints where a wave face turns toward the sun, and soft light streaks just under the surface
    col += vec3(1.0, 0.96, 0.85) * pow(max(dot(refl, sun), 0.0), 120.0) * (0.9 + 0.5 * audio);
    col += vec3(0.20, 0.55, 0.65) * pow(crest, 4.0) * 0.35 * att;
    float fog = 1.0 - exp(-tt * 0.03);
    return mix(col, vec3(0.80, 0.90, 0.95), fog);
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

// Every scene is brought to about the same average brightness (measured: hills 101, valley 36, water 109, silk 58, wash 71,
// tunnel 37 out of 255 before this), so a cross-fade is a change of picture and never a step in light.
vec3 renderScene(int id, vec2 p, float ts, float t, float audio) {
    float z = (ts + float(id) * 37.0) * (P_speed * 6.0) + audio * 0.6;
    if (id == 1) return 0.65 * hills(p, z, t, audio);
    if (id == 2) return 1.5 * valley(p, z, t, audio);
    if (id == 3) return 0.57 * water(p, z, t, audio);
    if (id == 4) return 1.12 * silk(p, t, audio);
    if (id == 5) return 0.95 * wash(p, t, audio);
    return 1.6 * tunnel(p, z * 0.6, t, audio);
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
    // (the beat uses only the fractional part of the phase: after hours the phase is huge and cos() of it would be noise)
    float beat = pow(0.5 + 0.5 * cos(6.2831853 * fract(phase)), 8.0);
    beat *= 0.25 + 0.75 * pow(0.5 + 0.5 * sin(phase * 0.47), 4.0);
    // AUDIO: replace beat above with a smoothed audio-level uniform (0..1).
    float audio = P_pulse * beat;

    vec3 col;
    if (P_scene >= 0.5) {
        // one scene locked: its flight clock restarts every 40 minutes, dissolving into the start over the last seconds, so the
        // positions stay small however long it runs
        int sid = int(P_scene + 0.5);
        float tl = mod(iTime, 2400.0);
        col = renderScene(sid, p, tl, iTime, audio);
        if (tl > 2400.0 - P_fade) {
            float w = smoothstep(0.0, 1.0, (tl - (2400.0 - P_fade)) / P_fade);
            col = mix(col, renderScene(sid, p, tl - 2400.0, iTime, audio), w);
        }
    } else {
        int nd = max(1, min(6, int(floor(log(max(P_playlist, 1.0)) / 2.302585) + 1.0)));
        float slotF = floor(iTime / P_scene_period);
        float into = iTime - slotF * P_scene_period;
        int k = int(mod(slotF, float(nd)));
        int idA = clamp(digitAt(P_playlist, k, nd), 1, 6);
        // each slot flies from its own stretch of country; the offset cycles through 64 stretches, so nothing grows without bound
        // and a pane left running for days keeps the same precision (the cycle seam is inside a cross-fade)
        col = renderScene(idA, p, into + mod(slotF, 64.0) * 11.0, iTime, audio);
        float fadeStart = P_scene_period - P_fade;
        if (into > fadeStart) {
            int idB = clamp(digitAt(P_playlist, int(mod(slotF + 1.0, float(nd))), nd), 1, 6);
            float w = smoothstep(0.0, 1.0, (into - fadeStart) / P_fade);
            col = mix(col, renderScene(idB, p, (into - P_scene_period) + mod(slotF + 1.0, 64.0) * 11.0, iTime, audio), w);
        }
    }
    col = softLimit(col, P_maxlum);
    fragColor = vec4(mix(term.rgb, col, P_strength), term.a);
}
