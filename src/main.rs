mod aggregation;
mod app;
mod docker_client;
mod docker_host;
mod icons;
mod settings;
mod theme;
mod ui;

use std::io;
use std::process::Command;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    self, BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use tokio::sync::mpsc;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{ContainerAction, ContainerInfo, Page};
use settings::SortBy;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILD_DATE: &str = env!("BUILD_DATE");
const GIT_HASH: &str = env!("GIT_HASH");
const GIT_BRANCH: &str = env!("GIT_BRANCH");
const BUILD_TARGET: &str = env!("BUILD_TARGET");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Handle CLI arguments before entering the TUI
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "--version" | "-V" => {
                println!(
                    "wtop {} (built {}, {}, {}/{})",
                    VERSION, BUILD_DATE, BUILD_TARGET, GIT_BRANCH, GIT_HASH
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
    let (mut docker, docker_host, connect_error) = match docker_client::connect() {
        Ok((d, endpoint)) => (Some(d), Some(endpoint.host), None),
        Err(e) => (None, None, Some(e)),
    };

    // Terminal setup. `try_init` enables raw mode + the alternate screen and installs a
    // panic hook that restores the terminal first, so a panic never leaves the user's
    // shell stuck in raw mode.
    let mut terminal = ratatui::try_init()?;

    let (settings, settings_warning) = settings::Settings::load_with_warning();
    let themes = theme::load_all();
    let mut app = app::App::new(settings, themes);
    app.docker_host = docker_host;
    if let Some(msg) = settings_warning.or(connect_error) {
        app.info_popup = Some(msg);
    }

    let result = run_loop(&mut terminal, &mut app, &mut docker).await;

    // Terminal teardown (always runs, even when run_loop returned an error)
    ratatui::restore();

    result
}

/// Load static detail + initial stats + logs for the currently selected container.
async fn load_container_data(app: &mut app::App, docker: &bollard::Docker) {
    if let Some(id) = app.selected_container_id() {
        let id = id.to_string();
        app.detail = docker_client::inspect_container(docker, &id).await;

        if app.settings.poll_all_containers {
            // poll_all mode: history/stats live in all_history/all_stats;
            // only fetch if container has no entry yet
            if !app.all_stats.contains_key(&id) {
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
    docker: &mut Option<bollard::Docker>,
    results: &mpsc::UnboundedSender<ActionOutcome>,
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

    // Exec opens a shell selection submenu; actual spawn happens in run_loop
    if action == ContainerAction::Exec {
        app.exec_shell_menu = Some(app::ExecShellMenu::new(id));
        return;
    }

    let pending = app::PendingAction { action, container_id: id, container_name: name };
    if action.needs_confirmation() {
        app.confirm_action = Some(pending);
    } else {
        spawn_action(app, docker, results, pending);
    }
}

/// Outcome of a background lifecycle action, sent back to the UI loop.
struct ActionOutcome {
    pending: app::PendingAction,
    result: Result<(), String>,
}

/// Run a lifecycle action on a background task so the UI keeps drawing and handling
/// input. (A Stop waits up to the container's stop timeout, 10s by default; awaiting it
/// inline froze the whole TUI for that long.)
fn spawn_action(
    app: &mut app::App,
    docker: &bollard::Docker,
    results: &mpsc::UnboundedSender<ActionOutcome>,
    pending: app::PendingAction,
) {
    if let Some(running) = app.actions_in_flight.get(&pending.container_id) {
        app.set_status(format!(
            "{} {} is still in progress",
            running.action.progress_label(),
            running.container_name
        ));
        return;
    }
    app.set_status(format!(
        "{} {}...",
        pending.action.progress_label(),
        pending.container_name
    ));
    app.actions_in_flight.insert(pending.container_id.clone(), pending.clone());

    let docker = docker.clone();
    let results = results.clone();
    tokio::spawn(async move {
        let id = pending.container_id.as_str();
        let result = match pending.action {
            ContainerAction::Start => docker_client::start_container(&docker, id).await,
            ContainerAction::Stop => docker_client::stop_container(&docker, id).await,
            ContainerAction::Restart => docker_client::restart_container(&docker, id).await,
            ContainerAction::Pause => docker_client::pause_container(&docker, id).await,
            ContainerAction::Unpause => docker_client::unpause_container(&docker, id).await,
            ContainerAction::Kill => docker_client::kill_container(&docker, id).await,
            ContainerAction::Remove => docker_client::remove_container(&docker, id).await,
            ContainerAction::Details | ContainerAction::Logs | ContainerAction::Exec => {
                Err("not a lifecycle action".to_string())
            }
        };
        // The receiver only disappears when wtop is quitting; nothing to report then.
        let _ = results.send(ActionOutcome { pending, result });
    });
}

/// Apply a finished background action to the app state.
fn apply_action_outcome(app: &mut app::App, outcome: ActionOutcome) {
    let ActionOutcome { pending, result } = outcome;
    app.actions_in_flight.remove(&pending.container_id);
    match result {
        Ok(()) => app.set_status(format!("{}: {}", pending.action.label(), pending.container_name)),
        Err(e) => app.set_status(format!(
            "{} {} failed: {}",
            pending.action.label(),
            pending.container_name,
            e
        )),
    }
}

/// When a per-container page is open but its container no longer exists (removed from
/// wtop or elsewhere), go back to the list instead of showing stale data under another
/// container's name.
fn leave_page_if_container_gone(app: &mut app::App) {
    if !matches!(app.page, Page::Detail | Page::Resources | Page::Logs) {
        return;
    }
    let Some(id) = app.logs_container_id.as_deref() else {
        return;
    };
    if !app.containers.iter().any(|c| c.id == id) {
        app.logs_container_id = None;
        app.detail = None;
        app.stats = None;
        app.logs.clear();
        app.log_search_active = false;
        app.set_page(Page::List);
        app.set_status("Container no longer exists".to_string());
    }
}

/// Exit codes `docker exec` uses when the command could not be started at all
/// (125: docker/daemon error, 126: not executable, 127: not found).
fn exec_failed_to_start(code: Option<i32>) -> bool {
    matches!(code, Some(125..=127))
}

/// Build the `docker exec` command for `shell` in `container_id`.
///
/// `DOCKER_HOST` is set to the endpoint wtop itself is connected to, so the CLI cannot
/// pick a different daemon through its own context and fail with "No such container".
/// (DOCKER_HOST takes precedence over DOCKER_CONTEXT in the CLI; we also drop the latter.)
fn exec_command(docker_host: Option<&str>, container_id: &str, shell: &app::ExecShell) -> Command {
    let mut cmd = Command::new("docker");
    if let Some(host) = docker_host {
        cmd.env("DOCKER_HOST", host).env_remove("DOCKER_CONTEXT");
    }
    cmd.args(["exec", "-it", "-e", "TERM=xterm-256color", "-e", "COLORTERM=truecolor"])
        .arg(container_id)
        .args(shell.argv);
    cmd
}

/// Suspend the TUI, run an interactive shell in the container, then restore the TUI.
///
/// If the shell could not be started (missing binary, CLI missing, wrong daemon), the
/// error printed by `docker` stays on screen until the user presses Enter; previously
/// it was wiped instantly when the alternate screen came back.
fn run_exec(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut app::App,
    container_id: &str,
    shell: &app::ExecShell,
) -> io::Result<()> {
    terminal::disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    io::stdout().execute(terminal::Clear(terminal::ClearType::All))?;
    terminal.show_cursor()?;

    let started = std::time::Instant::now();
    let status = exec_command(app.docker_host.as_deref(), container_id, shell).status();

    let failure = match &status {
        Ok(s) if s.success() => None,
        // A non-zero exit right away almost always means the session never started
        // (e.g. "No such container"); a long session ending non-zero is just the
        // shell's last exit status and needs no pause.
        Ok(s) if exec_failed_to_start(s.code()) || started.elapsed() < Duration::from_secs(2) => {
            Some(format!(
                "Could not start '{}' in the container (exit code {}). \
                 The image may not include it; try another shell.",
                shell.name,
                s.code().unwrap_or(-1)
            ))
        }
        Ok(_) => None,
        Err(e) if e.kind() == io::ErrorKind::NotFound => Some(
            "Exec Shell needs the `docker` CLI, but it was not found in PATH.".to_string(),
        ),
        Err(e) => Some(format!("Failed to run `docker exec`: {}", e)),
    };

    if let Some(msg) = &failure {
        use std::io::Write;
        println!("\n[wtop] {}", msg);
        print!("[wtop] Press Enter to return to wtop...");
        let _ = io::stdout().flush();
        let mut line = String::new();
        let _ = io::stdin().read_line(&mut line);
    }

    app.set_status(match (&status, &failure) {
        (_, Some(_)) => format!("Exec ({}): failed to start", shell.name),
        (Ok(s), None) if s.success() => format!("Exec ({}): session ended", shell.name),
        (Ok(s), None) => format!("Exec ({}): exited with {}", shell.name, s.code().unwrap_or(-1)),
        (Err(_), None) => unreachable!("spawn errors always set a failure message"),
    });

    terminal::enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    app.needs_clear = true;
    Ok(())
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
const LEFT_COL: &[usize] = &[0, 1, 2, 3, 4, 5, 22, 23, 19, 18];
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
        2 => {
            let n = app.themes.len();
            if n > 0 {
                let cur = app.themes.iter().position(|t| t.id.eq_ignore_ascii_case(&app.settings.theme)).unwrap_or(0);
                let next = if forward { (cur + 1) % n } else { (cur + n - 1) % n };
                app.settings.theme = app.themes[next].id.clone();
            }
        }
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
        22 => app.settings.icon_style = if forward {
            app.settings.icon_style.next()
        } else {
            app.settings.icon_style.prev()
        },
        23 => app.settings.confirm_quit = !app.settings.confirm_quit,
        _ => {}
    }
    app.settings.save();
    if matches!(app.settings_selection, 14..=17) {
        check_poll_all_hint(app);
    }
}

/// Trigger quit — shows confirm dialog if the setting is on.
fn try_quit(app: &mut app::App) {
    if app.settings.confirm_quit {
        app.quit_confirm = true;
    } else {
        app.quit();
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

/// Detect how wtop was installed based on the binary's path.
enum InstallMethod {
    Homebrew,
    Cargo,
    Binary,
}

fn detect_install_method() -> InstallMethod {
    let exe_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    if exe_path.contains("/Cellar/") || exe_path.contains("/homebrew/") {
        InstallMethod::Homebrew
    } else if exe_path.contains(".cargo/bin") {
        InstallMethod::Cargo
    } else {
        InstallMethod::Binary
    }
}

const REPO: &str = "danielme85/wtop";

/// Name of the release asset built for this platform (matches release.yml).
fn platform_asset_name() -> Result<String, String> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        other => return Err(format!("no prebuilt binary for OS '{}'", other)),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => return Err(format!("no prebuilt binary for architecture '{}'", other)),
    };
    Ok(format!("wtop-{}-{}", os, arch))
}

/// The parts of GitHub's "latest release" response that the updater needs.
#[derive(Debug, PartialEq)]
struct ReleaseInfo {
    tag: String,
    /// (asset name, download URL)
    assets: Vec<(String, String)>,
}

fn parse_release(json: &str) -> Result<ReleaseInfo, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("invalid GitHub API response: {}", e))?;
    let tag = v["tag_name"]
        .as_str()
        .ok_or("GitHub API response has no tag_name")?
        .to_string();
    let assets = v["assets"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|asset| {
                    Some((
                        asset["name"].as_str()?.to_string(),
                        asset["browser_download_url"].as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(ReleaseInfo { tag, assets })
}

impl ReleaseInfo {
    /// Download URL of `name`, only if it points at this repo's release downloads.
    fn asset_url(&self, name: &str) -> Option<&str> {
        let prefix = format!("https://github.com/{}/releases/download/", REPO);
        self.assets
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, url)| url.as_str())
            .filter(|url| url.starts_with(&prefix))
    }
}

/// Find the expected SHA-256 (lowercase hex) for `asset` in a `sha256sum`-style file.
fn expected_sha256(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        // sha256sum marks binary-mode entries with a leading '*'.
        let name = parts.next()?.trim_start_matches('*');
        (name == asset && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(data).iter().map(|b| format!("{:02x}", b)).collect()
}

/// Fetch a URL with curl (HTTPS only, fail on HTTP errors, follow redirects).
fn curl_get(url: &str, extra_args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("curl")
        .args(["-fsSL", "--proto", "=https", "--tlsv1.2"])
        .args(extra_args)
        .arg(url)
        .output()
        .map_err(|e| format!("curl not found: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "download failed for {} (curl exit {:?})",
            url,
            output.status.code()
        ));
    }
    Ok(output.stdout)
}

/// Check GitHub for a newer release and install it in place.
///
/// The binary is downloaded from the tagged GitHub release and verified against the
/// release's SHA256SUMS before it replaces the running executable. (Previously this
/// piped an install script from the `main` branch straight into `sh`, unverified.)
fn run_update(force: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("wtop {} — checking for updates...", VERSION);

    let body = curl_get(
        &format!("https://api.github.com/repos/{}/releases/latest", REPO),
        &["-H", "Accept: application/vnd.github+json"],
    )?;
    let release = parse_release(&String::from_utf8_lossy(&body))?;
    let latest_version = release.tag.trim_start_matches('v');
    println!("Latest release: {}", latest_version);

    if !force && !is_newer(latest_version, VERSION) {
        println!(
            "Already up to date ({}). Use --reinstall to force a clean reinstall.",
            VERSION
        );
        return Ok(());
    }

    match detect_install_method() {
        InstallMethod::Homebrew => {
            println!("\nThis binary was installed via Homebrew.");
            println!("To update, run:\n");
            println!("  brew upgrade wtop");
            return Ok(());
        }
        InstallMethod::Cargo => {
            println!("\nThis binary was installed via Cargo.");
            println!("To update, run:\n");
            println!("  cargo install wtop");
            return Ok(());
        }
        InstallMethod::Binary => {}
    }

    let asset = platform_asset_name()?;
    let releases_page = format!("https://github.com/{}/releases", REPO);
    let bin_url = release.asset_url(&asset).ok_or_else(|| {
        format!("release {} has no asset '{}'. See {}", release.tag, asset, releases_page)
    })?;
    let sums_url = release.asset_url("SHA256SUMS").ok_or_else(|| {
        format!(
            "release {} does not publish SHA256SUMS, so the download cannot be verified. \
             Refusing to install it; download manually from {}",
            release.tag, releases_page
        )
    })?;

    if force {
        println!("Reinstalling wtop {}...", latest_version);
    } else {
        println!("Updating {} → {}...", VERSION, latest_version);
    }

    let sums = String::from_utf8_lossy(&curl_get(sums_url, &[])?).into_owned();
    let expected = expected_sha256(&sums, &asset)
        .ok_or_else(|| format!("SHA256SUMS has no entry for {}", asset))?;
    let binary = curl_get(bin_url, &[])?;
    let actual = sha256_hex(&binary);
    if actual != expected {
        return Err(format!(
            "checksum mismatch for {}: expected {}, got {}. Not installing.",
            asset, expected, actual
        )
        .into());
    }
    println!("Verified SHA-256 {}", actual);

    install_binary(&binary, latest_version)?;
    println!("\nSuccessfully installed wtop {}.", latest_version);
    Ok(())
}

/// Atomically replace the running executable with `binary` after a smoke test.
fn install_binary(binary: &[u8], expected_version: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let exe = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|e| format!("cannot locate the running wtop binary: {}", e))?;
    let dir = exe.parent().ok_or("running binary has no parent directory")?;
    // Same directory as the target so the final rename is atomic (same filesystem).
    let tmp = dir.join(format!(".wtop-update-{}", std::process::id()));
    let cleanup = |e: String| {
        let _ = std::fs::remove_file(&tmp);
        e
    };

    std::fs::write(&tmp, binary).map_err(|e| {
        format!("cannot write to {} ({}); check its permissions", dir.display(), e)
    })?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| cleanup(format!("cannot make new binary executable: {}", e)))?;

    // Smoke test: the new binary must run on this machine and report the expected version.
    let out = Command::new(&tmp)
        .arg("--version")
        .output()
        .map_err(|e| cleanup(format!("new binary does not run on this system: {}", e)))?;
    let reported = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() || !reported.contains(expected_version) {
        return Err(cleanup(format!(
            "new binary failed its self-check (reported {:?}); not installing",
            reported.trim()
        )));
    }

    std::fs::rename(&tmp, &exe)
        .map_err(|e| cleanup(format!("cannot replace {}: {}", exe.display(), e)))?;
    Ok(())
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
    docker: &mut Option<bollard::Docker>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_tick = std::time::Instant::now();
    // Track consecutive failures to throttle reconnect attempts.
    let mut consecutive_failures: u32 = 0;

    // Background lifecycle actions report back through this channel.
    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<ActionOutcome>();
    let mut refresh_now = false;

    while app.running {
        // Apply finished background actions, and refresh right away so the list
        // reflects the new container state without waiting for the next tick.
        while let Ok(outcome) = action_rx.try_recv() {
            apply_action_outcome(app, outcome);
            refresh_now = true;
        }

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
                // Quit-confirm dialog takes top priority
                if app.quit_confirm {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('y') => app.quit(),
                        _ => app.quit_confirm = false,
                    }
                // Destructive-action confirmation: only 'y' proceeds
                } else if let Some(pending) = app.confirm_action.take() {
                    if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                        match docker {
                            Some(d) => spawn_action(app, d, &action_tx, pending),
                            None => app.set_status("No Docker connection".to_string()),
                        }
                    } else {
                        app.set_status(format!("{} cancelled", pending.action.label()));
                    }
                // Info popup takes priority when open
                } else if app.info_popup.is_some() {
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter => {
                            app.info_popup = None;
                        }
                        _ => {}
                    }
                // Shell selection submenu (shown after picking Exec from action menu)
                } else if app.exec_shell_menu.is_some() {
                    match key.code {
                        KeyCode::Esc => { app.exec_shell_menu = None; }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if let Some(ref mut m) = app.exec_shell_menu { m.select_prev(); }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if let Some(ref mut m) = app.exec_shell_menu { m.select_next(); }
                        }
                        KeyCode::Enter => {
                            if let Some(m) = app.exec_shell_menu.take() {
                                run_exec(terminal, app, &m.container_id, m.selected_shell())?;
                            }
                        }
                        _ => {}
                    }
                // Action menu takes priority when open
                } else if app.action_menu.is_some() {
                    match key.code {
                        KeyCode::Esc => app.close_action_menu(),
                        KeyCode::Up | KeyCode::Char('k') => {
                            if let Some(ref mut menu) = app.action_menu {
                                menu.select_prev();
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
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
                                handle_action(app, docker, &action_tx, action).await;
                            }
                        }
                        _ => {}
                    }
                } else if key.code == KeyCode::Char('q') && !app.is_typing() {
                    try_quit(app);
                } else if key.code == KeyCode::Esc && app.page != Page::Settings {
                    // Esc: go back one level, or quit from the list
                    match app.page {
                        Page::List => try_quit(app),
                        Page::Detail => app.set_page(Page::List),
                        Page::Resources => app.set_page(Page::Detail),
                        Page::Logs => {
                            if app.log_search_active {
                                app.log_search_active = false;
                            } else {
                                app.set_page(Page::Resources);
                                app.auto_scroll = true;
                            }
                        }
                        Page::Settings => unreachable!(),
                    }
                } else if key.code == KeyCode::Char('s')
                    && app.page != Page::Settings
                    && !app.is_typing()
                {
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
                            KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
                            KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                            KeyCode::Home => app.selected = 0,
                            KeyCode::End => {
                                app.selected = app.containers.len().saturating_sub(1);
                            }
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
                            KeyCode::Up | KeyCode::Char('k') => {
                                app.detail_scroll = app.detail_scroll.saturating_sub(1);
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
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
                            KeyCode::Home => {
                                app.selected = 0;
                                if let Some(docker) = docker {
                                    load_container_data(app, docker).await;
                                }
                            }
                            KeyCode::End => {
                                app.selected = app.containers.len().saturating_sub(1);
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
                            KeyCode::Home => {
                                app.selected = 0;
                                if let Some(docker) = docker {
                                    load_container_data(app, docker).await;
                                }
                            }
                            KeyCode::End => {
                                app.selected = app.containers.len().saturating_sub(1);
                                if let Some(docker) = docker {
                                    load_container_data(app, docker).await;
                                }
                            }
                            _ => {}
                        },
                        Page::Logs => {
                            // Search input mode takes priority
                            if app.log_search_active {
                                match key.code {
                                    KeyCode::Esc | KeyCode::Enter => {
                                        app.log_search_active = false;
                                    }
                                    KeyCode::Backspace => {
                                        app.log_search_query.pop();
                                    }
                                    KeyCode::Char(c)
                                        if !key.modifiers.intersects(
                                            KeyModifiers::CONTROL | KeyModifiers::ALT,
                                        ) =>
                                    {
                                        app.log_search_query.push(c);
                                    }
                                    _ => {}
                                }
                            } else {
                                match key.code {
                                    KeyCode::Char('/') => {
                                        app.log_search_active = true;
                                        app.log_search_query.clear();
                                    }
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
                                    KeyCode::Up | KeyCode::Char('k') => {
                                        app.auto_scroll = false;
                                        app.log_scroll = app.log_scroll.saturating_sub(1);
                                    }
                                    KeyCode::Down | KeyCode::Char('j') => {
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
                                    KeyCode::Home => {
                                        app.selected = 0;
                                        app.auto_scroll = true;
                                        if let Some(docker) = docker {
                                            load_container_data(app, docker).await;
                                        }
                                    }
                                    KeyCode::End => {
                                        app.selected = app.containers.len().saturating_sub(1);
                                        app.auto_scroll = true;
                                        if let Some(docker) = docker {
                                            load_container_data(app, docker).await;
                                        }
                                    }
                                    _ => {}
                                }
                            }
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
        if refresh_now || last_tick.elapsed() >= tick_rate {
            refresh_now = false;
            // Attempt reconnection if Docker is disconnected
            if docker.is_none() && consecutive_failures < 3 {
                if let Ok((d, endpoint)) = docker_client::connect() {
                    *docker = Some(d);
                    app.docker_host = Some(endpoint.host);
                    app.set_status("Docker reconnected".to_string());
                    consecutive_failures = 0;
                } else {
                    consecutive_failures += 1;
                }
            }
            if let Some(ref docker_conn) = docker {
                // Preserve selection across refresh+sort
                let selected_id = app.selected_container_id().map(|s| s.to_string());
                let new_containers = docker_client::list_containers(docker_conn).await;

                // Detect disconnect: had containers but now empty, and ping fails
                if new_containers.is_empty()
                    && !app.containers.is_empty()
                    && !docker_client::ping(docker_conn).await
                {
                    app.set_status("Docker connection lost — attempting to reconnect...".to_string());
                    *docker = None;
                    consecutive_failures = 0;
                    last_tick = std::time::Instant::now();
                    continue;
                }
                consecutive_failures = 0;
                // Only re-sort if the container set or statuses changed
                let changed = new_containers.len() != app.containers.len()
                    || new_containers.iter().zip(app.containers.iter()).any(|(n, o)| {
                        n.id != o.id || n.status != o.status
                    });
                app.containers = new_containers;
                if changed {
                    sort_containers(&mut app.containers, app.settings.sort_by, &app.all_stats);
                }
                // Restore selection by ID, or clamp
                if let Some(ref id) = selected_id {
                    if let Some(pos) = app.containers.iter().position(|c| c.id == *id) {
                        app.selected = pos;
                    }
                }
                if !app.containers.is_empty() {
                    app.selected = app.selected.min(app.containers.len() - 1);
                }
                leave_page_if_container_gone(app);

                // Background stats polling for all containers
                if app.settings.poll_all_containers {
                    for container in &app.containers {
                        if !container.status.contains("Up") {
                            continue;
                        }
                        let cid = &container.id;
                        if let Some(new_stats) = docker_client::fetch_stats(docker_conn, cid).await {
                            let history = app
                                .all_history
                                .entry(cid.clone())
                                .or_insert_with(app::StatsHistory::new);
                            let (cpu_pct, percpu_pcts) = history.push(&new_stats);
                            let mut stored = new_stats;
                            stored.cpu_percent = cpu_pct;
                            stored.percpu_percent = percpu_pcts;
                            app.all_stats.insert(cid.clone(), stored);
                        }
                    }
                    // Clean up entries for containers that no longer exist
                    let current_ids: std::collections::HashSet<&str> =
                        app.containers.iter().map(|c| c.id.as_str()).collect();
                    app.all_history.retain(|id, _| current_ids.contains(id.as_str()));
                    app.all_stats.retain(|id, _| current_ids.contains(id.as_str()));
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
                                // Already polled above; UI reads from all_history/all_stats directly
                            } else {
                                let mut new_stats =
                                    docker_client::fetch_stats(docker_conn, &id).await;
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
                            app.logs = docker_client::fetch_logs(docker_conn, &id, log_lines).await;
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

    // --- destructive-action confirmation & background actions ---

    fn test_app() -> app::App {
        app::App::new(settings::Settings::default(), vec![theme::builtin_default()])
    }

    fn pending(action: ContainerAction, id: &str) -> app::PendingAction {
        app::PendingAction { action, container_id: id.into(), container_name: format!("name-{id}") }
    }

    fn offline_docker() -> bollard::Docker {
        // Client construction is lazy; nothing connects to this socket.
        bollard::Docker::connect_with_http("tcp://127.0.0.1:9", 1, bollard::API_DEFAULT_VERSION)
            .unwrap()
    }

    #[test]
    fn only_kill_and_remove_need_confirmation() {
        assert!(ContainerAction::Kill.needs_confirmation());
        assert!(ContainerAction::Remove.needs_confirmation());
        for a in [ContainerAction::Start, ContainerAction::Stop, ContainerAction::Restart,
                  ContainerAction::Pause, ContainerAction::Unpause] {
            assert!(!a.needs_confirmation(), "{:?}", a);
        }
    }

    #[tokio::test]
    async fn remove_from_menu_asks_for_confirmation_instead_of_running() {
        let mut app = test_app();
        app.containers = vec![make_container("c1", "web", "Up 1 hour", None)];
        let mut docker = Some(offline_docker());
        let (tx, mut rx) = mpsc::unbounded_channel();
        handle_action(&mut app, &mut docker, &tx, ContainerAction::Remove).await;
        let confirm = app.confirm_action.as_ref().expect("confirmation dialog shown");
        assert_eq!(confirm.container_id, "c1");
        assert!(app.actions_in_flight.is_empty(), "nothing may run before 'y'");
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn second_action_on_busy_container_is_refused() {
        let mut app = test_app();
        app.actions_in_flight.insert("c1".into(), pending(ContainerAction::Stop, "c1"));
        let (tx, _rx) = mpsc::unbounded_channel();
        spawn_action(&mut app, &offline_docker(), &tx, pending(ContainerAction::Restart, "c1"));
        let (msg, _) = app.status_message.as_ref().unwrap();
        assert!(msg.contains("still in progress"), "{msg}");
        assert_eq!(app.actions_in_flight["c1"].action, ContainerAction::Stop);
    }

    #[test]
    fn finished_action_clears_in_flight_and_reports() {
        let mut app = test_app();
        app.actions_in_flight.insert("c1".into(), pending(ContainerAction::Stop, "c1"));
        apply_action_outcome(&mut app, ActionOutcome {
            pending: pending(ContainerAction::Stop, "c1"),
            result: Err("boom".into()),
        });
        assert!(app.actions_in_flight.is_empty());
        assert!(app.status_message.as_ref().unwrap().0.contains("failed: boom"));
    }

    #[test]
    fn detail_page_closes_when_its_container_disappears() {
        let mut app = test_app();
        app.containers = vec![make_container("c2", "other", "Up", None)];
        app.page = Page::Logs;
        app.logs_container_id = Some("c1".into());
        app.logs = vec!["stale".into()];
        leave_page_if_container_gone(&mut app);
        assert!(app.page == Page::List);
        assert!(app.logs.is_empty() && app.logs_container_id.is_none());

        // Still present -> stay on the page.
        app.page = Page::Detail;
        app.logs_container_id = Some("c2".into());
        leave_page_if_container_gone(&mut app);
        assert!(app.page == Page::Detail);
    }

    // --- exec ---

    fn exec_args(cmd: &Command) -> Vec<String> {
        cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn exec_uses_bare_shell_names_resolved_by_container_path() {
        let bash = app::EXEC_SHELLS.iter().find(|s| s.name == "bash").unwrap();
        let cmd = exec_command(None, "abc123", bash);
        assert_eq!(exec_args(&cmd).last().unwrap(), "bash");
        assert!(app::EXEC_SHELLS.iter().all(|s| !s.argv[0].starts_with('/')));
    }

    #[test]
    fn exec_auto_is_default_and_falls_back_to_sh() {
        let auto = &app::EXEC_SHELLS[0];
        assert_eq!(auto.name, "auto");
        assert_eq!(auto.argv[0], "sh");
        assert!(auto.argv[2].contains("exec bash") && auto.argv[2].ends_with("exec sh"));
    }

    #[test]
    fn exec_targets_same_daemon_as_wtop() {
        let shell = &app::EXEC_SHELLS[0];
        let cmd = exec_command(Some("unix:///run/user/1000/docker.sock"), "abc123", shell);
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(envs.contains(&(
            std::ffi::OsStr::new("DOCKER_HOST"),
            Some(std::ffi::OsStr::new("unix:///run/user/1000/docker.sock"))
        )));
        assert!(envs.contains(&(std::ffi::OsStr::new("DOCKER_CONTEXT"), None)));
        let args = exec_args(&cmd);
        assert_eq!(&args[..2], ["exec", "-it"]);
        assert!(args.contains(&"abc123".to_string()));
    }

    #[test]
    fn exec_start_failure_codes() {
        assert!(exec_failed_to_start(Some(126)));
        assert!(exec_failed_to_start(Some(127)));
        assert!(!exec_failed_to_start(Some(1)));
        assert!(!exec_failed_to_start(Some(0)));
        assert!(!exec_failed_to_start(None));
    }

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

    // --- self-update ---

    const RELEASE_JSON: &str = r#"{
        "tag_name": "v1.4.0",
        "name": "Release 1.4.0",
        "assets": [
            {"name": "wtop-linux-amd64",
             "browser_download_url": "https://github.com/danielme85/wtop/releases/download/v1.4.0/wtop-linux-amd64"},
            {"name": "SHA256SUMS",
             "browser_download_url": "https://github.com/danielme85/wtop/releases/download/v1.4.0/SHA256SUMS"},
            {"name": "wtop-darwin-arm64",
             "browser_download_url": "https://evil.example.com/wtop-darwin-arm64"}
        ]
    }"#;

    #[test]
    fn parse_release_reads_tag_and_assets() {
        let r = parse_release(RELEASE_JSON).unwrap();
        assert_eq!(r.tag, "v1.4.0");
        assert_eq!(r.assets.len(), 3);
        assert!(r.asset_url("wtop-linux-amd64").unwrap().ends_with("/v1.4.0/wtop-linux-amd64"));
        assert!(r.asset_url("SHA256SUMS").is_some());
    }

    #[test]
    fn asset_url_rejects_urls_outside_this_repo() {
        let r = parse_release(RELEASE_JSON).unwrap();
        assert_eq!(r.asset_url("wtop-darwin-arm64"), None);
        assert_eq!(r.asset_url("missing"), None);
    }

    #[test]
    fn parse_release_without_tag_is_an_error() {
        assert!(parse_release(r#"{"message": "API rate limit exceeded"}"#).is_err());
        assert!(parse_release("not json").is_err());
    }

    #[test]
    fn expected_sha256_finds_matching_entry() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let sums = format!("{a}  wtop-linux-amd64\n{b} *wtop-darwin-arm64\n");
        assert_eq!(expected_sha256(&sums, "wtop-linux-amd64"), Some(a));
        assert_eq!(expected_sha256(&sums, "wtop-darwin-arm64"), Some("b".repeat(64)));
        assert_eq!(expected_sha256(&sums, "wtop-linux-arm64"), None);
        // A prefix of another asset name must not match.
        assert_eq!(expected_sha256(&sums, "wtop-linux"), None);
        assert_eq!(expected_sha256("xyz  wtop-linux-amd64", "wtop-linux-amd64"), None);
    }

    #[test]
    fn sha256_hex_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn platform_asset_name_matches_release_naming() {
        let name = platform_asset_name().unwrap();
        assert!(name.starts_with("wtop-linux-") || name.starts_with("wtop-darwin-"));
        assert!(name.ends_with("-amd64") || name.ends_with("-arm64"));
    }

    // --- sort_containers ---

    fn make_container(id: &str, name: &str, status: &str, compose: Option<&str>) -> ContainerInfo {
        ContainerInfo {
            id: id.to_string(),
            name: name.to_string(),
            image: "img".to_string(),
            status: status.to_string(),
            compose_project: compose.map(|s| s.to_string()),
            health: None,
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
