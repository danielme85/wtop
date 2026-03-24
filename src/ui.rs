use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, BorderType, Borders, Cell, Chart, Clear, Dataset, GraphType, Paragraph, Row,
    Sparkline, Table, Wrap,
};
use ratatui::Frame;

use crate::aggregation;
use crate::app::{App, Page};
use crate::settings::{BarStyle, GraphStyle, SortBy};
use crate::theme::Theme;

/// Draw the entire TUI frame (stateless, immediate-mode).
pub fn draw(frame: &mut Frame, app: &mut App) {
    let theme = app.active_theme();

    let bg_block = Block::default().style(Style::default().bg(theme.bg));
    frame.render_widget(bg_block, frame.area());

    let chunks =
        Layout::vertical([Constraint::Length(3), Constraint::Min(0), Constraint::Length(1)])
            .split(frame.area());

    draw_header(frame, app, &theme, chunks[0]);

    match app.page {
        Page::List => draw_container_list(frame, app, &theme, chunks[1]),
        Page::Detail => draw_detail(frame, app, &theme, chunks[1]),
        Page::Resources => draw_resources(frame, app, &theme, chunks[1]),
        Page::Logs => draw_logs(frame, app, &theme, chunks[1]),
        Page::Settings => draw_settings(frame, app, &theme, chunks[1]),
    }

    draw_footer(frame, app, &theme, chunks[2]);

    // Action menu popup (rendered last so it's on top)
    if app.action_menu.is_some() {
        draw_action_menu(frame, app, &theme);
    }

    // Info popup (rendered on top of everything)
    if app.info_popup.is_some() {
        draw_info_popup(frame, app, &theme);
    }
}

fn content_block<'a>(title: &str, theme: &Theme) -> Block<'a> {
    Block::default()
        .title(Span::styled(
            format!(" {} ", title),
            Style::default().fg(theme.title),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.bg))
        .padding(ratatui::widgets::Padding::new(1, 1, 0, 0))
}

fn spark_block<'a>(title: &str, theme: &Theme) -> Block<'a> {
    Block::default()
        .title(Span::styled(
            format!(" {} ", title),
            Style::default().fg(theme.title),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.bg))
}

fn draw_header(frame: &mut Frame, app: &App, theme: &Theme, area: ratatui::layout::Rect) {
    let total = app.containers.len();
    let running = app
        .containers
        .iter()
        .filter(|c| c.status.contains("Up"))
        .count();
    let stopped = total - running;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let cols = Layout::horizontal([Constraint::Min(0), Constraint::Length(40)]).split(inner);

    let title = Paragraph::new(Line::from(vec![Span::styled(
        format!(" 🐋 WhaleTop v{} ", env!("CARGO_PKG_VERSION")),
        Style::default()
            .fg(theme.title)
            .add_modifier(Modifier::BOLD),
    )]));
    frame.render_widget(title, cols[0]);

    let status = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("{} total  ", total),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("▲ {} running  ", running),
            Style::default()
                .fg(theme.running)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("▼ {} stopped ", stopped),
            Style::default()
                .fg(theme.stopped)
                .add_modifier(Modifier::BOLD),
        ),
    ]))
    .alignment(Alignment::Right);
    frame.render_widget(status, cols[1]);
}

fn draw_footer(frame: &mut Frame, app: &App, theme: &Theme, area: ratatui::layout::Rect) {
    // Show status message if recent (< 3 seconds)
    if let Some((ref msg, ref instant)) = app.status_message {
        if instant.elapsed() < std::time::Duration::from_secs(3) {
            let footer = Paragraph::new(Line::from(Span::styled(
                format!(" {} ", msg),
                Style::default().fg(theme.title),
            )))
            .style(Style::default().bg(theme.bg));
            frame.render_widget(footer, area);
            return;
        }
    }

    let help_text = match app.page {
        Page::List => {
            if app.containers.is_empty() {
                "s: Settings  q: Quit".to_string()
            } else {
                format!(
                    "Left/Right: Navigate  Up/Down: Select  Enter: Actions  Tab: Sort [{}]  s: Settings  q: Quit",
                    app.settings.sort_by.label()
                )
            }
        }
        Page::Detail => {
            "Left/Right: Navigate  Up/Down: Scroll  Enter: Actions  PgUp/PgDn: Container  s: Settings  q: Quit".to_string()
        }
        Page::Resources => {
            "Left/Right: Navigate  Enter: Actions  PgUp/PgDn: Container  s: Settings  q: Quit".to_string()
        }
        Page::Logs => {
            let scroll_indicator = if app.auto_scroll { "ON" } else { "OFF" };
            format!(
                "Left/Right: Navigate  Up/Down: Scroll  Enter: Actions  PgUp/PgDn: Container  a: Auto-scroll [{}]  s: Settings  q: Quit",
                scroll_indicator
            )
        }
        Page::Settings => {
            if app.settings_editing {
                "Left/Right: Change Value  Enter/Esc: Done".to_string()
            } else {
                "Arrows: Navigate  Enter: Edit  Esc/s: Back  q: Quit".to_string()
            }
        }
    };

    let footer = Paragraph::new(Line::from(Span::styled(
        format!("{} ", help_text),
        Style::default().fg(theme.text),
    )))
    .style(Style::default().bg(theme.bg))
    .alignment(Alignment::Right);
    frame.render_widget(footer, area);
}

fn draw_container_list(frame: &mut Frame, app: &App, theme: &Theme, area: ratatui::layout::Rect) {
    let cols = &app.settings.columns;
    let header_style = Style::default()
        .fg(theme.title)
        .add_modifier(Modifier::BOLD);

    // Build dynamic headers and constraints based on column visibility
    let mut headers: Vec<Cell> = Vec::new();
    let mut constraints: Vec<Constraint> = Vec::new();

    // Status indicator column (always present)
    headers.push(Cell::from("").style(header_style));
    constraints.push(Constraint::Length(2));

    // Fixed-width columns
    if cols.id {
        headers.push(Cell::from("ID").style(header_style));
        constraints.push(Constraint::Length(14));
    }

    // Count flexible columns to split percentage evenly
    let flex_count = [cols.name, cols.image, cols.status]
        .iter()
        .filter(|&&v| v)
        .count();
    let flex_pct = if flex_count > 0 {
        // Leave room for fixed-width activity columns
        let activity_cols = [cols.cpu, cols.mem, cols.disk, cols.network]
            .iter()
            .filter(|&&v| v)
            .count() as u16;
        let reserved = activity_cols * 10; // ~10 chars each
        // Use percentage of remaining space
        let avail = 100u16.saturating_sub(if cols.id { 10 } else { 0 }).saturating_sub(reserved);
        avail / flex_count as u16
    } else {
        30
    };

    if cols.name {
        headers.push(Cell::from("Name").style(header_style));
        constraints.push(Constraint::Percentage(flex_pct));
    }
    if cols.image {
        headers.push(Cell::from("Image").style(header_style));
        constraints.push(Constraint::Percentage(flex_pct));
    }
    if cols.status {
        headers.push(Cell::from("Status").style(header_style));
        constraints.push(Constraint::Percentage(flex_pct));
    }

    // Activity columns (fixed width, wider when bars are enabled)
    if cols.cpu {
        headers.push(Cell::from("CPU").style(header_style));
        let w = if app.settings.show_cpu_bar { 18 } else { 8 };
        constraints.push(Constraint::Length(w));
    }
    if cols.mem {
        headers.push(Cell::from("MEM").style(header_style));
        let w = if app.settings.show_mem_bar { 18 } else { 8 };
        constraints.push(Constraint::Length(w));
    }
    if cols.disk {
        headers.push(Cell::from("Disk").style(header_style));
        let w = if app.settings.show_disk_bar { 20 } else { 10 };
        constraints.push(Constraint::Length(w));
    }
    if cols.network {
        headers.push(Cell::from("Net").style(header_style));
        let w = if app.settings.show_network_bar { 20 } else { 10 };
        constraints.push(Constraint::Length(w));
    }

    let header_row = Row::new(headers).height(1);
    let col_count = constraints.len();

    let mut rows: Vec<Row> = Vec::new();
    let mut last_project: Option<Option<&str>> = None;

    for (i, c) in app.containers.iter().enumerate() {
        // Insert compose project group header when sorting by compose project
        if app.settings.sort_by == SortBy::ComposeProject {
            let current_project = c.compose_project.as_deref();
            let show_header = match last_project {
                None => true,
                Some(ref prev) => *prev != current_project,
            };
            if show_header {
                let label = current_project.unwrap_or("(no project)");
                let sep_style = Style::default().fg(theme.title).add_modifier(Modifier::BOLD);
                let mut header_cells: Vec<Cell> = Vec::with_capacity(col_count);
                // Fill each column: put "──" in the narrow status col,
                // and the project name in the first flexible column.
                let mut label_placed = false;
                // Status indicator column
                header_cells.push(Cell::from("──").style(sep_style));
                // Remaining columns
                for _ in 1..(col_count) {
                    if !label_placed {
                        header_cells.push(Cell::from(format!("── {} ──", label)).style(sep_style));
                        label_placed = true;
                    } else {
                        header_cells.push(Cell::from(""));
                    }
                }
                rows.push(Row::new(header_cells));
                last_project = Some(current_project);
            }
        }

        {
            let is_selected = i == app.selected;
            let status_style = if c.status.contains("Up") {
                Style::default().fg(theme.running)
            } else {
                Style::default().fg(theme.text)
            };

            let base_style = if is_selected {
                Style::default().fg(Color::White).bg(theme.border)
            } else {
                Style::default().fg(theme.text)
            };

            let status_style = if is_selected {
                status_style.bg(theme.border)
            } else {
                status_style
            };

            let stats = app.all_stats.get(&c.id);

            // Status indicator icon
            let (icon, icon_color) = if c.status.contains("Paused") {
                ("⏸", theme.title)
            } else if c.status.contains("Up") {
                ("▶", theme.running)
            } else {
                ("■", theme.stopped)
            };
            let icon_style = if is_selected {
                Style::default().fg(icon_color).bg(theme.border)
            } else {
                Style::default().fg(icon_color)
            };

            let mut cells: Vec<Cell> = Vec::new();
            cells.push(Cell::from(icon).style(icon_style));
            if cols.id {
                cells.push(Cell::from(c.id.as_str()).style(base_style));
            }
            if cols.name {
                cells.push(Cell::from(c.name.as_str()).style(base_style));
            }
            if cols.image {
                cells.push(Cell::from(c.image.as_str()).style(base_style));
            }
            if cols.status {
                cells.push(Cell::from(c.status.as_str()).style(status_style));
            }
            if cols.cpu {
                let pct = stats.and_then(|s| s.cpu_percent);
                let val = pct
                    .map(|p| format!("{:.1}%", p))
                    .unwrap_or_else(|| "-".to_string());
                let style = if is_selected {
                    Style::default().fg(theme.running).bg(theme.border)
                } else {
                    Style::default().fg(theme.running)
                };
                if app.settings.show_cpu_bar {
                    let bar = mini_bar_width(pct.unwrap_or(0.0), theme.running, 6, app.settings.bar_style);
                    cells.push(Cell::from(Line::from(vec![
                        bar,
                        Span::styled(format!(" {}", val), style),
                    ])));
                } else {
                    cells.push(Cell::from(val).style(style));
                }
            }
            if cols.mem {
                let pct = stats.and_then(|s| s.mem_percent);
                let val = pct
                    .map(|p| format!("{:.1}%", p))
                    .unwrap_or_else(|| "-".to_string());
                let style = if is_selected {
                    Style::default().fg(theme.cyan).bg(theme.border)
                } else {
                    Style::default().fg(theme.cyan)
                };
                if app.settings.show_mem_bar {
                    let bar = mini_bar_width(pct.unwrap_or(0.0), theme.cyan, 6, app.settings.bar_style);
                    cells.push(Cell::from(Line::from(vec![
                        bar,
                        Span::styled(format!(" {}", val), style),
                    ])));
                } else {
                    cells.push(Cell::from(val).style(style));
                }
            }
            if cols.disk {
                let val = stats
                    .and_then(|s| s.block_read)
                    .map(format_bytes_short)
                    .unwrap_or_else(|| "-".to_string());
                let style = if is_selected {
                    Style::default().fg(theme.purple).bg(theme.border)
                } else {
                    Style::default().fg(theme.purple)
                };
                if app.settings.show_disk_bar {
                    // Use mem_percent-style bar based on block usage (no natural max, so show value only with placeholder bar)
                    let pct = stats.and_then(|s| s.block_read).map(|b| (b as f64 / (1024.0 * 1024.0 * 1024.0) * 100.0).min(100.0)).unwrap_or(0.0);
                    let bar = mini_bar_width(pct, theme.purple, 6, app.settings.bar_style);
                    cells.push(Cell::from(Line::from(vec![
                        bar,
                        Span::styled(format!(" {}", val), style),
                    ])));
                } else {
                    cells.push(Cell::from(val).style(style));
                }
            }
            if cols.network {
                let val = stats
                    .and_then(|s| s.net_rx)
                    .map(format_bytes_short)
                    .unwrap_or_else(|| "-".to_string());
                let style = if is_selected {
                    Style::default().fg(theme.cyan).bg(theme.border)
                } else {
                    Style::default().fg(theme.cyan)
                };
                if app.settings.show_network_bar {
                    let pct = stats.and_then(|s| s.net_rx).map(|b| (b as f64 / (1024.0 * 1024.0 * 1024.0) * 100.0).min(100.0)).unwrap_or(0.0);
                    let bar = mini_bar_width(pct, theme.cyan, 6, app.settings.bar_style);
                    cells.push(Cell::from(Line::from(vec![
                        bar,
                        Span::styled(format!(" {}", val), style),
                    ])));
                } else {
                    cells.push(Cell::from(val).style(style));
                }
            }

            rows.push(Row::new(cells));
        }
    }

    let table = Table::new(rows, constraints)
        .header(header_row)
        .block(content_block("Containers", theme));

    frame.render_widget(table, area);
}

fn detail_line(label: &str, value: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            label.to_string(),
            Style::default()
                .fg(theme.title)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.to_string(), Style::default().fg(theme.text)),
    ])
}

fn section_header(title: &str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(theme.border)
            .add_modifier(Modifier::BOLD),
    ))
}

fn draw_detail(frame: &mut Frame, app: &mut App, theme: &Theme, area: ratatui::layout::Rect) {
    let container_name = app
        .containers
        .get(app.selected)
        .map(|c| c.name.as_str())
        .unwrap_or("unknown");

    let title = format!("Detail: {}", container_name);

    let mut lines: Vec<Line> = Vec::new();

    if let Some(ref d) = app.detail {
        // General
        lines.push(section_header("── General ──", theme));
        lines.push(detail_line("  ID:       ", &d.full_id, theme));
        lines.push(detail_line("  Image:    ", &d.image, theme));
        lines.push(detail_line("  Command:  ", &d.command, theme));
        lines.push(detail_line("  Created:  ", &d.created, theme));
        lines.push(detail_line("  State:    ", &d.state, theme));
        lines.push(Line::default());

        // Compose
        if let Some(ref compose) = d.compose {
            lines.push(section_header("── Compose ──", theme));
            lines.push(detail_line("  Project:  ", &compose.project, theme));
            lines.push(detail_line("  Service:  ", &compose.service, theme));
            if let Some(ref dir) = compose.working_dir {
                lines.push(detail_line("  Dir:      ", dir, theme));
            }
            if let Some(ref files) = compose.config_files {
                lines.push(detail_line("  Config:   ", files, theme));
            }
            lines.push(Line::default());
        }

        // Network
        lines.push(section_header("── Network ──", theme));
        if d.networks.is_empty() {
            lines.push(Line::from(Span::styled(
                "  (none)".to_string(),
                Style::default().fg(theme.dim),
            )));
        } else {
            for n in &d.networks {
                lines.push(detail_line("  ", n, theme));
            }
        }
        lines.push(Line::default());

        // Ports
        lines.push(section_header("── Ports ──", theme));
        if d.ports.is_empty() {
            lines.push(Line::from(Span::styled(
                "  (none)".to_string(),
                Style::default().fg(theme.dim),
            )));
        } else {
            for p in &d.ports {
                lines.push(detail_line("  ", p, theme));
            }
        }
        lines.push(Line::default());

        // Volumes
        lines.push(section_header("── Volumes ──", theme));
        if d.volumes.is_empty() {
            lines.push(Line::from(Span::styled(
                "  (none)".to_string(),
                Style::default().fg(theme.dim),
            )));
        } else {
            for v in &d.volumes {
                lines.push(detail_line("  ", v, theme));
            }
        }
        lines.push(Line::default());

        // Environment
        lines.push(section_header("── Environment ──", theme));
        if d.env.is_empty() {
            lines.push(Line::from(Span::styled(
                "  (none)".to_string(),
                Style::default().fg(theme.dim),
            )));
        } else {
            for e in &d.env {
                if let Some((name, value)) = e.split_once('=') {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("  {}=", name),
                            Style::default()
                                .fg(theme.title)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(value.to_string(), Style::default().fg(theme.text)),
                    ]));
                } else {
                    lines.push(detail_line("  ", e, theme));
                }
            }
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Loading...".to_string(),
            Style::default().fg(theme.dim),
        )));
    }

    let paragraph = Paragraph::new(lines)
        .block(content_block(&title, theme))
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));

    frame.render_widget(paragraph, area);
}

/// Right-align sparkline data by prepending zeros so the graph is flush-right.
fn right_align_data(data: &[u64], area_width: u16) -> Vec<u64> {
    let inner_width = area_width.saturating_sub(2) as usize; // subtract borders
    if data.len() >= inner_width {
        // Take only the most recent points that fit
        data[data.len() - inner_width..].to_vec()
    } else {
        let mut padded = vec![0u64; inner_width - data.len()];
        padded.extend_from_slice(data);
        padded
    }
}

/// Render a mini inline bar for a 0–100 percentage using the given style.
fn mini_bar(percent: f64, fill_color: Color, style: BarStyle) -> Span<'static> {
    mini_bar_width(percent, fill_color, 10, style)
}

/// Render a mini inline bar with configurable width.
fn mini_bar_width(
    percent: f64,
    fill_color: Color,
    width: usize,
    style: BarStyle,
) -> Span<'static> {
    let clamped = percent.clamp(0.0, 100.0);
    let filled = ((clamped / 100.0) * width as f64).round() as usize;
    let empty = width - filled;

    match style {
        BarStyle::Block => {
            let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
            Span::styled(bar, Style::default().fg(fill_color))
        }
        BarStyle::Smooth => {
            const FRACS: &[&str] = &[" ", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
            let total_eighths = ((clamped / 100.0) * (width * 8) as f64).round() as usize;
            let full = total_eighths / 8;
            let remainder = total_eighths % 8;
            let trail = width - full - if remainder > 0 { 1 } else { 0 };
            let frac = if remainder > 0 { FRACS[remainder] } else { "" };
            let bar = format!("{}{}{}", "█".repeat(full), frac, " ".repeat(trail));
            Span::styled(bar, Style::default().fg(fill_color))
        }
        BarStyle::Gradient => {
            let mut bar = String::with_capacity(width * 4);
            for i in 0..width {
                if i < filled.saturating_sub(2) {
                    bar.push('█');
                } else if i == filled.saturating_sub(2) && filled >= 2 {
                    bar.push('▓');
                } else if i == filled.saturating_sub(1) && filled >= 1 {
                    bar.push('▒');
                } else {
                    bar.push(' ');
                }
            }
            Span::styled(bar, Style::default().fg(fill_color))
        }
        BarStyle::Dot => {
            let bar = format!("{}{}", "●".repeat(filled), "○".repeat(empty));
            Span::styled(bar, Style::default().fg(fill_color))
        }
        BarStyle::Pipe => {
            let bar = format!("{}{}", "|".repeat(filled), ".".repeat(empty));
            Span::styled(bar, Style::default().fg(fill_color))
        }
        BarStyle::Arrow => {
            // [=========> ----] style — needs at least 3 chars to show head
            let inner = width.saturating_sub(2);
            let fill = ((clamped / 100.0) * inner as f64).round() as usize;
            let head = if fill > 0 { ">" } else { "-" };
            let eq = fill.saturating_sub(1);
            let dashes = inner - fill;
            let bar = format!("[{}{}{}]", "=".repeat(eq), head, "-".repeat(dashes));
            Span::styled(bar, Style::default().fg(fill_color))
        }
        BarStyle::Dashed => {
            let bar = format!("{}{}", ":".repeat(filled), ".".repeat(empty));
            Span::styled(bar, Style::default().fg(fill_color))
        }
        BarStyle::Classic => {
            let bar = format!("[{}{}]", "#".repeat(filled), "-".repeat(empty));
            Span::styled(bar, Style::default().fg(fill_color))
        }
    }
}

fn sparkline_bar_set(style: GraphStyle) -> symbols::bar::Set {
    match style {
        GraphStyle::Smooth => symbols::bar::NINE_LEVELS,
        GraphStyle::Chunky => symbols::bar::THREE_LEVELS,
        GraphStyle::Braille => symbols::bar::Set {
            full:            "⣿",
            seven_eighths:   "⣷",
            three_quarters:  "⣶",
            five_eighths:    "⣤",
            half:            "⣤",
            three_eighths:   "⣀",
            one_quarter:     "⣀",
            one_eighth:      "⡀",
            empty:           " ",
        },
        GraphStyle::Shade => symbols::bar::Set {
            full:            "█",
            seven_eighths:   "█",
            three_quarters:  "▓",
            five_eighths:    "▓",
            half:            "▒",
            three_eighths:   "▒",
            one_quarter:     "░",
            one_eighth:      "░",
            empty:           " ",
        },
        // Line/Area use the Chart widget — this path is never reached for those styles
        GraphStyle::Line | GraphStyle::Area => symbols::bar::NINE_LEVELS,
    }
}

/// Render a graph widget (Sparkline or Chart) into `area`.
///
/// Line/Area styles use ratatui's `Chart` widget with a connected line.
/// All other styles use `Sparkline` with the appropriate bar set.
fn render_graph(
    frame: &mut Frame,
    data: &[u64],
    y_max: u64,
    color: Color,
    graph_style: GraphStyle,
    title: &str,
    theme: &Theme,
    area: ratatui::layout::Rect,
) {
    if matches!(graph_style, GraphStyle::Line | GraphStyle::Area) {
        let xy: Vec<(f64, f64)> = data
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as f64, v as f64))
            .collect();
        let x_max = (xy.len() as f64 - 1.0).max(1.0);
        let y_bound = y_max as f64;

        let mut datasets = Vec::new();
        if matches!(graph_style, GraphStyle::Area) {
            datasets.push(
                Dataset::default()
                    .graph_type(GraphType::Bar)
                    .style(Style::default().fg(color).add_modifier(Modifier::DIM))
                    .data(&xy),
            );
        }
        datasets.push(
            Dataset::default()
                .graph_type(GraphType::Line)
                .style(Style::default().fg(color).add_modifier(Modifier::BOLD))
                .data(&xy),
        );

        let chart = Chart::new(datasets)
            .block(spark_block(title, theme))
            .x_axis(
                Axis::default()
                    .bounds([0.0, x_max])
                    .style(Style::default().fg(theme.border)),
            )
            .y_axis(
                Axis::default()
                    .bounds([0.0, y_bound])
                    .style(Style::default().fg(theme.border)),
            );
        frame.render_widget(chart, area);
    } else {
        let spark = Sparkline::default()
            .block(spark_block(title, theme))
            .data(data)
            .max(y_max)
            .style(Style::default().fg(color))
            .bar_set(sparkline_bar_set(graph_style));
        frame.render_widget(spark, area);
    }
}

fn format_bytes_short(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn draw_resources(frame: &mut Frame, app: &mut App, theme: &Theme, area: ratatui::layout::Rect) {
    let container_name = app
        .containers
        .get(app.selected)
        .map(|c| c.name.as_str())
        .unwrap_or("unknown");

    let title = format!("Resources: {}", container_name);

    // Outer block with title (no horizontal padding so sparklines span full width)
    let outer = spark_block(&title, theme);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    if app.stats.is_none() {
        let msg = Paragraph::new(Span::styled(
            "Waiting for stats...",
            Style::default().fg(theme.dim),
        ));
        frame.render_widget(msg, inner);
        return;
    }

    let stats = app.stats.as_ref().unwrap();

    // Layout: summary line + 3 graph rows (CPU, Memory, I/O)
    let rows = Layout::vertical([
        Constraint::Length(3), // summary
        Constraint::Min(4),   // CPU graph
        Constraint::Min(4),   // Memory graph
        Constraint::Min(4),   // Disk I/O graph
        Constraint::Min(4),   // Network I/O graph
    ])
    .split(inner);

    // ── Summary line ──
    let num_cpus = stats.num_cpus.unwrap_or(1);
    let cpu_host_str = stats
        .cpu_percent
        .map(|c| format!("{:.1}%", c))
        .unwrap_or_else(|| "n/a".to_string());
    let cpu_per_core_str = stats
        .cpu_percent
        .map(|c| format!("{:.1}%", c / num_cpus as f64))
        .unwrap_or_else(|| "n/a".to_string());
    let mem_str = stats
        .mem_usage
        .as_deref()
        .unwrap_or("n/a");
    let mem_pct_str = stats
        .mem_percent
        .map(|p| format!("{:.1}%", p))
        .unwrap_or_else(|| "n/a".to_string());
    let pids_str = stats
        .pids
        .map(|p| p.to_string())
        .unwrap_or_else(|| "n/a".to_string());
    let blk_r = stats.block_read.map(format_bytes_short).unwrap_or_else(|| "n/a".to_string());
    let blk_w = stats.block_write.map(format_bytes_short).unwrap_or_else(|| "n/a".to_string());
    let host_net = app.detail.as_ref().map(|d| d.host_network).unwrap_or(false);
    let na_net = if host_net { "N/A (host)".to_string() } else { "N/A".to_string() };
    let net_r = stats.net_rx.map(format_bytes_short).unwrap_or_else(|| na_net.clone());
    let net_t = stats.net_tx.map(format_bytes_short).unwrap_or_else(|| na_net.clone());

    // Split summary into columns so data spreads across the full width
    let summary_cols = Layout::horizontal([
        Constraint::Ratio(1, 5),
        Constraint::Ratio(1, 5),
        Constraint::Ratio(1, 5),
        Constraint::Ratio(1, 5),
        Constraint::Ratio(1, 5),
    ])
    .split(rows[0]);

    // CPU
    let cpu_bar = mini_bar(stats.cpu_percent.unwrap_or(0.0), theme.running, app.settings.bar_style);
    let cpu_summary = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("  CPU ", Style::default().fg(theme.title).add_modifier(Modifier::BOLD)),
            cpu_bar,
        ]),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(format!("{} host", cpu_host_str), Style::default().fg(theme.running)),
        ]),
        Line::from(Span::styled(format!("  {}/core ({})", cpu_per_core_str, num_cpus), Style::default().fg(theme.dim))),
    ]);
    frame.render_widget(cpu_summary, summary_cols[0]);

    // Memory
    let mem_bar = mini_bar(stats.mem_percent.unwrap_or(0.0), theme.cyan, app.settings.bar_style);
    let mem_summary = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("  Memory ", Style::default().fg(theme.title).add_modifier(Modifier::BOLD)),
            mem_bar,
        ]),
        Line::from(Span::styled(format!("  {}", mem_str), Style::default().fg(theme.cyan))),
        Line::from(Span::styled(format!("  {}", mem_pct_str), Style::default().fg(theme.cyan))),
    ]);
    frame.render_widget(mem_summary, summary_cols[1]);

    // Disk I/O
    let disk_summary = Paragraph::new(vec![
        Line::from(Span::styled("  Disk I/O", Style::default().fg(theme.title).add_modifier(Modifier::BOLD))),
        Line::from(vec![
            Span::styled("  R: ", Style::default().fg(theme.dim)),
            Span::styled(blk_r, Style::default().fg(theme.purple)),
        ]),
        Line::from(vec![
            Span::styled("  W: ", Style::default().fg(theme.dim)),
            Span::styled(blk_w, Style::default().fg(theme.purple)),
        ]),
    ]);
    frame.render_widget(disk_summary, summary_cols[2]);

    // Network I/O
    let net_summary = Paragraph::new(vec![
        Line::from(Span::styled("  Network", Style::default().fg(theme.title).add_modifier(Modifier::BOLD))),
        Line::from(vec![
            Span::styled("  RX: ", Style::default().fg(theme.dim)),
            Span::styled(net_r, Style::default().fg(theme.cyan)),
        ]),
        Line::from(vec![
            Span::styled("  TX: ", Style::default().fg(theme.dim)),
            Span::styled(net_t, Style::default().fg(theme.cyan)),
        ]),
    ]);
    frame.render_widget(net_summary, summary_cols[3]);

    // PIDs
    let pids_summary = Paragraph::new(vec![
        Line::from(Span::styled("  PIDs", Style::default().fg(theme.title).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled(format!("  {}", pids_str), Style::default().fg(theme.text))),
    ]);
    frame.render_widget(pids_summary, summary_cols[4]);

    // ── Aggregation setup ──
    let agg_mode = app.settings.aggregation_mode;
    let agg_window = app.aggregation_window_ticks();

    // ── CPU Sparkline ──
    let cpu_agg = aggregation::aggregate_ring(&app.history.cpu, agg_mode, agg_window);
    let cpu_data: Vec<u64> = right_align_data(&cpu_agg, rows[1].width);
    let cpu_title = format!(
        "CPU — {} host ({}/core)",
        stats.cpu_percent.map(|c| format!("{:.1}%", c)).unwrap_or_else(|| "n/a".into()),
        stats.cpu_percent.map(|c| format!("{:.1}%", c / num_cpus as f64)).unwrap_or_else(|| "n/a".into()),
    );
    render_graph(frame, &cpu_data, 10000, theme.running, app.settings.graph_style, &cpu_title, theme, rows[1]);

    // ── Memory Sparkline ──
    let mem_agg = aggregation::aggregate_ring(&app.history.mem, agg_mode, agg_window);
    let mem_data: Vec<u64> = right_align_data(&mem_agg, rows[2].width);
    let mem_max = mem_data.iter().copied().max().unwrap_or(1).max(1);
    let mem_title = format!(
        "Memory — {} ({})",
        stats.mem_usage.as_deref().unwrap_or("n/a"),
        stats.mem_percent.map(|p| format!("{:.1}%", p)).unwrap_or_else(|| "n/a".into())
    );
    render_graph(frame, &mem_data, mem_max + mem_max / 10, theme.cyan, app.settings.graph_style, &mem_title, theme, rows[2]);

    // ── Disk I/O Sparkline ──
    let disk_cols = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[3]);

    let disk_r_agg = aggregation::aggregate_ring(&app.history.block_read, agg_mode, agg_window);
    let disk_w_agg = aggregation::aggregate_ring(&app.history.block_write, agg_mode, agg_window);
    let disk_r_data: Vec<u64> = right_align_data(&disk_r_agg, disk_cols[0].width);
    let disk_w_data: Vec<u64> = right_align_data(&disk_w_agg, disk_cols[1].width);
    let disk_max = disk_r_data
        .iter()
        .chain(disk_w_data.iter())
        .copied()
        .max()
        .unwrap_or(1)
        .max(1);

    let disk_r_val = aggregation::aggregate_latest(&app.history.block_read, agg_mode, agg_window);
    let disk_w_val = aggregation::aggregate_latest(&app.history.block_write, agg_mode, agg_window);
    let disk_r_title = format!("Disk Read — {}", format_bytes_short(disk_r_val));
    let disk_w_title = format!("Disk Write — {}", format_bytes_short(disk_w_val));

    let disk_y_max = disk_max + disk_max / 10;
    render_graph(frame, &disk_r_data, disk_y_max, theme.purple, app.settings.graph_style, &disk_r_title, theme, disk_cols[0]);
    render_graph(frame, &disk_w_data, disk_y_max, theme.stopped, app.settings.graph_style, &disk_w_title, theme, disk_cols[1]);

    // ── Network I/O Sparkline ──
    let net_cols = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[4]);

    let net_rx_agg = aggregation::aggregate_ring(&app.history.net_rx, agg_mode, agg_window);
    let net_tx_agg = aggregation::aggregate_ring(&app.history.net_tx, agg_mode, agg_window);
    let net_rx_data: Vec<u64> = right_align_data(&net_rx_agg, net_cols[0].width);
    let net_tx_data: Vec<u64> = right_align_data(&net_tx_agg, net_cols[1].width);
    let net_max = net_rx_data
        .iter()
        .chain(net_tx_data.iter())
        .copied()
        .max()
        .unwrap_or(1)
        .max(1);

    let net_rx_val = aggregation::aggregate_latest(&app.history.net_rx, agg_mode, agg_window);
    let net_tx_val = aggregation::aggregate_latest(&app.history.net_tx, agg_mode, agg_window);
    let net_rx_title = format!("Net RX — {}", format_bytes_short(net_rx_val));
    let net_tx_title = format!("Net TX — {}", format_bytes_short(net_tx_val));

    let net_y_max = net_max + net_max / 10;
    render_graph(frame, &net_rx_data, net_y_max, theme.cyan, app.settings.graph_style, &net_rx_title, theme, net_cols[0]);
    render_graph(frame, &net_tx_data, net_y_max, theme.accent, app.settings.graph_style, &net_tx_title, theme, net_cols[1]);
}

/// Detected log level for a line.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    None,
}

/// Detect log level from a line by scanning for common keywords.
fn detect_log_level(line: &str) -> LogLevel {
    // Scan only the first ~120 chars (level keywords appear early in the line)
    let prefix: &str = if line.len() > 120 { &line[..120] } else { line };
    let upper = prefix.to_ascii_uppercase();

    // Check for common level patterns: "ERROR", "ERR", "FATAL", "PANIC",
    // "WARN", "WARNING", "INFO", "DEBUG", "DBG", "TRACE", "TRC"
    // Use word-boundary-aware matching to avoid false positives like "INFORMATION"
    for token in upper.split(|c: char| !c.is_ascii_alphanumeric()) {
        match token {
            "ERROR" | "ERR" | "FATAL" | "PANIC" | "CRITICAL" | "CRIT" => return LogLevel::Error,
            "WARN" | "WARNING" => return LogLevel::Warn,
            "INFO" => return LogLevel::Info,
            "DEBUG" | "DBG" => return LogLevel::Debug,
            "TRACE" | "TRC" => return LogLevel::Trace,
            _ => {}
        }
    }
    LogLevel::None
}

/// Color a log line based on its detected level.
fn highlight_log_line<'a>(line: &'a str, theme: &Theme) -> Line<'a> {
    let level = detect_log_level(line);

    let color = match level {
        LogLevel::Error => Color::Rgb(231, 76, 60),   // red
        LogLevel::Warn => Color::Rgb(230, 180, 40),   // yellow/amber
        LogLevel::Info => theme.running,               // green
        LogLevel::Debug => theme.cyan,                 // blue
        LogLevel::Trace => theme.dim,                  // dim/gray
        LogLevel::None => theme.text,                  // default
    };

    Line::from(Span::styled(line, Style::default().fg(color)))
}

fn draw_logs(frame: &mut Frame, app: &mut App, theme: &Theme, area: ratatui::layout::Rect) {
    let container_name = app
        .containers
        .get(app.selected)
        .map(|c| c.name.as_str())
        .unwrap_or("unknown");

    let title = format!("Logs: {}", container_name);

    let mut lines: Vec<Line> = app
        .logs
        .iter()
        .map(|l| {
            if app.settings.log_color {
                highlight_log_line(l.as_str(), theme)
            } else {
                Line::from(Span::styled(l.as_str(), Style::default().fg(theme.text)))
            }
        })
        .collect();

    // When auto-scrolling, add padding so the last log line isn't flush
    // against the bottom border.
    let bottom_padding: usize = if app.auto_scroll { 3 } else { 0 };
    for _ in 0..bottom_padding {
        lines.push(Line::default());
    }

    let block = content_block(&title, theme);
    let inner = block.inner(area);
    let inner_width = inner.width as usize;
    let inner_height = inner.height as usize;

    // Count wrapped visual lines so scroll calculation is accurate.
    let visual_line_count: usize = lines
        .iter()
        .map(|line| {
            let w = line.width();
            if w == 0 || inner_width == 0 {
                1
            } else {
                w.div_ceil(inner_width)
            }
        })
        .sum();

    // Recalculate scroll when auto-scrolling so the last log line is visible.
    if app.auto_scroll && !lines.is_empty() {
        if visual_line_count > inner_height {
            app.log_scroll = (visual_line_count - inner_height) as u16;
        } else {
            app.log_scroll = 0;
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.log_scroll, 0));

    frame.render_widget(paragraph, area);
}

fn draw_action_menu(frame: &mut Frame, app: &App, theme: &Theme) {
    let menu = match app.action_menu {
        Some(ref m) => m,
        None => return,
    };

    let container_name = app
        .containers
        .get(app.selected)
        .map(|c| c.name.as_str())
        .unwrap_or("unknown");

    let title = format!(" {} ", container_name);

    // Size the popup: width based on longest label, height based on item count
    let item_count = menu.actions.len() as u16;
    let menu_width: u16 = 26;
    let menu_height = item_count + 2; // +2 for borders

    // Center the popup
    let area = frame.area();
    let x = area.x + (area.width.saturating_sub(menu_width)) / 2;
    let y = area.y + (area.height.saturating_sub(menu_height)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, menu_width, menu_height);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme.title)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.bg));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let lines: Vec<Line> = menu
        .actions
        .iter()
        .enumerate()
        .map(|(i, action)| {
            let is_selected = i == menu.selected;
            let label = format!(" {} ", action.label());
            if is_selected {
                Line::from(Span::styled(
                    label,
                    Style::default()
                        .fg(Color::White)
                        .bg(theme.border)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::styled(label, Style::default().fg(theme.text)))
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

fn draw_info_popup(frame: &mut Frame, app: &App, theme: &Theme) {
    let msg = match app.info_popup {
        Some(ref m) => m,
        None => return,
    };

    // Word-wrap the message to fit the popup width
    let max_text_width = 40usize;
    let mut wrapped_lines: Vec<String> = Vec::new();
    for line in msg.lines() {
        let mut current = String::new();
        for word in line.split_whitespace() {
            if current.is_empty() {
                current = word.to_string();
            } else if current.len() + 1 + word.len() > max_text_width {
                wrapped_lines.push(current);
                current = word.to_string();
            } else {
                current.push(' ');
                current.push_str(word);
            }
        }
        wrapped_lines.push(current);
    }

    let popup_width = (max_text_width + 4) as u16; // padding + borders
    let popup_height = wrapped_lines.len() as u16 + 4; // borders + blank line + dismiss hint

    let area = frame.area();
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(
            " Info ",
            Style::default()
                .fg(theme.title)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.bg));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let mut lines: Vec<Line> = wrapped_lines
        .iter()
        .map(|l| Line::from(Span::styled(format!(" {} ", l), Style::default().fg(theme.text))))
        .collect();
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        " Press Esc to Dismiss ",
        Style::default().fg(theme.dim),
    )));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

/// Render a single settings row (label + value selector).
fn settings_row<'a>(label: &str, value: &str, selected: bool, editing: bool, theme: &Theme) -> Line<'a> {
    let value_display = if editing {
        format!(" ◀ {} ▶ ", value)
    } else {
        format!("   {}   ", value)
    };
    if selected && editing {
        // Editing: bright highlight on value, arrows shown
        Line::from(vec![
            Span::styled(
                format!(" {} ", label),
                Style::default()
                    .fg(theme.title)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                value_display,
                Style::default()
                    .fg(theme.bg)
                    .bg(theme.title)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if selected {
        // Selected but not editing: subtle highlight
        Line::from(vec![
            Span::styled(
                format!(" {} ", label),
                Style::default()
                    .fg(theme.title)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                value_display,
                Style::default()
                    .fg(Color::White)
                    .bg(theme.border)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(format!(" {} ", label), Style::default().fg(theme.text)),
            Span::styled(value_display, Style::default().fg(theme.dim)),
        ])
    }
}

fn draw_settings(frame: &mut Frame, app: &App, theme: &Theme, area: ratatui::layout::Rect) {
    use crate::settings::ColumnVisibility;

    let on_off = |v: bool| if v { "On" } else { "Off" };
    let sel = app.settings_selection;
    let editing = app.settings_editing;

    // ── Build data for each section ──

    let general: Vec<(&str, String)> = vec![
        ("Aggregation Mode", app.settings.aggregation_mode.label().to_string()),
        ("Aggregation Window", app.settings.aggregation_window.label()),
        ("Color Theme", app.settings.theme.label().to_string()),
        ("Refresh Rate", app.settings.refresh_rate.label().to_string()),
        ("Log Buffer Size", app.settings.log_buffer_size.label().to_string()),
        ("Poll All Containers", on_off(app.settings.poll_all_containers).to_string()),
    ];

    let sorting: Vec<(&str, String)> = vec![
        ("Sort By", app.settings.sort_by.label().to_string()),
    ];

    let logs: Vec<(&str, String)> = vec![
        ("Log Colors", on_off(app.settings.log_color).to_string()),
    ];

    let mut columns: Vec<(&str, String)> = Vec::new();
    for i in 0..ColumnVisibility::COUNT {
        columns.push((
            ColumnVisibility::column_label(i),
            on_off(app.settings.columns.is_visible(i)).to_string(),
        ));
    }

    let bars: Vec<(&str, String)> = vec![
        ("Bar Style", app.settings.bar_style.label().to_string()),
        ("Graph Style", app.settings.graph_style.label().to_string()),
        ("CPU", on_off(app.settings.show_cpu_bar).to_string()),
        ("MEM", on_off(app.settings.show_mem_bar).to_string()),
        ("Disk", on_off(app.settings.show_disk_bar).to_string()),
        ("Network", on_off(app.settings.show_network_bar).to_string()),
    ];

    // ── Layout: outer block, then two columns ──

    let outer = content_block("Settings", theme);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let col_layout =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(inner);

    // ── Left column: General + Sorting + Logs ──
    // Compute heights: each section needs rows + 2 (border)
    let general_h = general.len() as u16 + 2;
    let sorting_h = sorting.len() as u16 + 2;
    let logs_h = logs.len() as u16 + 2;
    let left_sections = Layout::vertical([
        Constraint::Length(general_h),
        Constraint::Length(sorting_h),
        Constraint::Length(logs_h),
        Constraint::Min(0),
    ])
    .split(col_layout[0]);

    // General box
    let general_block = spark_block("General", theme);
    let general_inner = general_block.inner(left_sections[0]);
    frame.render_widget(general_block, left_sections[0]);

    let general_lines: Vec<Line> = general
        .iter()
        .enumerate()
        .map(|(i, (label, value))| {
            let is_sel = sel == i;
            settings_row(label, value, is_sel, is_sel && editing, theme)
        })
        .collect();
    frame.render_widget(Paragraph::new(general_lines), general_inner);

    // Sorting box
    let sorting_block = spark_block("Sorting", theme);
    let sorting_inner = sorting_block.inner(left_sections[1]);
    frame.render_widget(sorting_block, left_sections[1]);

    let sorting_lines: Vec<Line> = sorting
        .iter()
        .enumerate()
        .map(|(i, (label, value))| {
            let is_sel = sel == 19 + i;
            settings_row(label, value, is_sel, is_sel && editing, theme)
        })
        .collect();
    frame.render_widget(Paragraph::new(sorting_lines), sorting_inner);

    // Logs box
    let logs_block = spark_block("Logs", theme);
    let logs_inner = logs_block.inner(left_sections[2]);
    frame.render_widget(logs_block, left_sections[2]);

    let logs_lines: Vec<Line> = logs
        .iter()
        .enumerate()
        .map(|(i, (label, value))| {
            let is_sel = sel == 18 + i;
            settings_row(label, value, is_sel, is_sel && editing, theme)
        })
        .collect();
    frame.render_widget(Paragraph::new(logs_lines), logs_inner);

    // ── Right column: Columns + Mini Bars + Preview ──
    let columns_h = columns.len() as u16 + 2;
    let bars_h = bars.len() as u16 + 2;
    let preview_h: u16 = 6 + 6; // bar box(6) + graph box(6: border 2 + 4 rows like real graphs)
    let right_sections = Layout::vertical([
        Constraint::Length(columns_h),
        Constraint::Length(bars_h),
        Constraint::Length(preview_h),
        Constraint::Min(0),
    ])
    .split(col_layout[1]);

    // Columns box
    let columns_block = spark_block("Columns", theme);
    let columns_inner = columns_block.inner(right_sections[0]);
    frame.render_widget(columns_block, right_sections[0]);

    let columns_lines: Vec<Line> = columns
        .iter()
        .enumerate()
        .map(|(i, (label, value))| {
            let short = label.strip_prefix("Column: ").unwrap_or(label);
            let is_sel = sel == 6 + i;
            settings_row(short, value, is_sel, is_sel && editing, theme)
        })
        .collect();
    frame.render_widget(Paragraph::new(columns_lines), columns_inner);

    // Mini Bars box
    let bars_block = spark_block("Mini Bars", theme);
    let bars_inner = bars_block.inner(right_sections[1]);
    frame.render_widget(bars_block, right_sections[1]);

    // Flat indices: Bar Style=20, Graph Style=21, CPU=14, MEM=15, Disk=16, Network=17
    let bars_flat_idx = [20, 21, 14, 15, 16, 17];
    let bars_lines: Vec<Line> = bars
        .iter()
        .enumerate()
        .map(|(i, (label, value))| {
            let is_sel = sel == bars_flat_idx[i];
            settings_row(label, value, is_sel, is_sel && editing, theme)
        })
        .collect();
    frame.render_widget(Paragraph::new(bars_lines), bars_inner);

    // ── Preview box ──
    let preview_area = right_sections[2];
    if preview_area.height >= 6 {
        // Split preview into bar samples (top) and sparkline graph (bottom)
        let preview_sections = Layout::vertical([
            Constraint::Length(6), // border(2) + 3 bar samples + 1 pad
            Constraint::Length(6), // border(2) + 4 rows (matches real sparkline Min(4))
        ])
        .split(preview_area);

        // Bar preview box
        let bar_block = spark_block(
            &format!("Bar Preview — {}", app.settings.bar_style.label()),
            theme,
        );
        let bar_inner = bar_block.inner(preview_sections[0]);
        frame.render_widget(bar_block, preview_sections[0]);

        let bar_w = (bar_inner.width as usize).saturating_sub(7); // room for " 25% "
        let sample_lines: Vec<Line> = [25.0, 50.0, 75.0]
            .iter()
            .map(|&pct| {
                let bar = mini_bar_width(pct, theme.running, bar_w.max(4), app.settings.bar_style);
                Line::from(vec![
                    Span::styled(format!(" {:>2.0}% ", pct), Style::default().fg(theme.dim)),
                    bar,
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(sample_lines), bar_inner);

        // Graph preview box — render with synthetic sine wave using current style
        let graph_title = format!("Graph Preview — {}", app.settings.graph_style.label());
        let graph_w = preview_sections[1].width.saturating_sub(2) as usize; // inside border
        let sample_data: Vec<u64> = (0..graph_w.max(1))
            .map(|i| {
                let t = i as f64 / graph_w.max(1) as f64 * std::f64::consts::PI * 3.0;
                ((t.sin() * 0.4 + 0.5) * 10000.0) as u64
            })
            .collect();
        render_graph(frame, &sample_data, 10000, theme.cyan, app.settings.graph_style, &graph_title, theme, preview_sections[1]);
    }
}
