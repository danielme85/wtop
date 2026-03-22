use std::collections::{HashMap, VecDeque};

use crate::settings::Settings;
use crate::theme::Theme;

/// Max number of history samples to keep for sparkline graphs.
/// Sized to cover wide terminals (up to ~300 columns).
pub const HISTORY_LEN: usize = 300;

/// Information about a single Docker container.
#[derive(Clone, Debug)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
}

/// Static container information from docker inspect (does not change while running).
#[derive(Clone, Debug, Default)]
pub struct ContainerDetail {
    pub full_id: String,
    pub image: String,
    pub command: String,
    pub created: String,
    pub state: String,
    pub env: Vec<String>,
    pub ports: Vec<String>,
    pub volumes: Vec<String>,
    pub networks: Vec<String>,
    pub compose: Option<ComposeInfo>,
    pub host_network: bool,
}

/// Docker Compose metadata extracted from container labels.
#[derive(Clone, Debug)]
pub struct ComposeInfo {
    pub project: String,
    pub service: String,
    pub working_dir: Option<String>,
    pub config_files: Option<String>,
}

/// Live resource usage stats (refreshed on tick).
#[derive(Clone, Debug, Default)]
pub struct ContainerStats {
    pub cpu_percent: Option<f64>,
    /// Cumulative container CPU usage (nanoseconds) for cross-tick delta.
    pub cpu_total: Option<u64>,
    /// Cumulative system CPU usage (nanoseconds) for cross-tick delta.
    pub system_total: Option<u64>,
    /// Number of online CPUs.
    pub num_cpus: Option<u64>,
    /// Per-CPU cumulative usage (nanoseconds).
    pub percpu_total: Option<Vec<u64>>,
    /// Computed per-core CPU percentages.
    pub percpu_percent: Option<Vec<f64>>,
    pub mem_used: Option<u64>,
    pub mem_limit: Option<u64>,
    pub mem_usage: Option<String>,
    pub mem_percent: Option<f64>,
    pub block_read: Option<u64>,
    pub block_write: Option<u64>,
    pub net_rx: Option<u64>,
    pub net_tx: Option<u64>,
    pub pids: Option<u64>,
}

/// History ring buffers for sparkline graphs.
#[derive(Clone, Debug)]
pub struct StatsHistory {
    pub cpu: VecDeque<u64>,
    pub mem: VecDeque<u64>,
    pub block_read: VecDeque<u64>,
    pub block_write: VecDeque<u64>,
    pub net_rx: VecDeque<u64>,
    pub net_tx: VecDeque<u64>,
    prev_cpu_total: Option<u64>,
    prev_system_total: Option<u64>,
    prev_timestamp: Option<std::time::Instant>,
    prev_percpu_total: Option<Vec<u64>>,
    prev_block_read: Option<u64>,
    prev_block_write: Option<u64>,
    prev_net_rx: Option<u64>,
    prev_net_tx: Option<u64>,
}

impl StatsHistory {
    pub fn new() -> Self {
        Self {
            cpu: VecDeque::from(vec![0; HISTORY_LEN]),
            mem: VecDeque::from(vec![0; HISTORY_LEN]),
            block_read: VecDeque::from(vec![0; HISTORY_LEN]),
            block_write: VecDeque::from(vec![0; HISTORY_LEN]),
            net_rx: VecDeque::from(vec![0; HISTORY_LEN]),
            net_tx: VecDeque::from(vec![0; HISTORY_LEN]),
            prev_cpu_total: None,
            prev_system_total: None,
            prev_timestamp: None,
            prev_percpu_total: None,
            prev_block_read: None,
            prev_block_write: None,
            prev_net_rx: None,
            prev_net_tx: None,
        }
    }

    /// Push a stats sample; returns (overall CPU %, per-core CPU %).
    pub fn push(&mut self, stats: &ContainerStats) -> (Option<f64>, Option<Vec<f64>>) {
        let now = std::time::Instant::now();
        let cpu_now = stats.cpu_total.unwrap_or(0);
        let num_cpus = stats.num_cpus.unwrap_or(1) as f64;

        // CPU: compute delta across ticks
        // Prefer system_cpu_usage delta; fall back to wall-clock time (some cgroup v2 setups
        // don't report system_cpu_usage).
        let cpu_pct = if let Some(prev_cpu) = self.prev_cpu_total {
            let cpu_delta = cpu_now.saturating_sub(prev_cpu) as f64;

            // Try system CPU delta first
            let sys_delta = match (stats.system_total, self.prev_system_total) {
                (Some(cur), Some(prev)) => {
                    let d = cur.saturating_sub(prev) as f64;
                    if d > 0.0 { Some(d) } else { None }
                }
                _ => None,
            };

            if let Some(sd) = sys_delta {
                (cpu_delta / sd) * num_cpus * 100.0
            } else if let Some(prev_ts) = self.prev_timestamp {
                // Fall back to wall-clock time
                let wall_ns = prev_ts.elapsed().as_nanos() as f64;
                if wall_ns > 0.0 {
                    (cpu_delta / wall_ns) * 100.0
                } else {
                    0.0
                }
            } else {
                0.0
            }
        } else {
            0.0
        };

        // Per-core CPU percentages
        let sys_delta_for_percpu = match (stats.system_total, self.prev_system_total) {
            (Some(cur), Some(prev)) => {
                let d = cur.saturating_sub(prev) as f64;
                if d > 0.0 { Some(d) } else { None }
            }
            _ => None,
        };
        let wall_delta_ns = self.prev_timestamp.map(|ts| ts.elapsed().as_nanos() as f64);

        let percpu_pcts = if let (Some(ref cur), Some(ref prev)) = (&stats.percpu_total, &self.prev_percpu_total) {
            if cur.len() == prev.len() {
                if let Some(sd) = sys_delta_for_percpu {
                    Some(cur.iter().zip(prev.iter()).map(|(&c, &p)| {
                        let delta = c.saturating_sub(p) as f64;
                        (delta / sd) * num_cpus * 100.0
                    }).collect::<Vec<f64>>())
                } else if let Some(wall_ns) = wall_delta_ns {
                    if wall_ns > 0.0 {
                        Some(cur.iter().zip(prev.iter()).map(|(&c, &p)| {
                            let delta = c.saturating_sub(p) as f64;
                            (delta / wall_ns) * 100.0
                        }).collect::<Vec<f64>>())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        self.prev_cpu_total = Some(cpu_now);
        self.prev_system_total = stats.system_total;
        self.prev_timestamp = Some(now);
        self.prev_percpu_total = stats.percpu_total.clone();
        let cpu_val = (cpu_pct * 100.0) as u64;
        Self::push_ring(&mut self.cpu, cpu_val);

        // Memory: store in KiB
        let mem_val = stats.mem_used.map(|m| m / 1024).unwrap_or(0);
        Self::push_ring(&mut self.mem, mem_val);

        // Block I/O: store delta (bytes/tick)
        let br = stats.block_read.unwrap_or(0);
        let bw = stats.block_write.unwrap_or(0);
        if let Some(prev) = self.prev_block_read {
            Self::push_ring(&mut self.block_read, br.saturating_sub(prev));
        } else {
            Self::push_ring(&mut self.block_read, 0);
        }
        if let Some(prev) = self.prev_block_write {
            Self::push_ring(&mut self.block_write, bw.saturating_sub(prev));
        } else {
            Self::push_ring(&mut self.block_write, 0);
        }
        self.prev_block_read = Some(br);
        self.prev_block_write = Some(bw);

        // Network I/O: store delta (bytes/tick)
        let rx = stats.net_rx.unwrap_or(0);
        let tx = stats.net_tx.unwrap_or(0);
        if let Some(prev) = self.prev_net_rx {
            Self::push_ring(&mut self.net_rx, rx.saturating_sub(prev));
        } else {
            Self::push_ring(&mut self.net_rx, 0);
        }
        if let Some(prev) = self.prev_net_tx {
            Self::push_ring(&mut self.net_tx, tx.saturating_sub(prev));
        } else {
            Self::push_ring(&mut self.net_tx, 0);
        }
        self.prev_net_rx = Some(rx);
        self.prev_net_tx = Some(tx);

        (Some(cpu_pct), percpu_pcts)
    }

    fn push_ring(ring: &mut VecDeque<u64>, val: u64) {
        if ring.len() >= HISTORY_LEN {
            ring.pop_front();
        }
        ring.push_back(val);
    }
}

/// Actions available in the container action menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ContainerAction {
    Start,
    Stop,
    Restart,
    Pause,
    Unpause,
    Kill,
    Details,
    Logs,
    Remove,
}

impl ContainerAction {
    /// Actions shown for a running container.
    pub fn for_running() -> Vec<ContainerAction> {
        vec![
            ContainerAction::Details,
            ContainerAction::Logs,
            ContainerAction::Stop,
            ContainerAction::Restart,
            ContainerAction::Pause,
            ContainerAction::Kill,
            ContainerAction::Remove,
        ]
    }

    /// Actions shown for a paused container.
    pub fn for_paused() -> Vec<ContainerAction> {
        vec![
            ContainerAction::Details,
            ContainerAction::Logs,
            ContainerAction::Unpause,
            ContainerAction::Stop,
            ContainerAction::Kill,
            ContainerAction::Remove,
        ]
    }

    /// Actions shown for a stopped container.
    pub fn for_stopped() -> Vec<ContainerAction> {
        vec![
            ContainerAction::Details,
            ContainerAction::Logs,
            ContainerAction::Start,
            ContainerAction::Remove,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            ContainerAction::Start => "Start",
            ContainerAction::Stop => "Stop",
            ContainerAction::Restart => "Restart",
            ContainerAction::Pause => "Pause",
            ContainerAction::Unpause => "Unpause",
            ContainerAction::Kill => "Kill",
            ContainerAction::Details => "Details",
            ContainerAction::Logs => "Logs",
            ContainerAction::Remove => "Remove",
        }
    }
}

/// State for the action menu popup.
pub struct ActionMenu {
    pub actions: Vec<ContainerAction>,
    pub selected: usize,
}

impl ActionMenu {
    pub fn new(actions: Vec<ContainerAction>) -> Self {
        Self {
            actions,
            selected: 0,
        }
    }

    pub fn select_next(&mut self) {
        if !self.actions.is_empty() {
            self.selected = (self.selected + 1).min(self.actions.len() - 1);
        }
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn selected_action(&self) -> Option<ContainerAction> {
        self.actions.get(self.selected).copied()
    }
}

/// Which page/tab is active.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    List,
    Detail,
    Resources,
    Logs,
    Settings,
}

/// Application state for the TUI.
pub struct App {
    pub containers: Vec<ContainerInfo>,
    pub running: bool,
    pub selected: usize,
    pub page: Page,
    pub logs: Vec<String>,
    pub log_scroll: u16,
    pub auto_scroll: bool,
    pub detail: Option<ContainerDetail>,
    pub stats: Option<ContainerStats>,
    pub history: StatsHistory,
    pub detail_scroll: u16,
    /// Container ID whose logs/detail are currently loaded.
    pub logs_container_id: Option<String>,
    /// Action menu popup (None = closed).
    pub action_menu: Option<ActionMenu>,
    /// Transient status message shown in footer (e.g. "Container stopped").
    pub status_message: Option<(String, std::time::Instant)>,
    /// User settings (persisted to disk).
    pub settings: Settings,
    /// Which setting is selected (flat index).
    pub settings_selection: usize,
    /// Whether a setting value is being edited (Enter to toggle).
    pub settings_editing: bool,
    /// Page to return to when leaving Settings.
    pub previous_page: Option<Page>,
    /// Per-container stats history (keyed by container ID), used when poll_all is on.
    pub all_history: HashMap<String, StatsHistory>,
    /// Latest stats snapshot per container, used for list-page activity indicators.
    pub all_stats: HashMap<String, ContainerStats>,
    /// Set to true when the page changes to force a full terminal redraw.
    pub needs_clear: bool,
    /// Informational popup message (dismissed with Esc/Enter).
    pub info_popup: Option<String>,
}

impl App {
    pub fn new(settings: Settings) -> Self {
        Self {
            containers: Vec::new(),
            running: true,
            selected: 0,
            page: Page::List,
            logs: Vec::new(),
            log_scroll: 0,
            auto_scroll: true,
            detail: None,
            stats: None,
            history: StatsHistory::new(),
            detail_scroll: 0,
            logs_container_id: None,
            action_menu: None,
            status_message: None,
            settings,
            settings_selection: 0,
            settings_editing: false,
            previous_page: None,
            all_history: HashMap::new(),
            all_stats: HashMap::new(),
            needs_clear: false,
            info_popup: None,
        }
    }

    pub fn aggregation_window_ticks(&self) -> usize {
        self.settings
            .aggregation_window
            .as_ticks(self.settings.refresh_rate.as_millis())
    }

    pub fn active_theme(&self) -> Theme {
        Theme::from_name(self.settings.theme)
    }

    /// Open the action menu for the currently selected container.
    pub fn open_action_menu(&mut self) {
        if let Some(c) = self.containers.get(self.selected) {
            let status = &c.status;
            let actions = if status.contains("Paused") {
                ContainerAction::for_paused()
            } else if status.contains("Up") {
                ContainerAction::for_running()
            } else {
                ContainerAction::for_stopped()
            };
            self.action_menu = Some(ActionMenu::new(actions));
        }
    }

    pub fn close_action_menu(&mut self) {
        self.action_menu = None;
    }

    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some((msg, std::time::Instant::now()));
    }

    /// Switch to a new page and flag the terminal for a full redraw.
    pub fn set_page(&mut self, page: Page) {
        if self.page != page {
            self.page = page;
            self.needs_clear = true;
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn select_next(&mut self) {
        if !self.containers.is_empty() {
            self.selected = (self.selected + 1).min(self.containers.len() - 1);
        }
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn selected_container_id(&self) -> Option<&str> {
        self.containers.get(self.selected).map(|c| c.id.as_str())
    }
}
