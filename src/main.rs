mod aggregation;
mod app;
mod docker_client;
mod settings;
mod theme;
mod ui;

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{ContainerAction, Page};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let result = match action {
        ContainerAction::Start => docker_client::start_container(docker, &id).await,
        ContainerAction::Stop => docker_client::stop_container(docker, &id).await,
        ContainerAction::Restart => docker_client::restart_container(docker, &id).await,
        ContainerAction::Pause => docker_client::pause_container(docker, &id).await,
        ContainerAction::Unpause => docker_client::unpause_container(docker, &id).await,
        ContainerAction::Kill => docker_client::kill_container(docker, &id).await,
        ContainerAction::Remove => docker_client::remove_container(docker, &id).await,
        ContainerAction::Details | ContainerAction::Logs => unreachable!(),
    };

    match result {
        Ok(()) => app.set_status(format!("{}: {}", action.label(), name)),
        Err(e) => app.set_status(format!("Error: {}", e)),
    }
}

/// Handle key input on the Settings page.
fn handle_settings_key(app: &mut app::App, key: KeyCode) {
    match key {
        KeyCode::Esc | KeyCode::Char('s') => {
            // Return to previous page
            app.set_page(app.previous_page.unwrap_or(Page::List));
            app.previous_page = None;
        }
        KeyCode::Up => {
            app.settings_selection = app.settings_selection.saturating_sub(1);
        }
        KeyCode::Down => {
            if app.settings_selection < settings::Settings::ROW_COUNT - 1 {
                app.settings_selection += 1;
            }
        }
        KeyCode::Right => {
            match app.settings_selection {
                0 => app.settings.aggregation_mode = app.settings.aggregation_mode.next(),
                1 => app.settings.aggregation_window.increment(),
                2 => app.settings.theme = app.settings.theme.next(),
                3 => app.settings.refresh_rate = app.settings.refresh_rate.next(),
                4 => app.settings.log_buffer_size = app.settings.log_buffer_size.next(),
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
                _ => {}
            }
            app.settings.save();
        }
        KeyCode::Left => {
            match app.settings_selection {
                0 => app.settings.aggregation_mode = app.settings.aggregation_mode.prev(),
                1 => app.settings.aggregation_window.decrement(),
                2 => app.settings.theme = app.settings.theme.prev(),
                3 => app.settings.refresh_rate = app.settings.refresh_rate.prev(),
                4 => app.settings.log_buffer_size = app.settings.log_buffer_size.prev(),
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
                _ => {}
            }
            app.settings.save();
        }
        _ => {}
    }
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

        // Draw
        terminal.draw(|f| ui::draw(f, app))?;

        // Poll for input (non-blocking, 50ms timeout)
        let timeout = Duration::from_millis(50);
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                // Action menu takes priority when open
                if app.action_menu.is_some() {
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
                app.containers = docker_client::list_containers(docker).await;
                // Clamp selection if containers were removed
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
