mod aggregation;
mod app;
mod docker_client;
mod settings;
mod theme;
mod ui;

use std::io;
use std::process::Command;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{
    self, BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{ContainerAction, ContainerInfo, Page};
use settings::SortBy;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILD_DATE: &str = env!("BUILD_DATE");
const GIT_HASH: &str = env!("GIT_HASH");
const GIT_BRANCH: &str = env!("GIT_BRANCH");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Handle CLI arguments before entering the TUI
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "--version" | "-V" => {
                println!(
                    "wtop {} (built {}, {}/{})",
                    VERSION, BUILD_DATE, GIT_BRANCH, GIT_HASH
                );
                return Ok(());
            }
            "--update" => {
                run_update(false)?;
                return Ok(());
            }
            "--reinstall" => {
                run_update(true)?;
                return Ok(());
            }
            arg => {
                eprintln!("Unknown argument: {}", arg);
                eprintln!("Usage: wtop [--version] [--update] [--reinstall]");
                std::process::exit(1);
            }
        }
    }

    // Connect to Docker (fail gracefully if unavailable)
    let docker = docker_client::connect().ok();

    // Terminal setup
    terminal::enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let settings = settings::Settings::load();
    let mut app = app::App::new(settings);

    let result = run_loop(&mut terminal, &mut app, &docker).await;

    // Terminal teardown (always runs)
    terminal::disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

/// Load static detail + initial stats + logs for the currently selected container.
async fn load_container_data(app: &mut app::App, docker: &bollard::Docker) {
    if let Some(id) = app.selected_container_id() {
        let id = id.to_string();
        app.detail = docker_client::inspect_container(docker, &id).await;

        if app.settings.poll_all_containers {
            // Reuse existing history/stats from background polling
            if let Some(h) = app.all_history.get(&id) {
                app.history = h.clone();
            } else {
                app.history = app::StatsHistory::new();
            }
            if let Some(s) = app.all_stats.get(&id) {
                app.stats = Some(s.clone());
            } else {
                app.stats = docker_client::fetch_stats(docker, &id).await;
            }
        } else {
            app.stats = docker_client::fetch_stats(docker, &id).await;
            if let Some(ref s) = app.stats {
                app.history = app::StatsHistory::new();
                let (cpu_pct, percpu_pcts) = app.history.push(s);
                if let Some(ref mut ns) = app.stats {
                    ns.cpu_percent = cpu_pct;
                    ns.percpu_percent = percpu_pcts;
                }
            }
        }

        let log_lines = app.settings.log_buffer_size.as_usize();
        app.logs = docker_client::fetch_logs(docker, &id, log_lines).await;
        app.logs_container_id = Some(id);
        if !app.auto_scroll {
            app.log_scroll = 0;
        }
        app.detail_scroll = 0;
    }
}

/// Execute a container action from the action menu.
async fn handle_action(
    app: &mut app::App,
    docker: &Option<bollard::Docker>,
    action: ContainerAction,
) {
    // Navigation actions don't need Docker
    match action {
        ContainerAction::Details => {
            if let Some(docker) = docker {
                load_container_data(app, docker).await;
            }
            app.set_page(Page::Detail);
            return;
        }
        ContainerAction::Logs => {
            if let Some(docker) = docker {
                load_container_data(app, docker).await;
            }
            app.set_page(Page::Logs);
            return;
        }
        _ => {}
    }

    let docker = match docker {
        Some(d) => d,
        None => {
            app.set_status("No Docker connection".to_string());
            return;
        }
    };

    let id = match app.selected_container_id() {
        Some(id) => id.to_string(),
        None => return,
    };

    let name = app
        .containers
        .get(app.selected)
        .map(|c| c.name.clone())
        .unwrap_or_default();

    // Exec is handled separately in run_loop (needs terminal access)
    if action == ContainerAction::Exec {
        app.pending_exec = Some(id);
        return;
    }

    let result = match action {
        ContainerAction::Start => docker_client::start_container(docker, &id).await,
        ContainerAction::Stop => docker_client::stop_container(docker, &id).await,
        ContainerAction::Restart => docker_client::restart_container(docker, &id).await,
        ContainerAction::Pause => docker_client::pause_container(docker, &id).await,
        ContainerAction::Unpause => docker_client::unpause_container(docker, &id).await,
        ContainerAction::Kill => docker_client::kill_container(docker, &id).await,
        ContainerAction::Remove => docker_client::remove_container(docker, &id).await,
        ContainerAction::Details | ContainerAction::Logs | ContainerAction::Exec => {
            unreachable!()
        }
    };

    match result {
        Ok(()) => app.set_status(format!("{}: {}", action.label(), name)),
        Err(e) => app.set_status(format!("Error: {}", e)),
    }
}

/// Check if any mini bar is enabled without poll_all_containers and show a hint.
fn check_poll_all_hint(app: &mut app::App) {
    let any_bar = app.settings.show_cpu_bar
        || app.settings.show_mem_bar
        || app.settings.show_disk_bar
        || app.settings.show_network_bar;
    if any_bar && !app.settings.poll_all_containers {
        app.info_popup = Some(
            "To use performance indicators on the container overview page you have to enable \"Poll All Containers\".".to_string()
        );
    }
}

/// Settings grid layout.
/// Left column:  General (0-5), Sorting (19), Logs (18)  → 8 rows
/// Right column: Columns (6-13), Bars & Graphs (20,21,14-17) → 14 rows
///
/// Grid is addressed as (column, row_within_column).
/// These helpers convert between the flat selection index and grid position.
/// Left-column flat indices in display order.
const LEFT_COL: &[usize] = &[0, 1, 2, 3, 4, 5, 19, 18];
/// Right-column flat indices in display order.
const RIGHT_COL: &[usize] = &[6, 7, 8, 9, 10, 11, 12, 13, 20, 21, 14, 15, 16, 17];

fn settings_grid_pos(sel: usize) -> (usize, usize) {
    // (column, row_within_column)
    if let Some(row) = LEFT_COL.iter().position(|&i| i == sel) {
        (0, row)
    } else if let Some(row) = RIGHT_COL.iter().position(|&i| i == sel) {
        (1, row)
    } else {
        (0, 0)
    }
}

fn settings_from_grid(col: usize, row: usize) -> usize {
    if col == 0 {
        LEFT_COL[row.min(LEFT_COL.len() - 1)]
    } else {
        RIGHT_COL[row.min(RIGHT_COL.len() - 1)]
    }
}

/// Adjust a setting value (next/prev).
fn adjust_setting(app: &mut app::App, forward: bool) {
    match app.settings_selection {
        0 => app.settings.aggregation_mode = if forward {
            app.settings.aggregation_mode.next()
        } else {
            app.settings.aggregation_mode.prev()
        },
        1 => if forward { app.settings.aggregation_window.increment() }
             else { app.settings.aggregation_window.decrement() },
        2 => app.settings.theme = if forward {
            app.settings.theme.next()
        } else {
            app.settings.theme.prev()
        },
        3 => app.settings.refresh_rate = if forward {
            app.settings.refresh_rate.next()
        } else {
            app.settings.refresh_rate.prev()
        },
        4 => app.settings.log_buffer_size = if forward {
            app.settings.log_buffer_size.next()
        } else {
            app.settings.log_buffer_size.prev()
        },
        5 => {
            app.settings.poll_all_containers = !app.settings.poll_all_containers;
            if !app.settings.poll_all_containers {
                app.all_history.clear();
                app.all_stats.clear();
            }
        }
        6..=13 => app.settings.columns.toggle(app.settings_selection - 6),
        14 => app.settings.show_cpu_bar = !app.settings.show_cpu_bar,
        15 => app.settings.show_mem_bar = !app.settings.show_mem_bar,
        16 => app.settings.show_disk_bar = !app.settings.show_disk_bar,
        17 => app.settings.show_network_bar = !app.settings.show_network_bar,
        18 => app.settings.log_color = !app.settings.log_color,
        19 => app.settings.sort_by = if forward {
            app.settings.sort_by.next()
        } else {
            app.settings.sort_by.prev()
        },
        20 => app.settings.bar_style = if forward {
            app.settings.bar_style.next()
        } else {
            app.settings.bar_style.prev()
        },
        21 => app.settings.graph_style = if forward {
            app.settings.graph_style.next()
        } else {
            app.settings.graph_style.prev()
        },
        _ => {}
    }
    app.settings.save();
    if matches!(app.settings_selection, 14..=17) {
        check_poll_all_hint(app);
    }
}

/// Handle key input on the Settings page.
fn handle_settings_key(app: &mut app::App, key: KeyCode) {
    if app.settings_editing {
        // Editing mode: Left/Right change value, Enter/Esc exit
        match key {
            KeyCode::Right => adjust_setting(app, true),
            KeyCode::Left => adjust_setting(app, false),
            KeyCode::Enter | KeyCode::Esc => {
                app.settings_editing = false;
            }
            _ => {}
        }
        return;
    }

    // Navigation mode
    match key {
        KeyCode::Esc | KeyCode::Char('s') => {
            app.settings_editing = false;
            app.set_page(app.previous_page.unwrap_or(Page::List));
            app.previous_page = None;
        }
        KeyCode::Up => {
            let (col, row) = settings_grid_pos(app.settings_selection);
            if row > 0 {
                app.settings_selection = settings_from_grid(col, row - 1);
            }
        }
        KeyCode::Down => {
            let (col, row) = settings_grid_pos(app.settings_selection);
            let max_row = if col == 0 { LEFT_COL.len() - 1 } else { RIGHT_COL.len() - 1 };
            if row < max_row {
                app.settings_selection = settings_from_grid(col, row + 1);
            }
        }
        KeyCode::Left => {
            let (col, row) = settings_grid_pos(app.settings_selection);
            if col == 1 {
                app.settings_selection = settings_from_grid(0, row);
            }
        }
        KeyCode::Right => {
            let (col, row) = settings_grid_pos(app.settings_selection);
            if col == 0 {
                app.settings_selection = settings_from_grid(1, row);
            }
        }
        KeyCode::Enter => {
            app.settings_editing = true;
        }
        _ => {}
    }
}

/// Sort the container list according to the current sort setting.
fn sort_containers(containers: &mut [ContainerInfo], sort_by: SortBy, all_stats: &std::collections::HashMap<String, app::ContainerStats>) {
    match sort_by {
        SortBy::Name => containers.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        SortBy::Status => containers.sort_by(|a, b| {
            // Running first, then paused, then stopped
            fn status_rank(s: &str) -> u8 {
                if s.contains("Up") && !s.contains("Paused") { 0 }
                else if s.contains("Paused") { 1 }
                else { 2 }
            }
            status_rank(&a.status).cmp(&status_rank(&b.status))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        }),
        SortBy::Cpu => containers.sort_by(|a, b| {
            let a_cpu = all_stats.get(&a.id).and_then(|s| s.cpu_percent).unwrap_or(-1.0);
            let b_cpu = all_stats.get(&b.id).and_then(|s| s.cpu_percent).unwrap_or(-1.0);
            b_cpu.partial_cmp(&a_cpu).unwrap_or(std::cmp::Ordering::Equal)
        }),
        SortBy::Memory => containers.sort_by(|a, b| {
            let a_mem = all_stats.get(&a.id).and_then(|s| s.mem_used).unwrap_or(0);
            let b_mem = all_stats.get(&b.id).and_then(|s| s.mem_used).unwrap_or(0);
            b_mem.cmp(&a_mem)
        }),
        SortBy::Disk => containers.sort_by(|a, b| {
            let a_disk = all_stats.get(&a.id).and_then(|s| s.block_read).unwrap_or(0);
            let b_disk = all_stats.get(&b.id).and_then(|s| s.block_read).unwrap_or(0);
            b_disk.cmp(&a_disk)
        }),
        SortBy::Network => containers.sort_by(|a, b| {
            let a_net = all_stats.get(&a.id).and_then(|s| s.net_rx).unwrap_or(0);
            let b_net = all_stats.get(&b.id).and_then(|s| s.net_rx).unwrap_or(0);
            b_net.cmp(&a_net)
        }),
        SortBy::ComposeProject => containers.sort_by(|a, b| {
            let a_proj = a.compose_project.as_deref().unwrap_or("\u{ffff}");
            let b_proj = b.compose_project.as_deref().unwrap_or("\u{ffff}");
            a_proj.to_lowercase().cmp(&b_proj.to_lowercase())
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        }),
    }
}

/// Check GitHub releases and update (or reinstall) the binary via remote-install.sh.
fn run_update(force: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("wtop {} — checking for updates...", VERSION);

    let output = Command::new("curl")
        .args([
            "-fsSL",
            "-H",
            "Accept: application/vnd.github.v3+json",
            "https://api.github.com/repos/danielme85/wtop/releases/latest",
        ])
        .output()
        .map_err(|e| format!("curl not found: {}", e))?;

    if !output.status.success() {
        eprintln!("Failed to reach GitHub API (curl exited {:?})", output.status.code());
        return Ok(());
    }

    let body = String::from_utf8_lossy(&output.stdout);
    let latest_tag = match extract_json_string(&body, "tag_name") {
        Some(t) => t,
        None => {
            eprintln!("Could not parse latest release version from GitHub API response.");
            return Ok(());
        }
    };
    let latest_version = latest_tag.trim_start_matches('v');

    println!("Latest release: {}", latest_version);

    if !force && !is_newer(latest_version, VERSION) {
        println!(
            "Already up to date ({}). Use --reinstall to force a clean reinstall.",
            VERSION
        );
        return Ok(());
    }

    if force {
        println!("Reinstalling wtop {}...", latest_version);
    } else {
        println!("Updating {} → {}...", VERSION, latest_version);
    }

    let status = Command::new("sh")
        .arg("-c")
        .arg("curl -fsSL https://raw.githubusercontent.com/danielme85/wtop/main/remote-install.sh | sh")
        .status()
        .map_err(|e| format!("Failed to run install script: {}", e))?;

    if status.success() {
        if force {
            println!("\nSuccessfully reinstalled wtop {}.", latest_version);
        } else {
            println!("\nSuccessfully updated wtop {} → {}.", VERSION, latest_version);
        }
        println!("Run 'wtop --version' to confirm.");
    } else {
        eprintln!("Install script failed (exit {:?}).", status.code());
    }

    Ok(())
}

/// Extract the string value for a JSON key (no external parser needed).
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let marker = format!("\"{}\":", key);
    let pos = json.find(&marker)?;
    let after = json[pos + marker.len()..].trim_start();
    if after.starts_with('"') {
        let inner = &after[1..];
        let end = inner.find('"')?;
        Some(inner[..end].to_string())
    } else {
        None
    }
}

/// Returns true if `latest` is strictly greater than `current` (semver numeric comparison).
fn is_newer(latest: &str, current: &str) -> bool {
    fn parse(v: &str) -> Vec<u64> {
        v.split('.').filter_map(|p| p.parse().ok()).collect()
    }
    parse(latest) > parse(current)
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut app::App,
    docker: &Option<bollard::Docker>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_tick = std::time::Instant::now();

    while app.running {
        // Force a full redraw when the page changes to prevent visual artifacts
        if app.needs_clear {
            terminal.clear()?;
            app.needs_clear = false;
        }

        // Draw (synchronized update prevents tearing on modern terminals)
        io::stdout().execute(BeginSynchronizedUpdate)?;
        terminal.draw(|f| ui::draw(f, app))?;
        io::stdout().execute(EndSynchronizedUpdate)?;

        // Poll for input (non-blocking, 50ms timeout)
        let timeout = Duration::from_millis(50);
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                // Info popup takes priority when open
                if app.info_popup.is_some() {
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter => {
                            app.info_popup = None;
                        }
                        _ => {}
                    }
                // Action menu takes priority when open
                } else if app.action_menu.is_some() {
                    match key.code {
                        KeyCode::Esc => app.close_action_menu(),
                        KeyCode::Up => {
                            if let Some(ref mut menu) = app.action_menu {
                                menu.select_prev();
                            }
                        }
                        KeyCode::Down => {
                            if let Some(ref mut menu) = app.action_menu {
                                menu.select_next();
                            }
                        }
                        KeyCode::Enter => {
                            let action = app
                                .action_menu
                                .as_ref()
                                .and_then(|m| m.selected_action());
                            app.close_action_menu();
                            if let Some(action) = action {
                                handle_action(app, docker, action).await;
                            }
                            // Handle exec: suspend TUI, spawn interactive shell, resume
                            if let Some(container_id) = app.pending_exec.take() {
                                terminal::disable_raw_mode()?;
                                io::stdout().execute(LeaveAlternateScreen)?;
                                io::stdout().execute(terminal::Clear(terminal::ClearType::All))?;
                                terminal.show_cursor()?;

                                let status = Command::new("docker")
                                    .args(["exec", "-it", &container_id, "sh"])
                                    .status();

                                match status {
                                    Ok(s) if s.success() => {
                                        app.set_status("Exec: exited shell".to_string());
                                    }
                                    Ok(s) => {
                                        app.set_status(format!(
                                            "Exec: shell exited with {}",
                                            s.code().unwrap_or(-1)
                                        ));
                                    }
                                    Err(e) => {
                                        app.set_status(format!("Exec error: {}", e));
                                    }
                                }

                                terminal::enable_raw_mode()?;
                                io::stdout().execute(EnterAlternateScreen)?;
                                app.needs_clear = true;
                            }
                        }
                        _ => {}
                    }
                } else if key.code == KeyCode::Char('q') {
                    app.quit();
                } else if key.code == KeyCode::Char('s') && app.page != Page::Settings {
                    // Global 's' opens settings from any page
                    app.previous_page = Some(app.page);
                    app.set_page(Page::Settings);
                } else {
                    match app.page {
                        Page::List => match key.code {
                            KeyCode::Tab => {
                                app.settings.sort_by = app.settings.sort_by.next();
                                sort_containers(&mut app.containers, app.settings.sort_by, &app.all_stats);
                                app.settings.save();
                                app.set_status(format!("Sort: {}", app.settings.sort_by.label()));
                            }
                            KeyCode::BackTab => {
                                app.settings.sort_by = app.settings.sort_by.prev();
                                sort_containers(&mut app.containers, app.settings.sort_by, &app.all_stats);
                                app.settings.save();
                                app.set_status(format!("Sort: {}", app.settings.sort_by.label()));
                            }
                            KeyCode::Up => app.select_prev(),
                            KeyCode::Down => app.select_next(),
                            KeyCode::Enter => {
                                if app.selected_container_id().is_some() {
                                    app.open_action_menu();
                                }
                            }
                            KeyCode::Right => {
                                if app.selected_container_id().is_some() {
                                    if let Some(docker) = docker {
                                        load_container_data(app, docker).await;
                                    }
                                    app.set_page(Page::Detail);
                                }
                            }
                            KeyCode::Left => {
                                if app.selected_container_id().is_some() {
                                    if let Some(docker) = docker {
                                        load_container_data(app, docker).await;
                                    }
                                    app.auto_scroll = true;
                                    app.set_page(Page::Logs);
                                }
                            }
                            _ => {}
                        },
                        Page::Detail => match key.code {
                            KeyCode::Left => {
                                app.set_page(Page::List);
                            }
                            KeyCode::Right => {
                                app.set_page(Page::Resources);
                            }
                            KeyCode::Up => {
                                app.detail_scroll = app.detail_scroll.saturating_sub(1);
                            }
                            KeyCode::Down => {
                                app.detail_scroll = app.detail_scroll.saturating_add(1);
                            }
                            KeyCode::Enter => {
                                if app.selected_container_id().is_some() {
                                    app.open_action_menu();
                                }
                            }
                            KeyCode::PageUp => {
                                app.select_prev();
                                if let Some(docker) = docker {
                                    load_container_data(app, docker).await;
                                }
                            }
                            KeyCode::PageDown => {
                                app.select_next();
                                if let Some(docker) = docker {
                                    load_container_data(app, docker).await;
                                }
                            }
                            _ => {}
                        },
                        Page::Resources => match key.code {
                            KeyCode::Left => {
                                app.set_page(Page::Detail);
                            }
                            KeyCode::Right => {
                                app.set_page(Page::Logs);
                            }
                            KeyCode::Enter => {
                                if app.selected_container_id().is_some() {
                                    app.open_action_menu();
                                }
                            }
                            KeyCode::PageUp => {
                                app.select_prev();
                                if let Some(docker) = docker {
                                    load_container_data(app, docker).await;
                                }
                            }
                            KeyCode::PageDown => {
                                app.select_next();
                                if let Some(docker) = docker {
                                    load_container_data(app, docker).await;
                                }
                            }
                            _ => {}
                        },
                        Page::Logs => match key.code {
                            KeyCode::Char('a') => {
                                app.auto_scroll = !app.auto_scroll;
                            }
                            KeyCode::Left => {
                                app.set_page(Page::Resources);
                                app.auto_scroll = true;
                            }
                            KeyCode::Right => {
                                app.set_page(Page::List);
                                app.auto_scroll = true;
                            }
                            KeyCode::Up => {
                                app.auto_scroll = false;
                                app.log_scroll = app.log_scroll.saturating_sub(1);
                            }
                            KeyCode::Down => {
                                app.auto_scroll = false;
                                app.log_scroll = app.log_scroll.saturating_add(1);
                            }
                            KeyCode::Enter => {
                                if app.selected_container_id().is_some() {
                                    app.open_action_menu();
                                }
                            }
                            KeyCode::PageUp => {
                                app.select_prev();
                                app.auto_scroll = true;
                                if let Some(docker) = docker {
                                    load_container_data(app, docker).await;
                                }
                            }
                            KeyCode::PageDown => {
                                app.select_next();
                                app.auto_scroll = true;
                                if let Some(docker) = docker {
                                    load_container_data(app, docker).await;
                                }
                            }
                            _ => {}
                        },
                        Page::Settings => {
                            handle_settings_key(app, key.code);
                        }
                    }
                }
            }
        }

        // Refresh on each tick (dynamic rate from settings)
        let tick_rate = Duration::from_millis(app.settings.refresh_rate.as_millis());
        if last_tick.elapsed() >= tick_rate {
            if let Some(docker) = docker {
                // Preserve selection across refresh+sort
                let selected_id = app.selected_container_id().map(|s| s.to_string());
                app.containers = docker_client::list_containers(docker).await;
                sort_containers(&mut app.containers, app.settings.sort_by, &app.all_stats);
                // Restore selection by ID, or clamp
                if let Some(ref id) = selected_id {
                    if let Some(pos) = app.containers.iter().position(|c| c.id == *id) {
                        app.selected = pos;
                    }
                }
                if !app.containers.is_empty() {
                    app.selected = app.selected.min(app.containers.len() - 1);
                }

                // Background stats polling for all containers
                if app.settings.poll_all_containers {
                    for container in &app.containers {
                        if !container.status.contains("Up") {
                            continue;
                        }
                        let cid = container.id.clone();
                        if let Some(new_stats) = docker_client::fetch_stats(docker, &cid).await {
                            let history = app
                                .all_history
                                .entry(cid.clone())
                                .or_insert_with(app::StatsHistory::new);
                            let (cpu_pct, percpu_pcts) = history.push(&new_stats);
                            let mut stored = new_stats;
                            stored.cpu_percent = cpu_pct;
                            stored.percpu_percent = percpu_pcts;
                            app.all_stats.insert(cid, stored);
                        }
                    }
                    // Clean up entries for containers that no longer exist
                    let current_ids: std::collections::HashSet<String> =
                        app.containers.iter().map(|c| c.id.clone()).collect();
                    app.all_history.retain(|id, _| current_ids.contains(id));
                    app.all_stats.retain(|id, _| current_ids.contains(id));
                }

                // Refresh live data based on current page
                if let Some(ref id) = app.logs_container_id {
                    let id = id.clone();
                    match app.page {
                        Page::Detail => {
                            // No live refresh needed — detail is static
                        }
                        Page::Resources => {
                            if app.settings.poll_all_containers {
                                // Already polled above; sync to the single-container view
                                if let Some(h) = app.all_history.get(&id) {
                                    app.history = h.clone();
                                }
                                if let Some(s) = app.all_stats.get(&id) {
                                    app.stats = Some(s.clone());
                                }
                            } else {
                                let mut new_stats =
                                    docker_client::fetch_stats(docker, &id).await;
                                if let Some(ref s) = new_stats {
                                    let (cpu_pct, percpu_pcts) = app.history.push(s);
                                    if let Some(ref mut ns) = new_stats {
                                        ns.cpu_percent = cpu_pct;
                                        ns.percpu_percent = percpu_pcts;
                                    }
                                }
                                app.stats = new_stats;
                            }
                        }
                        Page::Logs => {
                            let log_lines = app.settings.log_buffer_size.as_usize();
                            app.logs = docker_client::fetch_logs(docker, &id, log_lines).await;
                        }
                        Page::List | Page::Settings => {}
                    }
                }
            }
            last_tick = std::time::Instant::now();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use app::{ContainerInfo, ContainerStats};

    // --- is_newer ---

    #[test]
    fn is_newer_patch_increment() {
        assert!(is_newer("1.0.3", "1.0.2"));
    }

    #[test]
    fn is_newer_minor_increment() {
        assert!(is_newer("1.1.0", "1.0.9"));
    }

    #[test]
    fn is_newer_major_increment() {
        assert!(is_newer("2.0.0", "1.9.9"));
    }

    #[test]
    fn is_newer_same_version_is_false() {
        assert!(!is_newer("1.0.2", "1.0.2"));
    }

    #[test]
    fn is_newer_older_version_is_false() {
        assert!(!is_newer("1.0.1", "1.0.2"));
    }

    // --- extract_json_string ---

    #[test]
    fn extract_json_string_basic() {
        let json = r#"{"tag_name": "v1.0.3", "name": "Release 1.0.3"}"#;
        assert_eq!(extract_json_string(json, "tag_name"), Some("v1.0.3".to_string()));
    }

    #[test]
    fn extract_json_string_with_whitespace_after_colon() {
        // GitHub API sometimes emits extra whitespace after the colon
        let json = r#"{"tag_name":  "v2.0.0"}"#;
        assert_eq!(extract_json_string(json, "tag_name"), Some("v2.0.0".to_string()));
    }

    #[test]
    fn extract_json_string_missing_key_returns_none() {
        let json = r#"{"other_key": "value"}"#;
        assert_eq!(extract_json_string(json, "tag_name"), None);
    }

    #[test]
    fn extract_json_string_picks_correct_key() {
        let json = r#"{"name": "the release", "tag_name": "v1.2.3", "body": "notes"}"#;
        assert_eq!(extract_json_string(json, "tag_name"), Some("v1.2.3".to_string()));
        assert_eq!(extract_json_string(json, "name"), Some("the release".to_string()));
    }

    // --- sort_containers ---

    fn make_container(id: &str, name: &str, status: &str, compose: Option<&str>) -> ContainerInfo {
        ContainerInfo {
            id: id.to_string(),
            name: name.to_string(),
            image: "img".to_string(),
            status: status.to_string(),
            compose_project: compose.map(|s| s.to_string()),
        }
    }

    fn make_stats(cpu: Option<f64>, mem: Option<u64>, block_read: Option<u64>, net_rx: Option<u64>) -> ContainerStats {
        ContainerStats {
            cpu_percent: cpu,
            mem_used: mem,
            block_read,
            net_rx,
            ..ContainerStats::default()
        }
    }

    #[test]
    fn sort_by_name_alphabetical() {
        let mut containers = vec![
            make_container("3", "zebra", "Up", None),
            make_container("1", "alpha", "Up", None),
            make_container("2", "mango", "Up", None),
        ];
        sort_containers(&mut containers, SortBy::Name, &HashMap::new());
        let names: Vec<&str> = containers.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["alpha", "mango", "zebra"]);
    }

    #[test]
    fn sort_by_name_is_case_insensitive() {
        let mut containers = vec![
            make_container("1", "Zebra", "Up", None),
            make_container("2", "apple", "Up", None),
        ];
        sort_containers(&mut containers, SortBy::Name, &HashMap::new());
        assert_eq!(containers[0].name, "apple");
    }

    #[test]
    fn sort_by_status_running_before_paused_before_stopped() {
        let mut containers = vec![
            make_container("1", "c1", "Exited (0)", None),
            make_container("2", "c2", "Up 2 hours (Paused)", None),
            make_container("3", "c3", "Up 5 minutes", None),
        ];
        sort_containers(&mut containers, SortBy::Status, &HashMap::new());
        assert_eq!(containers[0].id, "3"); // running
        assert_eq!(containers[1].id, "2"); // paused
        assert_eq!(containers[2].id, "1"); // stopped
    }

    #[test]
    fn sort_by_status_ties_broken_by_name() {
        let mut containers = vec![
            make_container("1", "zebra", "Up 1 hour", None),
            make_container("2", "alpha", "Up 2 hours", None),
        ];
        sort_containers(&mut containers, SortBy::Status, &HashMap::new());
        assert_eq!(containers[0].name, "alpha");
        assert_eq!(containers[1].name, "zebra");
    }

    #[test]
    fn sort_by_cpu_descending() {
        let mut containers = vec![
            make_container("a", "low", "Up", None),
            make_container("b", "high", "Up", None),
            make_container("c", "mid", "Up", None),
        ];
        let mut stats = HashMap::new();
        stats.insert("a".to_string(), make_stats(Some(10.0), None, None, None));
        stats.insert("b".to_string(), make_stats(Some(80.0), None, None, None));
        stats.insert("c".to_string(), make_stats(Some(40.0), None, None, None));
        sort_containers(&mut containers, SortBy::Cpu, &stats);
        let names: Vec<&str> = containers.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["high", "mid", "low"]);
    }

    #[test]
    fn sort_by_cpu_missing_stats_sorts_last() {
        let mut containers = vec![
            make_container("a", "no-stats", "Up", None),
            make_container("b", "has-stats", "Up", None),
        ];
        let mut stats = HashMap::new();
        stats.insert("b".to_string(), make_stats(Some(5.0), None, None, None));
        sort_containers(&mut containers, SortBy::Cpu, &stats);
        assert_eq!(containers[0].name, "has-stats");
        assert_eq!(containers[1].name, "no-stats");
    }

    #[test]
    fn sort_by_memory_descending() {
        let mut containers = vec![
            make_container("a", "small", "Up", None),
            make_container("b", "large", "Up", None),
        ];
        let mut stats = HashMap::new();
        stats.insert("a".to_string(), make_stats(None, Some(100), None, None));
        stats.insert("b".to_string(), make_stats(None, Some(900), None, None));
        sort_containers(&mut containers, SortBy::Memory, &stats);
        assert_eq!(containers[0].name, "large");
    }

    #[test]
    fn sort_by_compose_project_groups_then_by_name() {
        let mut containers = vec![
            make_container("1", "web",   "Up", Some("proj-b")),
            make_container("2", "alpha", "Up", Some("proj-a")),
            make_container("3", "db",    "Up", Some("proj-a")),
            make_container("4", "solo",  "Up", None),
        ];
        sort_containers(&mut containers, SortBy::ComposeProject, &HashMap::new());
        // proj-a containers first (alphabetical project), then proj-b, then no-project last
        assert_eq!(containers[0].name, "alpha");
        assert_eq!(containers[1].name, "db");
        assert_eq!(containers[2].name, "web");
        assert_eq!(containers[3].name, "solo");
    }
}
