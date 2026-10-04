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
In Edit: `Enter` edits (or cycles a choice), `←→` cycle choices, `x` unsets the value.

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

`gpf install-presets` (also done on first run) installs four profiles, none with a background image:

- **shd** — ember orange on charcoal with glass blur and a flowing wave shader
- **calm-dark** — quiet blue-grey, nearly opaque, soft vignette
- **crt-green** — phosphor green with scanlines and a faint flicker
- **aurora-glass** — translucent deep blue with a slow aurora glow

Shader library: `xmb-waves`, `crt-scanlines`, `aurora`, `soft-glow`. They are written to leave text legible.

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
gpf [list | apply NAME | off | rename OLD NEW | delete NAME [--yes] | new NAME [--from X] | adopt NAME | export NAME [DEST] [--with-images] [--force]
     | import PATH [--name N] | install-presets [--force] | reload | status | unlink]
```

`gpf status` (alias `doctor`) shows where things are, whether Ghostty is running and whether
`ghostty +validate-config` is happy.

## Hot reload

Ghostty reloads its config when it receives `SIGUSR2`; `ghostty-profiles` sends that to running
`ghostty` processes (`pgrep -x ghostty`). If Ghostty isn't found, press `ctrl+shift+r` inside it.
To use another mechanism, set `GHOSTTY_PROFILES_RELOAD_CMD` to a shell command that is run instead.

## Caveats

- Ghostty shows **no error** when a custom shader fails to compile; the window just looks unchanged. The
  bundled shaders are compile-checked in this repo's tests, but your own shaders are on you.
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
