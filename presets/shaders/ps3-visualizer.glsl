// Mirrored liquid drapery, inspired by the PS3 music visualizer (original code; only a single reference frame was looked at).
// Ten translucent banks of silk fold toward a pinched valley under a black void. Each bank's folds are bent by
// domain-warped value noise so they intertwine irregularly; thin glossy crest highlights break up along the folds,
// far banks sink into a teal haze. Cheap: no raymarch, no texture reads other than the terminal frame.
// Oscillating folds and breathing swells have no net screen-space direction.
//
// ORIENTATION: Ghostty's fragCoord has its origin at the TOP-left, so y grows DOWNWARD on screen. This shader is written
// directly in that space (uv.y = 0 is the top): the void is at small y, the near fabric at large y.
//
// The beat is simulated; the `// AUDIO:` line below marks where a smoothed 0..1 audio level would replace it
// (Ghostty supplies no audio today). It sits behind the text: only plain-background pixels are drawn.
// @motion none
// @coverage full
// @float opacity 1.0 0.0 1.0 "Opacity"
// @color color_a #07565d "Teal body"
// @color color_b #369e9e "Aqua mid"
// @color color_c #becdca "Ridge highlight"
// @float strength 0.85 0.0 1.5 "Strength"
// @float speed 0.22 0.0 1.0 "Flow speed"
// @float pulse 0.35 0.0 1.0 "Beat strength"
// @float tempo 72.0 30.0 180.0 "Tempo (BPM)"
// @float glossiness 0.65 0.0 1.0 "Glossiness"
// @float ridge 0.55 0.0 1.0 "Highlight sharpness"
// @float detail 0.6 0.0 1.0 "Fold irregularity"
// @preset ps3-teal color_a=#07565d color_b=#369e9e color_c=#becdca strength=0.85
// @preset midnight color_a=#071b49 color_b=#235a96 color_c=#acbfdc strength=0.85
// @preset amber color_a=#63300d color_b=#be8036 color_c=#dfcfb2 strength=0.85
// @preset rose color_a=#511d3c color_b=#aa587d color_c=#d9c4d0 strength=0.85
// @preset mono color_a=#303c40 color_b=#758588 color_c=#d3d9d8 strength=0.85

float h1(float n) { return fract(sin(n * 127.1) * 43758.5453); }

// smooth 1D value noise in 0..1
float vn(float p) {
    float i = floor(p);
    float f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(h1(i), h1(i + 1.0), f);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 term = texture(iChannel0, uv);
    // Mask against the profile background; preserve ink, selections and alpha.
    float free = 1.0 - gp_textMask(fragCoord, term);
    if (free < 0.999 || P_strength <= 0.0) {
        fragColor = term;
        return;
    }
    // Deliberately y-DOWN: top is the void, bottom is the near fabric.
    // Width-normalized valley keeps BOTH banks visible in portrait windows.
    float x = abs(uv.x * 2.0 - 1.0);
    float t = iTime * P_speed;
    float phase = iTime * P_tempo / 60.0;
    // Smooth periodic beat, with a slow envelope that accents occasional beats.
    float beat = pow(0.5 + 0.5 * cos(6.2831853 * phase), 8.0);
    beat *= 0.25 + 0.75 * pow(0.5 + 0.5 * sin(phase * 0.47), 4.0);
    // AUDIO: replace beat above with a smoothed audio-level uniform (0..1).
    float audio = P_pulse * beat;
    float swell = 0.55 * sin(t * 0.73) + 0.30 * sin(t * 1.19 + 1.7)
                + 0.15 * sin(t * 2.13 + 3.0);
    vec3 haze = P_color_a * 0.30;
    vec3 col = vec3(0.001, 0.002, 0.004);
    // Ten translucent banks, composited far to near.
    for (int i = 0; i < 10; i++) {
        float f = float(i) / 9.0;
        float depth = f * f;
        float k = 1.0 - 0.68 * f;
        float u = log(1.0 + x / (0.14 + f * 0.95)) * (4.0 + 7.0 * k);
        // two octaves of noise bend the fold coordinate: neighbouring folds braid into each other
        float warp = (vn(u * 0.55 + f * 7.3 + t * 0.33) - 0.5) + 0.5 * (vn(u * 1.45 - f * 3.1 - t * 0.27) - 0.5);
        float irregular = (0.35 + 3.2 * P_detail) * (0.35 + 0.65 * smoothstep(0.0, 0.3, x));   // calmer where the valley pinches
        float fold = sin(u + f * 8.0 - t * 0.7 + warp * irregular)
                   + 0.42 * sin(u * 1.83 - f * 12.0 + t * 0.53 + warp * irregular * 0.7)
                   + 0.18 * sin(u * 3.1 + f * 5.0 + t * 0.31)
                   + P_detail * 0.9 * smoothstep(0.05, 0.35, x) * (vn(u * 2.1 + f * 11.0 + t * 0.2) - 0.5);
        float crest = 0.565 + depth * 0.64 - 0.65 * pow(1.0 - f, 1.6) * (pow(x * x + 0.004, 0.40) - pow(0.004, 0.40));
        crest += (0.028 + f * 0.037) * fold * (0.50 + 0.50 * sqrt(x));
        crest += (swell * 0.016 + audio * 0.019) * sin(x * 7.0 + f * 5.0);
        float d = uv.y - crest;
        float width = 0.028 + depth * 0.19;
        float edge = max(1.4 / iResolution.y, 0.007 + depth * 0.038);
        float cover = smoothstep(-edge, edge, d);
        float face = exp(-max(d, 0.0) / width);
        float silk = 0.5 + 0.5 * sin(u * 0.63 + d * 12.0 - t * 0.45 + f * 9.0 + warp * 2.0);
        vec3 body = mix(P_color_a, P_color_b, 0.18 + 0.35 * silk + 0.22 * face);
        body *= 0.55 + 0.45 * face + 0.36 * f;
        // cloth-like mottling: the surface darkens and brightens irregularly along the folds
        body *= 0.78 + 0.45 * vn(u * 1.3 + d * 16.0 + f * 4.0 - t * 0.2);
        // far banks sink into the haze
        body = mix(body, haze, (1.0 - f) * 0.45);
        // glossy crest: a thin bright line that breaks up along the fold, plus a faint echo just below it
        float shineWidth = width * mix(0.34, 0.07, P_ridge);
        float streak = pow(vn(u * 1.1 + f * 13.0 - t * 0.45), 2.2);
        float shine = exp(-pow((d - edge) / shineWidth, 2.0));
        float echo = exp(-pow((d - edge - width * 0.85) / (shineWidth * 2.2), 2.0)) * 0.30;
        body += P_color_c * (shine + echo) * (0.08 + 1.15 * streak) * P_glossiness;
        body *= 0.52 + 0.48 * smoothstep(0.0, 0.45, f);
        // translucent: the banks behind show through, more so toward the horizon
        float alpha = mix(0.52, 0.93, vn(u * 0.8 + f * 17.0 + t * 0.15)) * (0.72 + 0.20 * f);
        col = mix(col, body, cover * alpha);
    }
    // a soft glow where the valley closes, and a darker vignette into the void above
    col += P_color_a * 0.22 * exp(-pow((x * 3.0), 2.0) - pow((uv.y - 0.60) * 5.0, 2.0)) * (1.0 + 0.6 * audio);
    col *= smoothstep(0.0, 0.22, uv.y + 0.10 * (1.0 - x));
    fragColor = vec4(mix(term.rgb, col, min(P_strength, 1.0)) * max(P_strength, 1.0), term.a);
    // Universal opacity is applied ONCE by the generated wrapper.
}
