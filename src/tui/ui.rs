//! Drawing. Pure functions of [`App`]; nothing here changes state.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap};

use super::app::{App, Comp, Tab, Top, WizardStep};
use super::color::{self, Hsv, WheelGeom};
use super::fields::Kind;
use crate::ghostty::{ThemeColors, theme_colors};
use crate::profile::{Profile, hex_rgb};

const ACCENT: Color = Color::Yellow;

fn rgb(hex: &str) -> Option<Color> {
    hex_rgb(hex).map(|(r, g, b)| color::term_color(r, g, b))
}

fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn swatch(hex: Option<&str>) -> Span<'static> {
    match hex.and_then(rgb) {
        Some(c) => Span::styled("██", Style::default().fg(c)),
        None => Span::styled("··", dim()),
    }
}

/// The colors a preview paints with, from either a profile or a theme.
struct Colors {
    background: Option<String>,
    foreground: Option<String>,
    cursor: Option<String>,
    selection_bg: Option<String>,
    selection_fg: Option<String>,
    palette: [Option<String>; 16],
}

impl Colors {
    fn from_profile(p: &Profile) -> Self {
        let pal = p.palette();
        Colors {
            background: p.color("background"),
            foreground: p.color("foreground"),
            cursor: p.color("cursor-color"),
            selection_bg: p.color("selection-background"),
            selection_fg: p.color("selection-foreground"),
            palette: std::array::from_fn(|i| pal.get(&(i as u8)).cloned()),
        }
    }

    fn from_theme(c: &ThemeColors) -> Self {
        Colors {
            background: c.background.clone(),
            foreground: c.foreground.clone(),
            cursor: c.cursor_color.clone(),
            selection_bg: c.selection_background.clone(),
            selection_fg: c.selection_foreground.clone(),
            palette: std::array::from_fn(|i| c.palette.get(&(i as u8)).cloned()),
        }
    }
}

fn preview(f: &mut Frame, area: Rect, title: &str, c: &Colors, extra: Vec<Line<'static>>) {
    let block = Block::default().borders(Borders::ALL).title(format!(" {title} "));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = vec![Line::from(vec![
        Span::raw("bg "),
        swatch(c.background.as_deref()),
        Span::raw("  fg "),
        swatch(c.foreground.as_deref()),
        Span::raw("  cursor "),
        swatch(c.cursor.as_deref()),
        Span::raw("  selection "),
        swatch(c.selection_bg.as_deref()),
    ])];
    for row in 0..2 {
        let mut spans = vec![Span::raw(if row == 0 { "ansi  " } else { "bright" })];
        for i in 0..8 {
            spans.push(Span::raw(" "));
            spans.push(swatch(c.palette[row * 8 + i].as_deref()));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));
    lines.extend(extra);

    let head_h = lines.len() as u16;
    let [top, sample] =
        Layout::vertical([Constraint::Length(head_h.min(inner.height)), Constraint::Min(0)]).areas(inner);
    f.render_widget(Paragraph::new(lines), top);

    // a miniature terminal painted with the colors
    let base = Style::default().bg(c.background.as_deref().and_then(rgb).unwrap_or(Color::Reset)).fg(c
        .foreground
        .as_deref()
        .and_then(rgb)
        .unwrap_or(Color::Reset));
    let pal = |i: usize| c.palette[i].as_deref().and_then(rgb).map(|col| base.fg(col)).unwrap_or(base);
    let sel = Style::default().bg(c.selection_bg.as_deref().and_then(rgb).unwrap_or(Color::Reset)).fg(c
        .selection_fg
        .as_deref()
        .and_then(rgb)
        .unwrap_or(Color::Reset));
    let cur = Style::default().bg(c.cursor.as_deref().and_then(rgb).unwrap_or(Color::Reset));
    let term = vec![
        Line::from(vec![
            Span::styled("~/code ", pal(4)),
            Span::styled("main ", pal(5)),
            Span::styled("$ ", pal(2)),
            Span::styled("cargo test", base),
        ]),
        Line::from(vec![Span::styled("   Compiling ", pal(2)), Span::styled("ghostty-profiles", base)]),
        Line::from(vec![Span::styled("warning: ", pal(3)), Span::styled("unused variable", base)]),
        Line::from(vec![Span::styled("error[E0382]: ", pal(1)), Span::styled("borrow of moved value", base)]),
        Line::from(vec![
            Span::styled(" selected text ", sel),
            Span::styled(" bold ", base.add_modifier(Modifier::BOLD)),
            Span::styled(" dim ", pal(8)),
        ]),
        Line::from(vec![Span::styled("$ ", pal(2)), Span::styled(" ", cur)]),
    ];
    f.render_widget(Paragraph::new(term).style(base), sample);
}

pub fn draw(f: &mut Frame, app: &App) {
    color::set_truecolor(app.truecolor);
    *app.picker_rects.borrow_mut() = Default::default();
    app.swatch_rects.borrow_mut().clear();
    let [tabs, body, footer] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(5), Constraint::Length(3)]).areas(f.area());

    let titles: Vec<Line> =
        Tab::ALL.iter().enumerate().map(|(i, t)| Line::from(format!(" {} {} ", i + 1, t.title()))).collect();
    let selected = Tab::ALL.iter().position(|t| *t == app.tab).unwrap_or(0);
    f.render_widget(
        Tabs::new(titles)
            .select(selected)
            .block(Block::default().borders(Borders::ALL).title(" ghostty-profiles "))
            .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
        tabs,
    );

    match app.tab {
        Tab::Profiles => draw_profiles(f, body, app),
        Tab::Themes => draw_themes(f, body, app),
        Tab::Edit => draw_edit(f, body, app),
        Tab::Shaders => draw_shaders(f, body, app),
    }
    draw_footer(f, footer, app);
    if app.wizard.is_some() {
        draw_wizard(f, app);
    } else if app.input.is_some() {
        if app.picker.is_some() {
            draw_picker(f, app);
        }
        draw_input(f, app);
    }
}

fn halves(area: Rect) -> [Rect; 2] {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .areas(area)
}

fn list_block(title: &str) -> Block<'static> {
    Block::default().borders(Borders::ALL).title(format!(" {title} "))
}

fn highlight() -> Style {
    Style::default().add_modifier(Modifier::REVERSED)
}

fn draw_profiles(f: &mut Frame, area: Rect, app: &App) {
    let [left, right] = halves(area);
    let mark = |on: bool| Span::styled(if on { "● " } else { "  " }, Style::default().fg(ACCENT));
    let mut items: Vec<ListItem> = vec![
        ListItem::new(Line::styled("+ New profile", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))),
        ListItem::new(Line::from(vec![mark(app.active.is_none()), Span::styled("(none)", dim())])),
    ];
    items.extend(app.profiles.iter().map(|n| {
        ListItem::new(Line::from(vec![mark(app.active.as_deref() == Some(n.as_str())), Span::raw(n.clone())]))
    }));
    let row = match app.top {
        Some(Top::New) => 0,
        Some(Top::None) => 1,
        None => app.sel + 2,
    };
    let mut st = ListState::default().with_selected(Some(row));
    f.render_stateful_widget(
        List::new(items).block(list_block("Profiles (● = active)")).highlight_style(highlight()),
        left,
        &mut st,
    );

    match app.top {
        Some(Top::New) => {
            let text = vec![
                Line::from("Make a new profile (Enter, or press n anywhere)."),
                Line::raw(""),
                Line::from("You type a name, then choose what it starts from:"),
                Line::from("  1  your current Ghostty setup"),
                Line::from("  2  a copy of the profile you had selected"),
                Line::from("  3  blank"),
                Line::raw(""),
                Line::styled("A new profile is never applied until you press Enter on it.", dim()),
                Line::raw(""),
                Line::from("Also on this tab: r renames, d deletes (asks first) and e exports the selected profile."),
            ];
            f.render_widget(Paragraph::new(text).block(list_block("New profile")).wrap(Wrap { trim: true }), right);
            return;
        }
        Some(Top::None) => {
            let on = app.active.is_none();
            let text = vec![
                Line::from(if on { "No profile is active." } else { "Enter turns the active profile off." }),
                Line::raw(""),
                Line::from("Your own Ghostty config is what applies while no profile is active."),
                Line::styled("Pick a profile and press Enter to apply it again. u does the same from any row.", dim()),
            ];
            f.render_widget(Paragraph::new(text).block(list_block("(none)")).wrap(Wrap { trim: true }), right);
            return;
        }
        None => {}
    }
    match &app.profile {
        Some(p) => {
            let mut extra = vec![Line::styled(p.description(), dim())];
            let shaders = p.shaders();
            extra.push(Line::from(format!(
                "shader: {}",
                if shaders.is_empty() { "none".to_string() } else { shaders.join(", ") }
            )));
            extra.push(Line::from(format!("background image: {}", if p.image().is_some() { "yes" } else { "no" })));
            extra.push(Line::raw(""));
            preview(f, right, &p.name, &Colors::from_profile(p), extra);
        }
        None => f.render_widget(
            Paragraph::new("No profiles yet. Press n to make one, or p to install the presets.")
                .block(list_block("Preview"))
                .wrap(Wrap { trim: true }),
            right,
        ),
    }
}

fn draw_themes(f: &mut Frame, area: Rect, app: &App) {
    let [left, right] = halves(area);
    let items: Vec<ListItem> = app
        .theme_view
        .iter()
        .filter_map(|i| app.themes.get(*i))
        .map(|t| ListItem::new(if t.user { format!("{} (yours)", t.name) } else { t.name.clone() }))
        .collect();
    let title = if app.theme_filter.is_empty() {
        format!("Themes ({})  / filter", app.theme_view.len())
    } else {
        format!("Themes ({})  filter: {}", app.theme_view.len(), app.theme_filter)
    };
    let mut st = ListState::default().with_selected((!app.theme_view.is_empty()).then_some(app.theme_sel));
    f.render_stateful_widget(List::new(items).block(list_block(&title)).highlight_style(highlight()), left, &mut st);

    match app.selected_theme() {
        Some(t) => {
            let target = app.profile.as_ref().map(|p| p.name.as_str()).unwrap_or("no profile");
            let extra = vec![Line::styled(format!("Enter bakes these colors into '{target}'"), dim()), Line::raw("")];
            preview(f, right, &t.name, &Colors::from_theme(&theme_colors(&t.text)), extra);
        }
        None => f.render_widget(Paragraph::new("No theme matches the filter.").block(list_block("Preview")), right),
    }
}

fn draw_edit(f: &mut Frame, area: Rect, app: &App) {
    let [left, right] = halves(area);
    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_row = 0;
    let mut group = "";
    let mut swatches: Vec<(usize, usize, usize)> = Vec::new(); // (field, item row, value width)
    for (i, fld) in app.fields.iter().enumerate() {
        if fld.group != group {
            group = fld.group;
            items.push(ListItem::new(Line::styled(format!("── {group}"), dim())));
        }
        if i == app.field_sel {
            selected_row = items.len();
        }
        let value = app.field_value(i);
        let mut spans = vec![Span::raw(format!("  {:<15} ", fld.label))];
        match &value {
            Some(v) => spans.push(Span::raw(v.clone())),
            None => spans.push(Span::styled("—", dim())),
        }
        if matches!(fld.kind, Kind::Hex | Kind::Palette(_)) {
            spans.push(Span::raw(" "));
            spans.push(swatch(value.as_deref()));
            swatches.push((i, items.len(), value.as_ref().map(|v| v.chars().count()).unwrap_or(1)));
        }
        items.push(ListItem::new(Line::from(spans)));
    }
    let name = app.profile.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| "no profile".into());
    let block = list_block(&format!("Edit '{name}'"));
    let inner = block.inner(left);
    let mut st = ListState::default().with_selected(Some(selected_row));
    f.render_stateful_widget(List::new(items).block(block).highlight_style(highlight()), left, &mut st);

    // the color swatches (and the hex beside them) are clickable: they open the picker
    let offset = st.offset();
    let mut rects = app.swatch_rects.borrow_mut();
    for (field, item, vw) in swatches {
        if item >= offset && item - offset < inner.height as usize {
            let (x, w) = (inner.x + 18, (vw + 3) as u16);
            if x < inner.x + inner.width {
                rects.push((
                    Rect { x, y: inner.y + (item - offset) as u16, width: w.min(inner.x + inner.width - x), height: 1 },
                    field,
                ));
            }
        }
    }
    drop(rects);

    match &app.profile {
        Some(p) => {
            let extra = vec![
                Line::styled("Enter edits · p or click a swatch: color picker · ←/→ cycles choices · x unsets", dim()),
                Line::raw(""),
            ];
            preview(f, right, "Live preview", &Colors::from_profile(p), extra);
        }
        None => f.render_widget(
            Paragraph::new("Select a profile on the Profiles tab first.").block(list_block("Preview")),
            right,
        ),
    }
}

fn draw_shaders(f: &mut Frame, area: Rect, app: &App) {
    let [left, right] = halves(area);
    let items: Vec<ListItem> = app
        .shader_rows
        .iter()
        .map(|r| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    if r.enabled { "[x] " } else { "[ ] " },
                    Style::default().fg(if r.enabled { ACCENT } else { Color::Reset }),
                ),
                Span::raw(r.name.clone()),
                Span::styled(if r.in_library { "" } else { "  (this profile)" }, dim()),
            ]))
        })
        .collect();
    let mut st = ListState::default().with_selected((!app.shader_rows.is_empty()).then_some(app.shader_sel));
    f.render_stateful_widget(List::new(items).block(list_block("Shaders")).highlight_style(highlight()), left, &mut st);

    let anim = app.profile.as_ref().and_then(|p| p.get("custom-shader-animation")).unwrap_or_else(|| "unset".into());
    let text = vec![
        Line::from("Enter / space toggles the shader for this profile."),
        Line::from("a toggles animation (custom-shader-animation)."),
        Line::raw(""),
        Line::from(format!("animation: {anim}")),
        Line::raw(""),
        Line::styled("Shaders are copied into the profile folder, so exports stay self-contained.", dim()),
        Line::styled(
            "Shader compile errors are not shown by Ghostty: if the window looks unchanged, the shader may have failed.",
            dim(),
        ),
    ];
    f.render_widget(Paragraph::new(text).block(list_block("About")).wrap(Wrap { trim: true }), right);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let color = if app.status.ok { Color::Green } else { Color::Red };
    let help = match app.tab {
        Tab::Profiles => {
            "Enter apply · n new profile · r rename · d delete · e export · u off · p presets · ctrl+r reload · q quit"
        }
        Tab::Themes => "↑↓ select · / filter · Enter bake into profile · Esc clear · Tab next · q quit",
        Tab::Edit => {
            "↑↓ select · Enter edit · p color picker (or click a swatch) · ←→ cycle · x unset · ctrl+r reload · q quit"
        }
        Tab::Shaders => "↑↓ select · Enter toggle · a animation · Tab next · q quit",
    };
    let text = vec![Line::styled(app.status.text.clone(), Style::default().fg(color)), Line::styled(help, dim())];
    f.render_widget(Paragraph::new(text).block(Block::default().borders(Borders::TOP)), area);
}

fn draw_input(f: &mut Frame, app: &App) {
    let Some(input) = &app.input else { return };
    let area = f.area();
    let w = area.width.saturating_sub(4).min(72);
    let h = 5.min(area.height);
    let r = Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + area.height.saturating_sub(h + 4),
        width: w,
        height: h,
    };
    f.render_widget(Clear, r);
    let mut lines =
        vec![Line::from(vec![Span::raw(input.buf.clone()), Span::styled("▏", Style::default().fg(ACCENT))])];
    match &input.error {
        Some(e) => lines.push(Line::styled(e.clone(), Style::default().fg(Color::Red))),
        None => lines.push(Line::styled("Enter to confirm · Esc to cancel", dim())),
    }
    f.render_widget(
        Paragraph::new(lines).alignment(Alignment::Left).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", input.title))
                .border_style(Style::default().fg(ACCENT)),
        ),
        r,
    );
}

fn draw_wizard(f: &mut Frame, app: &App) {
    let Some(w) = &app.wizard else { return };
    let area = f.area();
    let width = area.width.saturating_sub(4).min(78);
    let (title, lines) = match w.step {
        WizardStep::Base(sel) => {
            let copy = match &w.copy_from {
                Some(n) => format!("a copy of '{n}'"),
                None => "a copy of the selected profile (none selected)".to_string(),
            };
            let opts = ["1  your current Ghostty setup", &format!("2  {copy}"), "3  blank"];
            let mut lines: Vec<Line> = opts
                .iter()
                .enumerate()
                .map(|(i, o)| {
                    let style = if i == sel {
                        highlight()
                    } else if i == 1 && w.copy_from.is_none() {
                        dim()
                    } else {
                        Style::default()
                    };
                    Line::styled(format!(" {o} "), style)
                })
                .collect();
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                match sel {
                    0 => "Moves your look out of your Ghostty config into the profile (backed up).",
                    1 => "Starts with the same settings as that profile.",
                    _ => "Starts empty; fill it in on the Edit tab.",
                },
                dim(),
            ));
            lines.push(Line::styled("1-3 or Enter choose · ↑↓ move · Esc cancel · the profile is NOT applied", dim()));
            (format!(" New profile '{}': start from ", w.name), lines)
        }
        WizardStep::ConfirmAdopt => (
            format!(" Use your current Ghostty setup for '{}'? ", w.name),
            vec![
                Line::from("This MOVES your appearance settings (colors, fonts, cursor, opacity,"),
                Line::from("shaders, background) out of your Ghostty config into the new profile."),
                Line::from("Keybinds and other settings stay. Backups: *.bak-pre-ghostty-profiles"),
                Line::raw(""),
                Line::styled(
                    "Until you apply it (Enter on the profile), a reload shows defaults.",
                    Style::default().fg(Color::Yellow),
                ),
                Line::raw(""),
                Line::styled("y  confirm · Esc cancel · any other key goes back", dim()),
            ],
        ),
    };
    let h = (lines.len() as u16 + 2).min(area.height);
    let r = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width,
        height: h,
    };
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(title).border_style(Style::default().fg(ACCENT))),
        r,
    );
}

// ---- the color picker ------------------------------------------------------------------

fn hsv_color(h: f32, s: f32, v: f32) -> Color {
    let (r, g, b) = color::hsv_to_rgb(Hsv { h, s, v });
    color::term_color(r, g, b)
}

fn luminance(h: f32, s: f32, v: f32) -> f32 {
    let (r, g, b) = color::hsv_to_rgb(Hsv { h, s, v });
    (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0
}

/// A horizontal bar of `width` cells (`height` rows) whose color at fraction t is `at(t)`, with a marker
/// at `marker`.
fn draw_bar(f: &mut Frame, r: Rect, marker: f32, at: impl Fn(f32) -> (Color, f32)) {
    let mark_col = color::bar_cell(marker, r.width);
    for dx in 0..r.width {
        let (c, lum) = at(color::bar_fraction(dx as i32, r.width));
        for dy in 0..r.height {
            let cell = &mut f.buffer_mut()[(r.x + dx, r.y + dy)];
            cell.set_char(if dx == mark_col { '┃' } else { ' ' }).set_bg(c).set_fg(if lum > 0.5 {
                Color::Black
            } else {
                Color::White
            });
        }
    }
}

fn draw_picker(f: &mut Frame, app: &App) {
    let (Some(picker), Some(input)) = (&app.picker, &app.input) else { return };
    let hsv = picker.hsv;
    // keep clear of the input box, which is drawn at the bottom
    let full = f.area();
    let area = Rect { height: full.height.saturating_sub(6), ..full };
    if area.width < 12 || area.height < 4 {
        return;
    }
    let width = area.width.saturating_sub(2).min(64);
    let height = area.height.saturating_sub(1).min(21);
    let r = Rect { x: area.x + (area.width - width) / 2, y: area.y + (area.height - height) / 2, width, height };
    f.render_widget(Clear, r);
    let block =
        Block::default().borders(Borders::ALL).title(" Color picker ").border_style(Style::default().fg(ACCENT));
    let inner = block.inner(r);
    f.render_widget(block, r);

    let hex = color::hsv_to_hex(hsv);
    let (cr, cg, cb) = color::hsv_to_rgb(hsv);
    let current = color::term_color(cr, cg, cb);

    // wheel mode needs the wheel (2:1 cells), a gap, a 12-wide side panel, and 4 rows under the wheel
    let wheel_h = inner.height.saturating_sub(4).min(14).min(inner.width.saturating_sub(15) / 2);
    if wheel_h >= 5 {
        let wheel = Rect { x: inner.x, y: inner.y, width: wheel_h * 2, height: wheel_h };
        let geom = WheelGeom { cols: wheel.width, rows: wheel.height };
        for row in 0..wheel.height {
            for col in 0..wheel.width {
                let (x, y) = (col as f32 + 0.5, row as f32 * 2.0);
                let top = geom.pixel_hs(x, y + 0.5).map(|(h, s)| hsv_color(h, s, 1.0));
                let bottom = geom.pixel_hs(x, y + 1.5).map(|(h, s)| hsv_color(h, s, 1.0));
                let cell = &mut f.buffer_mut()[(wheel.x + col, wheel.y + row)];
                match (top, bottom) {
                    (Some(t), Some(b)) => {
                        cell.set_char('▀').set_fg(t).set_bg(b);
                    }
                    (Some(t), None) => {
                        cell.set_char('▀').set_fg(t);
                    }
                    (None, Some(b)) => {
                        cell.set_char('▄').set_fg(b);
                    }
                    (None, None) => {}
                }
            }
        }
        // the crosshair at the current hue/saturation
        let (_, _, mc, mr) = geom.marker(hsv.h, hsv.s);
        let under = hsv_color(hsv.h, hsv.s, 1.0);
        let ink = if luminance(hsv.h, hsv.s, 1.0) > 0.5 { Color::Black } else { Color::White };
        f.buffer_mut()[(wheel.x + mc, wheel.y + mr)]
            .set_char('+')
            .set_fg(ink)
            .set_bg(under)
            .set_style(Style::default().add_modifier(Modifier::BOLD));

        // brightness bar under the wheel
        let label = Rect { x: inner.x, y: wheel.y + wheel.height, width: wheel.width, height: 1 };
        f.render_widget(
            Paragraph::new(Line::styled(format!("brightness {:>3}%", (hsv.v * 100.0).round() as u32), dim())),
            label,
        );
        let bar = Rect {
            x: inner.x,
            y: label.y + 1,
            width: wheel.width,
            height: 2.min(inner.height.saturating_sub(wheel.height + 1)),
        };
        draw_bar(f, bar, hsv.v, |t| (hsv_color(hsv.h, hsv.s, t), luminance(hsv.h, hsv.s, t)));
        *app.picker_rects.borrow_mut() = super::app::PickerRects { wheel: Some(wheel), bars: vec![(Comp::Val, bar)] };

        // side panel: live preview, the hex and the keys
        let px = wheel.x + wheel.width + 2;
        let pw = (inner.x + inner.width).saturating_sub(px);
        if pw >= 10 {
            let sw = Rect { x: px, y: inner.y, width: pw.min(14), height: 3.min(inner.height) };
            for dy in 0..sw.height {
                for dx in 0..sw.width {
                    f.buffer_mut()[(sw.x + dx, sw.y + dy)].set_char(' ').set_bg(current);
                }
            }
            let text = vec![
                Line::from(vec![Span::styled(hex.clone(), Style::default().add_modifier(Modifier::BOLD))]),
                Line::styled(
                    format!("H {:>3}°  S {:>3}%", hsv.h.round() as u32 % 360, (hsv.s * 100.0).round() as u32),
                    dim(),
                ),
                Line::raw(""),
                Line::styled("click or drag the wheel", dim()),
                Line::styled("and the brightness bar", dim()),
                Line::raw(""),
                Line::styled("arrows: hue / saturation", dim()),
                Line::styled("[ ]  + -: brightness", dim()),
                Line::styled("Shift: bigger steps", dim()),
                Line::raw(""),
                Line::styled("Enter accept · Esc cancel", dim()),
            ];
            let tr =
                Rect { x: px, y: sw.y + sw.height + 1, width: pw, height: inner.height.saturating_sub(sw.height + 1) };
            f.render_widget(Paragraph::new(text), tr);
        }
        return;
    }

    // small terminal: three sliders instead of the wheel
    if inner.height >= 5 && inner.width >= 14 {
        let mut bars = Vec::new();
        let head = Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  ", Style::default().bg(current)),
                Span::styled(format!(" {hex}  "), Style::default().add_modifier(Modifier::BOLD)),
                Span::styled("Enter accept · Esc cancel", dim()),
            ])),
            head,
        );
        let rows = [(Comp::Hue, "H", hsv.h / 360.0), (Comp::Sat, "S", hsv.s), (Comp::Val, "V", hsv.v)];
        for (i, (comp, label, frac)) in rows.into_iter().enumerate() {
            let y = inner.y + 1 + i as u16;
            f.render_widget(Paragraph::new(Span::styled(label, dim())), Rect { x: inner.x, y, width: 2, height: 1 });
            let bar = Rect { x: inner.x + 2, y, width: inner.width.saturating_sub(2), height: 1 };
            draw_bar(f, bar, frac, |t| match comp {
                Comp::Hue => (hsv_color(t * 360.0, 1.0, 1.0), luminance(t * 360.0, 1.0, 1.0)),
                Comp::Sat => (hsv_color(hsv.h, t, hsv.v.max(0.4)), luminance(hsv.h, t, hsv.v.max(0.4))),
                Comp::Val => (hsv_color(hsv.h, hsv.s, t), luminance(hsv.h, hsv.s, t)),
            });
            bars.push((comp, bar));
        }
        f.render_widget(
            Paragraph::new(Line::styled("arrows hue/sat · [ ] brightness", dim())),
            Rect { x: inner.x, y: inner.y + 4, width: inner.width, height: 1 },
        );
        *app.picker_rects.borrow_mut() = super::app::PickerRects { wheel: None, bars };
        return;
    }

    // too small for either: the hex box still works
    f.render_widget(Paragraph::new(Line::styled(format!("{hex}  (type a hex, Esc cancels)"), dim())), inner);
    let _ = input;
}
