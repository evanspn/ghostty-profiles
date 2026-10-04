# ghostty-profiles

A small terminal UI for [Ghostty](https://ghostty.org) that lets you browse themes
and save, switch and tweak complete **looks** (profiles): colors, fonts, cursor,
opacity and blur, a background image and custom shaders. Edits apply to your
running Ghostty as you make them (hot reload).

Written in Rust with [ratatui](https://ratatui.rs). Two names for one program:
`ghostty-profiles` and the short `gpf`.

## Install

```sh
cargo install --git https://github.com/evanspn/ghostty-profiles
```

You need a Rust toolchain (1.88 or newer). No Ghostty files are needed at build time:
the 463 built-in Ghostty themes and the presets are compiled into the binary.

## Quick start

```sh
gpf                      # open the TUI (installs the presets on first run)
gpf list                 # list profiles, ● marks the active one
gpf apply shd            # make a profile the active look and reload Ghostty
gpf off                  # no profile: your own Ghostty config shows through again
gpf adopt mine           # turn your current Ghostty look into a profile
```

The first `apply` adds **one** line to your Ghostty config (a backup is kept):

```
config-file = ?ghostty-profiles-active.conf
```

Everything else lives in `~/.config/ghostty-profiles/`.

### Turning a profile off

Want your own config back for a while? Pick **`(none)`** (the first row of the Profiles tab, or press `u`), or run `gpf off`.
The generated file is emptied (so nothing from any profile applies), the include line stays, and Ghostty is reloaded so your own
settings show immediately. Applying a profile again turns it back on. `(none)` is marked ● while no profile is active, and
`gpf status` says `active profile  : none`. Your main Ghostty config is never touched by `off`.

`gpf unlink` is the stronger step: it removes the include line itself. Use `off` day to day and `unlink` to uninstall.

## The TUI

| Tab | What it does |
| --- | --- |
| **Profiles** | `(none)` plus your profiles, with a live preview. `Enter` applies one (on `(none)`: turns the look off). |
| **Themes** | All themes with a filter (`/`) and color swatches. `Enter` bakes the theme's colors into the current profile. |
| **Edit** | Colors (including the 16 ANSI colors, hex-validated, with swatches), font family/size/thickness/features, cell adjustments, cursor, opacity/blur/padding, background image path/opacity/fit/position. |
| **Shaders** | The bundled shader library plus the profile's own. `Enter` toggles, `a` toggles animation. |

Keys: `Tab`/`Shift-Tab` or `1`-`4` switch tabs, `↑↓` move, `u` off (no profile), `n` new profile (a copy of the selected
one), `r` rename, `d` delete (asks for `y`), `e` export, `p` install presets, `ctrl+r` force reload, `q` quit.
In Edit: `Enter` edits (or cycles a choice), `←→` cycle choices, `x` unsets the value, `p` opens the color picker.

### The color picker

On any color field (background, foreground, cursor, selection, the 16 ANSI colors) press `p`, or **click the field's swatch**,
to open a picker: a hue/saturation **wheel** drawn as a real circle (half-block characters, so it is round), a **brightness**
bar, a live preview of the color, and the hex in the input box below.

- **Mouse** (the TUI turns mouse reporting on while it runs and gives it back on exit): click or drag on the wheel to pick hue
  and saturation, click or drag the bar for brightness. The hex updates live. A click outside the circle is ignored; dragging
  past the rim sticks to the rim. Nothing is saved or reloaded while you drag: `Enter` accepts (autosave, re-render and the
  usual debounced hot reload), `Esc` cancels and keeps the old value.
- **Keyboard**: `←→` change hue, `↑↓` saturation, `[` `]` (or `-` `+`) brightness; hold `Shift` (or use `{` `}`) for bigger steps.
  Typing a hex in the box moves the crosshair to match.
- **Small or plain terminals**: on a short terminal the wheel shrinks and then gives way to three sliders (H, S, V); on a
  terminal without 24-bit color (`COLORTERM` is not `truecolor`) the picker and swatches use the nearest of the 256 colors.

Every valid edit is **autosaved** to the profile, the active config is re-rendered, and Ghostty
is reloaded after a short (250 ms) pause so a burst of edits causes one reload. Invalid values
(a bad hex color, an opacity of 3) are refused with a message and change nothing.

## Making a profile

On the Profiles tab press `n` (or select the `+ New profile` row and press `Enter`), type a name, then choose what it starts from:

1. **your current Ghostty setup**: the same as `gpf adopt NAME`. Your appearance settings (colors, fonts, cursor, opacity,
   shaders, background) are *moved* out of your Ghostty config into the profile; keybinds and other settings stay. The
   originals are backed up as `*.bak-pre-ghostty-profiles`, and you are asked to confirm first.
2. **a copy of the selected profile**
3. **blank**

The new profile appears in the list and is selected, but it is **not applied** until you press `Enter` on it. (After choosing
option 1 your look lives only in the new profile, so until you apply it a Ghostty reload shows the defaults.) Then use the Edit
tab as usual. Names are letters, digits, `.`, `_` and `-`, and must not already exist; mistakes are explained under the name box.

## Renaming a profile

In the TUI select a profile and press `r` (the box is pre-filled with the current name; Enter confirms, Esc cancels), or run
`gpf rename OLD NEW`. The whole folder moves, so its `images/` and `shaders/` come along unchanged. Renaming the **active**
profile keeps it active and updates the generated config (which holds absolute paths) and reloads Ghostty, so nothing is left
pointing at the old folder. A bundled preset can be renamed too; `gpf install-presets` will then install the original again.
Names follow the same rules as new profiles.

## Deleting a profile

In the TUI select a profile and press `d`, then `y` (any other key cancels). From the shell: `gpf delete NAME`
(alias `gpf rm`; `--yes` skips the question). The whole profile folder goes, including its `images/` and
`shaders/`. The active profile cannot be deleted: apply another one first. Bundled presets can be deleted too;
`gpf install-presets` brings them back.

## Profiles

A profile is a folder:

```
~/.config/ghostty-profiles/profiles/shd/
├── profile.conf      native Ghostty `key = value` syntax, plus a `# description:` comment
├── shaders/          shader files this profile uses
└── images/           background images (only if you add one)
```

`profile.conf` is plain Ghostty config, so anything Ghostty accepts works, and comments and key
order are preserved when the tool edits it. Asset paths inside it are **relative**
(`custom-shader = shaders/xmb-waves.glsl`); when applied, they are written as absolute paths
into the generated `~/.config/ghostty/ghostty-profiles-active.conf`, which is what Ghostty loads.

### Presets

`gpf install-presets` (also done on first run) installs these profiles, none with a background image:

| Profile | Look |
| --- | --- |
| **shd** | ember orange on charcoal with glass blur and the flowing XMB wave shader |
| **ps3-classic** | deep blue gradient with slow white and blue XMB ribbons |
| **dusk** | purple and pink XMB waves over deep violet |
| **mono-waves** | grayscale XMB waves, tintable |
| **aurora-glass** | translucent deep blue with slow aurora curtains behind the text (no blur) |
| **enchanted-night** | glowing runes drifting up behind the text (enchanting-table inspired) |
| **rainy-day** | chunky blue pixel rain on dark slate |
| **matrix** | falling green glyph columns |
| **deep-space** | drifting parallax stars (warp streaks are one slider away) |
| **calm-dark** | quiet blue-grey, nearly opaque, a soft vignette |
| **crt-green** | phosphor green with scanlines and a faint flicker |

## Shaders

A profile uses **one** shader (or none). The Shaders tab is a radio list: `(none)`, the library, and a **Your shaders** section for
shader files you wrote yourself in the profile's `shaders/` folder. `Enter` makes the shader under the cursor *the* shader (it replaces
the current one and the old one's generated files are removed); `a` toggles animation. **Browsing writes nothing:** a bundled shader is
copied into the profile only when you select it, and the cursor on one that is not in use shows what it declares, read-only.

| Shader | What it draws |
| --- | --- |
| `xmb-waves` | PS3-style flowing ribbons (presets: ember, ocean, forest, sakura, mono) |
| `xmb-classic` | the original look: white/blue ribbons over a deep blue gradient (classic, midnight, daybreak) |
| `xmb-dusk` | purple and pink waves (dusk, twilight, rose) |
| `xmb-mono` | grayscale waves with one tint (mono, warm, cool) |
| `xmb-aurora-ribbons` | ribbons of light hanging from the top (borealis, ice, ember) |
| `aurora` | slow curtains of light across the top, behind the text (borealis, arctic, solar) |
| `enchant-glyphs` | strings of glowing **Standard Galactic Alphabet** letters (the script of the Minecraft enchanting table) rising up the screen (enchanted, emerald, ember, sparse) |
| `pixel-rain` | blocky rain falling at a slight angle with splash pixels and an optional thunder flash (drizzle, rain, storm, night) |
| `matrix-rain` | sparse falling glyph columns with bright heads and short dim trails (matrix, cyber, amber, red) |
| `starfield` | three parallax layers of stars drifting left, optional warp streaks (deep-space, hyperdrive, warm) |
| `snow` | soft falling snow in three layers (snowfall, blizzard, ash) |
| `fireflies` | a few drifting glowing dots that pulse (fireflies, lanterns, spirits) |
| `soft-glow` | a gentle vignette on the background; an optional text glow (off by default, because it softens text) |
| `crt-scanlines` | scanlines, vignette and a faint flicker |

All of them are original code. `enchant-glyphs` draws the 26 letters of the Standard Galactic Alphabet (a constructed alphabet created for the
Commander Keen games, with one symbol per Latin letter; it is what Minecraft's enchanting table displays) from **hand-built stroke data**
written for this project: a few line segments per letter on a 12x12 grid. No font file, texture or artwork from any game is used or included.
The strings are random letters, and the same screen position always spells the same letters. The GPU tests render all 26 letterforms to a
chart and check that each is drawn and that no two look alike.

### Behind the text, never over it

Every effect shader is gated by a **text mask** that `gpf` generates into the shader's header (`gp_textMask`). The mask is 1 wherever the
terminal drew anything that is not plain background (text of **any** color, including dim `#4a4a4a` and near-black text, the cursor,
selections, inverse video) and on a 1-2 pixel fringe around it, so anti-aliased edges are covered; it is 0 on plain background. The effect
is only added where the mask is 0. The mask compares each pixel with the terminal's **background color**, which `gpf` takes from the
profile (`background`, or the bundled theme named by `theme`, else Ghostty's default `#282c34`) and writes into the header as `P_bg`,
so it also works on Ghostty builds that do not expose `iBackgroundColor`. The only neighbour reads are twelve taps within two pixels
and they feed the mask alone: a color is never blended with its neighbours, so no effect can blur text.

Limits, stated plainly:

- With a **background image** the background is not one color, so the mask compares each pixel with a local estimate of the picture
  (four taps 14 px out) and is more conservative. Text that is nearly the color of the picture under it cannot be told apart, and a large
  flat block (a selection, a panel) looks like a patch of picture, so effects can show over it.
- Text drawn in the background color inside an inverse-video block (the "hole" of a glyph) looks like background.
- The mask is a color test, so it cannot protect a pixel that is exactly the background color but part of a glyph.
- `crt-scanlines` modulates every pixel by design (that is what a CRT does) and is not gated.

### Orientation (important when you write a shader)

**In Ghostty `fragCoord` has its origin at the TOP-left and y grows DOWNWARD** (verified from the shader prefix Ghostty embeds in its
own binary: it calls `mainImage(_fragColor, gl_FragCoord.xy)` with Metal's top-left position, and `iChannel0` is sampled with
`fragCoord / iResolution.xy` and no flip). **Shadertoy is the other way round** (origin bottom-left, y up), so a Shadertoy shader that
moves things "down" moves them *up* in Ghostty. A read-back image has row 0 at the top, which is `fragCoord.y = 0`.

The generated header gives every shader `vec2 gp_yup(vec2 fragCoord)`, which returns familiar y-**up** coordinates (up = +y, falling =
-y). The directional shaders here compute in y-up space with it, and declare their intent with `// @motion`:

```glsl
// @motion down        down | up | left | right | radial | none   (as the user sees it on screen)
```

`@motion` is parsed (shown by `gpf shader show`) and **tested**: the GPU tests render two frames a short time apart and measure the
dominant shift of the effect in screen space (row index grows downward), then require it to match the declaration. Rain, matrix and
snow fall down, `enchant-glyphs` rises up on purpose, `starfield` drifts left.

### Opacity

Every shader has an `opacity` parameter (the first row of its parameters) that scales the **effect layer** (what the shader adds), never
the terminal's text. It is implemented by the generated header and footer, so a shader gets it by declaring
`// @float opacity 0.6 0.0 1.0 "Opacity"`: the shader's own `mainImage` becomes `gp_effect` and a generated `mainImage` blends it over the
untouched terminal with `mix(base, effect, P_opacity)`. The profile also has a **master effects opacity** that multiplies every shader's
own: the bar at the top of the Shaders tab (`[` / `]` or click/drag it, `O` to type a value), or `gpf shader opacity PROFILE 0.5`. It lives in
`effects.params` in the profile folder and travels with exports.

### Not blocking the screen

Particle effects (fireflies, snow, rain, matrix, stars, glyphs) are kept sparse: a **coverage budget** is part of the tests. At its default
parameters an effect may add at most 6% light on average over the background and light at most 8% of the pixels noticeably, unless it
declares `// @coverage full` (waves and ribbons, which are meant to span the screen). A shader that floods the screen fails the build.

### Tuning a shader

With the profile's shader selected, press `→` in the Shaders tab:

- the **preset** row (`←→`) switches between the shader's named presets (for example `ember`, `ocean`, `forest`);
- a **color** row: `Enter`/`p` or a click on its swatch opens the same hue wheel as the Edit tab;
- a **number** row: `←→` nudge it (`Shift` for bigger steps), click or drag its bar, or `Enter` to type a value;
- `R` resets the shader to its defaults, `Esc` goes back to the list.

Every change is saved, the shader copy is regenerated, and Ghostty is reloaded after the usual short pause. Values are kept in
`shaders/<name>.params` **only while they differ from the defaults**: a shader in its default state has no sidecar.

From the shell: `gpf shader show|set|preset|reset|use|opacity ...` (for example `gpf shader use PROFILE snow`,
`gpf shader set PROFILE snow opacity 0.4`, `gpf shader use PROFILE none`).

### One shader per profile, and the tidy-up

The profile folder holds only the shader in use (its rendered `.glsl`, and its `.params` while they hold a change), plus anything you wrote.
Older versions let a profile collect every shader you browsed; those are cleaned up automatically (when a profile is applied, and when it
is loaded in the TUI) or on demand:

```sh
gpf prune --dry-run     # show what would go
gpf prune [PROFILE]     # tidy one profile (default: all)
```

A prune removes only files it can prove are generated: copies identical to a bundled shader (or to a previous release's) that are not the
profile's shader, and parameter files holding only defaults. **Anything you wrote or changed is kept** (shader files appear under
*Your shaders*; parameter files with changed values for shaders that are not in use are kept and reported; `--drop-orphan-params` removes
those too). A profile folder is **backed up once**, before the first deletion, to `~/.config/ghostty-profiles/backups/NAME.bak-pre-prune`.
A profile that lists several `custom-shader` lines keeps the first and says so.

### Writing a tunable shader

A shader declares its tunable values in comments:

```glsl
// @motion down
// @float opacity 0.6 0.0 1.0 "Opacity"
// @color wave_a #ff6b1a "Wave color"
// @float strength 0.16 0.0 0.5 "Strength"          (default, min, max)
// @preset ocean wave_a=#2fa8ff strength=0.18
```

and uses them as `P_opacity`, `P_wave_a` (a `vec3`) and `P_strength` (a `float`). The profile keeps its values in
`shaders/<name>.params` (plain `name = value` lines); when the profile is applied, a header of `const` declarations (plus `P_bg`, `gp_textMask`,
`gp_yup`) is generated at the top of the profile's copy of the shader. Names are lowercase `a-z0-9_`; presets may also use `-`.
`// @coverage full` exempts a shader that is meant to span the screen from the coverage budget. A malformed annotation is an error
naming its line, never silently ignored.

**Safety:** a `.params` file can arrive inside a shared profile. Nothing from it is ever pasted into GLSL as text: each value
is parsed (a hex color, or a finite number inside the declared range) and the header is built from the parsed numbers, so it can
only choose numbers. Invalid values fall back to the defaults. `export` carries the `.params` file, the profile's `effects.params` and the
rendered shader; `import` still strips everything but appearance settings.

Untouched copies of shaders from older releases are upgraded to the current version the next time the profile is applied; a copy you
changed is left alone.

### Sharing and your own images

```sh
gpf export shd ./shd            # portable folder; background images are LEFT OUT
gpf export shd ./shd --with-images
gpf import ./shd --name shd2
```

**Privacy default:** `export` never includes background images unless you pass `--with-images`, and
shaders are copied in so an export is self-contained. The presets in this repository ship without any
image, and `.gitignore` excludes common image formats so a wallpaper cannot be committed by accident.
`import` refuses profiles whose asset paths point outside their own folder.

**A profile can only change how Ghostty looks.** Only appearance settings (colors, fonts, cursor, opacity/blur/padding,
background image, shaders) are ever written into the active Ghostty config. `import` strips everything else
(`command`, `initial-command`, `keybind`, `config-file`, ...) and tells you what it removed, and applying a profile
ignores such lines even if you add them by hand. Importing someone else's profile therefore cannot make Ghostty run
programs, rebind keys or load other files. (Shaders are code that runs on your GPU, so still only use shaders you trust.)

### Adopting your current setup

`gpf adopt NAME` moves the *appearance* settings (colors, fonts, cursor, opacity, shaders, images…)
out of your Ghostty config into a new profile and leaves everything else (keybinds, shell settings)
where it is. Referenced shaders and images are copied into the profile, the originals are not touched,
and your config files are backed up once as `*.bak-pre-ghostty-profiles`. Adopting does not apply anything.

## Commands

```
gpf [list | apply NAME | off | rename OLD NEW | delete NAME [--yes] | prune [PROFILE] [--dry-run] | new NAME [--from X] | adopt NAME | export NAME [DEST] [--with-images] [--force]
     | import PATH [--name N] | install-presets [--force] | reload | status | unlink]
```

`gpf status` (alias `doctor`) shows where things are, whether Ghostty is running and whether
`ghostty +validate-config` is happy.

## Hot reload

Ghostty reloads its config when it receives `SIGUSR2`; `ghostty-profiles` sends that to running
`ghostty` processes (`pgrep -x ghostty`). If Ghostty isn't found, press `ctrl+shift+r` inside it.
To use another mechanism, set `GHOSTTY_PROFILES_RELOAD_CMD` to a shell command that is run instead.

## Caveats

- Ghostty shows **no error** when a custom shader fails to compile; the window just looks unchanged. The bundled shaders are
  compile-checked in this repo's tests (at their defaults, every preset and extreme values), and when a GPU is available the
  tests also **run** every shader on it over a synthetic terminal frame (Metal on a Mac, via wgpu): text pixels must come out
  untouched, no shader may read neighbouring pixels, each must visibly draw something, and strength 0 must change nothing. That
  is the real shader code executing, but it is not Ghostty: Ghostty's own pipeline may differ in small ways. Your own shaders
  are on you.
- Config location is `$XDG_CONFIG_HOME` or `~/.config`. The macOS `~/Library/Application Support`
  location is not managed.
- The Edit tab covers the common appearance settings. Anything else can be added by hand in
  `profile.conf`; it is preserved.
- Themes are baked into a profile as plain colors (a copy, not a reference to the theme).

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Tests run in a temporary config home and never touch your real `~/.config` or signal a running Ghostty.

## License

MIT. The bundled themes come from [iTerm2-Color-Schemes](https://github.com/mbadolato/iTerm2-Color-Schemes) (MIT); see `themes/README.md`.
