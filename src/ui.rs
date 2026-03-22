use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Sparkline, Table, Wrap,
};
use ratatui::Frame;

use crate::aggregation;
use crate::app::{App, Page};
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
        " 🐋 WhaleTop ",
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
                "s: settings  q: quit".to_string()
            } else {
                "Left/Right: navigate  Up/Down: select  Enter: actions  s: settings  q: quit".to_string()
            }
        }
        Page::Detail => {
            "Left/Right: navigate  Up/Down: scroll  Enter: actions  PgUp/PgDn: container  s: settings  q: quit".to_string()
        }
        Page::Resources => {
            "Left/Right: navigate  Enter: actions  PgUp/PgDn: container  s: settings  q: quit".to_string()
        }
        Page::Logs => {
            let scroll_indicator = if app.auto_scroll { "ON" } else { "OFF" };
            format!(
                "Left/Right: navigate  Up/Down: scroll  Enter: actions  PgUp/PgDn: container  a: auto-scroll [{}]  s: settings  q: quit",
                scroll_indicator
            )
        }
        Page::Settings => {
            "Up/Down: navigate  Left/Right: change value  Esc/s: back  q: quit".to_string()
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

    let rows: Vec<Row> = app
        .containers
        .iter()
        .enumerate()
        .map(|(i, c)| {
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
                    let bar = mini_bar_width(pct.unwrap_or(0.0), theme.running, 6);
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
                    let bar = mini_bar_width(pct.unwrap_or(0.0), theme.cyan, 6);
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
                    let bar = mini_bar_width(pct, theme.purple, 6);
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
                    let bar = mini_bar_width(pct, theme.cyan, 6);
                    cells.push(Cell::from(Line::from(vec![
                        bar,
                        Span::styled(format!(" {}", val), style),
                    ])));
                } else {
                    cells.push(Cell::from(val).style(style));
                }
            }

            Row::new(cells)
        })
        .collect();

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
                lines.push(detail_line("  ", e, theme));
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

/// Render a mini inline bar like [###------] for a 0–100 percentage.
fn mini_bar(percent: f64, fill_color: Color) -> Span<'static> {
    mini_bar_width(percent, fill_color, 10)
}

/// Render a mini inline bar with configurable width.
fn mini_bar_width(percent: f64, fill_color: Color, width: usize) -> Span<'static> {
    let clamped = percent.clamp(0.0, 100.0);
    let filled = ((clamped / 100.0) * width as f64).round() as usize;
    let empty = width - filled;
    let bar = format!("[{}{}]", "#".repeat(filled), "-".repeat(empty));
    Span::styled(bar, Style::default().fg(fill_color))
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
    let cpu_bar = mini_bar(stats.cpu_percent.unwrap_or(0.0), theme.running);
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
    let mem_bar = mini_bar(stats.mem_percent.unwrap_or(0.0), theme.cyan);
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
    let cpu_spark = Sparkline::default()
        .block(spark_block(&cpu_title, theme))
        .data(&cpu_data)
        .max(10000) // 100.00% in fixed-point
        .style(Style::default().fg(theme.running))
        .bar_set(symbols::bar::NINE_LEVELS);
    frame.render_widget(cpu_spark, rows[1]);

    // ── Memory Sparkline ──
    let mem_agg = aggregation::aggregate_ring(&app.history.mem, agg_mode, agg_window);
    let mem_data: Vec<u64> = right_align_data(&mem_agg, rows[2].width);
    let mem_max = mem_data.iter().copied().max().unwrap_or(1).max(1);
    let mem_title = format!(
        "Memory — {} ({})",
        stats.mem_usage.as_deref().unwrap_or("n/a"),
        stats.mem_percent.map(|p| format!("{:.1}%", p)).unwrap_or_else(|| "n/a".into())
    );
    let mem_spark = Sparkline::default()
        .block(spark_block(&mem_title, theme))
        .data(&mem_data)
        .max(mem_max + mem_max / 10) // 10% headroom
        .style(Style::default().fg(theme.cyan))
        .bar_set(symbols::bar::NINE_LEVELS);
    frame.render_widget(mem_spark, rows[2]);

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

    let disk_r_spark = Sparkline::default()
        .block(spark_block(&disk_r_title, theme))
        .data(&disk_r_data)
        .max(disk_max + disk_max / 10)
        .style(Style::default().fg(theme.purple))
        .bar_set(symbols::bar::NINE_LEVELS);
    frame.render_widget(disk_r_spark, disk_cols[0]);

    let disk_w_spark = Sparkline::default()
        .block(spark_block(&disk_w_title, theme))
        .data(&disk_w_data)
        .max(disk_max + disk_max / 10)
        .style(Style::default().fg(theme.stopped))
        .bar_set(symbols::bar::NINE_LEVELS);
    frame.render_widget(disk_w_spark, disk_cols[1]);

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

    let net_rx_spark = Sparkline::default()
        .block(spark_block(&net_rx_title, theme))
        .data(&net_rx_data)
        .max(net_max + net_max / 10)
        .style(Style::default().fg(theme.cyan))
        .bar_set(symbols::bar::NINE_LEVELS);
    frame.render_widget(net_rx_spark, net_cols[0]);

    let net_tx_spark = Sparkline::default()
        .block(spark_block(&net_tx_title, theme))
        .data(&net_tx_data)
        .max(net_max + net_max / 10)
        .style(Style::default().fg(theme.accent))
        .bar_set(symbols::bar::NINE_LEVELS);
    frame.render_widget(net_tx_spark, net_cols[1]);
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
        .map(|l| Line::from(Span::styled(l.as_str(), Style::default().fg(theme.text))))
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

fn draw_settings(frame: &mut Frame, app: &App, theme: &Theme, area: ratatui::layout::Rect) {
    use crate::settings::ColumnVisibility;

    let on_off = |v: bool| if v { "On" } else { "Off" };

    let mut settings_rows: Vec<(&str, String)> = vec![
        ("Aggregation Mode", app.settings.aggregation_mode.label().to_string()),
        ("Aggregation Window", app.settings.aggregation_window.label()),
        ("Color Theme", app.settings.theme.label().to_string()),
        ("Refresh Rate", app.settings.refresh_rate.label().to_string()),
        ("Log Buffer Size", app.settings.log_buffer_size.label().to_string()),
        ("Poll All Containers", on_off(app.settings.poll_all_containers).to_string()),
    ];

    // Column visibility toggles
    for i in 0..ColumnVisibility::COUNT {
        settings_rows.push((
            ColumnVisibility::column_label(i),
            on_off(app.settings.columns.is_visible(i)).to_string(),
        ));
    }

    // Mini bar toggles
    settings_rows.push(("Bar: CPU", on_off(app.settings.show_cpu_bar).to_string()));
    settings_rows.push(("Bar: MEM", on_off(app.settings.show_mem_bar).to_string()));
    settings_rows.push(("Bar: Disk", on_off(app.settings.show_disk_bar).to_string()));
    settings_rows.push(("Bar: Network", on_off(app.settings.show_network_bar).to_string()));

    let block = content_block("Settings", theme);
    let inner_height = block.inner(area).height as usize;

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::default());

    for (i, (label, value)) in settings_rows.iter().enumerate() {
        // Add section separators
        if i == 6 {
            lines.push(Line::from(Span::styled(
                "  ── Container List ──".to_string(),
                Style::default()
                    .fg(theme.border)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::default());
        }
        if i == 14 {
            lines.push(Line::from(Span::styled(
                "  ── Mini Bars ──".to_string(),
                Style::default()
                    .fg(theme.border)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::default());
        }

        let is_selected = i == app.settings_selection;
        let value_display = format!("  < {} >  ", value);

        if is_selected {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", label),
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
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", label),
                    Style::default().fg(theme.text),
                ),
                Span::styled(
                    value_display,
                    Style::default().fg(theme.dim),
                ),
            ]));
        }
        lines.push(Line::default());
    }

    // Auto-scroll to keep selected row visible
    // Each settings row takes 2 lines (content + blank), plus header blank + section separators
    let separators = if app.settings_selection >= 14 {
        4 // two separators (2 lines each)
    } else if app.settings_selection >= 6 {
        2 // one separator
    } else {
        0
    };
    let selected_line = 1 + app.settings_selection * 2 + separators;
    let scroll = if selected_line + 2 > inner_height {
        (selected_line + 2 - inner_height) as u16
    } else {
        0
    };

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((scroll, 0));
    frame.render_widget(paragraph, area);
}
