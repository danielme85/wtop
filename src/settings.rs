use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregationMode {
    Average,
    Max,
    Last,
}

impl AggregationMode {
    pub fn label(self) -> &'static str {
        match self {
            AggregationMode::Average => "Average",
            AggregationMode::Max => "Max",
            AggregationMode::Last => "Last",
        }
    }

    pub fn next(self) -> Self {
        match self {
            AggregationMode::Average => AggregationMode::Max,
            AggregationMode::Max => AggregationMode::Last,
            AggregationMode::Last => AggregationMode::Average,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            AggregationMode::Average => AggregationMode::Last,
            AggregationMode::Max => AggregationMode::Average,
            AggregationMode::Last => AggregationMode::Max,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeName {
    Norse,
    Light,
    Dark,
    Mono,
}

impl ThemeName {
    pub fn label(self) -> &'static str {
        match self {
            ThemeName::Norse => "Norse",
            ThemeName::Light => "Light",
            ThemeName::Dark => "Dark",
            ThemeName::Mono => "Monochrome",
        }
    }

    pub fn next(self) -> Self {
        match self {
            ThemeName::Norse => ThemeName::Light,
            ThemeName::Light => ThemeName::Dark,
            ThemeName::Dark => ThemeName::Mono,
            ThemeName::Mono => ThemeName::Norse,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            ThemeName::Norse => ThemeName::Mono,
            ThemeName::Light => ThemeName::Norse,
            ThemeName::Dark => ThemeName::Light,
            ThemeName::Mono => ThemeName::Dark,
        }
    }
}

/// Refresh rate options in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefreshRate {
    Ms250,
    Ms500,
    S1,
    S2,
}

impl RefreshRate {
    pub fn as_millis(self) -> u64 {
        match self {
            RefreshRate::Ms250 => 250,
            RefreshRate::Ms500 => 500,
            RefreshRate::S1 => 1000,
            RefreshRate::S2 => 2000,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RefreshRate::Ms250 => "250ms",
            RefreshRate::Ms500 => "500ms",
            RefreshRate::S1 => "1s",
            RefreshRate::S2 => "2s",
        }
    }

    pub fn next(self) -> Self {
        match self {
            RefreshRate::Ms250 => RefreshRate::Ms500,
            RefreshRate::Ms500 => RefreshRate::S1,
            RefreshRate::S1 => RefreshRate::S2,
            RefreshRate::S2 => RefreshRate::Ms250,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            RefreshRate::Ms250 => RefreshRate::S2,
            RefreshRate::Ms500 => RefreshRate::Ms250,
            RefreshRate::S1 => RefreshRate::Ms500,
            RefreshRate::S2 => RefreshRate::S1,
        }
    }
}

/// Log buffer size options.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogBufferSize {
    Lines100,
    Lines200,
    Lines500,
    Lines1000,
}

impl LogBufferSize {
    pub fn as_usize(self) -> usize {
        match self {
            LogBufferSize::Lines100 => 100,
            LogBufferSize::Lines200 => 200,
            LogBufferSize::Lines500 => 500,
            LogBufferSize::Lines1000 => 1000,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LogBufferSize::Lines100 => "100",
            LogBufferSize::Lines200 => "200",
            LogBufferSize::Lines500 => "500",
            LogBufferSize::Lines1000 => "1000",
        }
    }

    pub fn next(self) -> Self {
        match self {
            LogBufferSize::Lines100 => LogBufferSize::Lines200,
            LogBufferSize::Lines200 => LogBufferSize::Lines500,
            LogBufferSize::Lines500 => LogBufferSize::Lines1000,
            LogBufferSize::Lines1000 => LogBufferSize::Lines100,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            LogBufferSize::Lines100 => LogBufferSize::Lines1000,
            LogBufferSize::Lines200 => LogBufferSize::Lines100,
            LogBufferSize::Lines500 => LogBufferSize::Lines200,
            LogBufferSize::Lines1000 => LogBufferSize::Lines500,
        }
    }
}

/// Aggregation window in steps of 0.25s.
/// Stored as integer quarter-seconds (1..=20 → 0.25s..5.0s).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregationWindow(u8);

impl AggregationWindow {
    const MIN: u8 = 1;  // 0.25s
    const MAX: u8 = 20; // 5.0s

    pub fn default_value() -> Self {
        Self(4) // 1.0s
    }

    pub fn as_secs_f64(self) -> f64 {
        self.0 as f64 * 0.25
    }

    pub fn label(self) -> String {
        let secs = self.as_secs_f64();
        if secs == secs.floor() {
            format!("{}s", secs as u32)
        } else {
            format!("{:.2}s", secs)
        }
    }

    /// Convert to number of ticks given a tick rate in milliseconds.
    pub fn as_ticks(self, tick_ms: u64) -> usize {
        let window_ms = (self.0 as u64) * 250;
        (window_ms / tick_ms).max(1) as usize
    }

    pub fn increment(&mut self) {
        if self.0 < Self::MAX {
            self.0 += 1;
        }
    }

    pub fn decrement(&mut self) {
        if self.0 > Self::MIN {
            self.0 -= 1;
        }
    }
}

impl Default for AggregationWindow {
    fn default() -> Self {
        Self::default_value()
    }
}

/// Which columns are visible on the container list page.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ColumnVisibility {
    pub id: bool,
    pub name: bool,
    pub image: bool,
    pub status: bool,
    pub cpu: bool,
    pub mem: bool,
    pub disk: bool,
    pub network: bool,
}

impl Default for ColumnVisibility {
    fn default() -> Self {
        Self {
            id: true,
            name: true,
            image: true,
            status: true,
            cpu: false,
            mem: false,
            disk: false,
            network: false,
        }
    }
}

impl ColumnVisibility {
    /// Number of currently visible columns.
    pub fn visible_count(&self) -> usize {
        [
            self.id, self.name, self.image, self.status,
            self.cpu, self.mem, self.disk, self.network,
        ]
        .iter()
        .filter(|&&v| v)
        .count()
    }

    /// Toggle column at index (0-7). Refuses to toggle off the last visible column.
    pub fn toggle(&mut self, index: usize) {
        let current = self.is_visible(index);
        if current && self.visible_count() <= 1 {
            return; // don't toggle off the last visible column
        }
        match index {
            0 => self.id = !self.id,
            1 => self.name = !self.name,
            2 => self.image = !self.image,
            3 => self.status = !self.status,
            4 => self.cpu = !self.cpu,
            5 => self.mem = !self.mem,
            6 => self.disk = !self.disk,
            7 => self.network = !self.network,
            _ => {}
        }
    }

    /// Column label for settings display.
    pub fn column_label(index: usize) -> &'static str {
        match index {
            0 => "Column: ID",
            1 => "Column: Name",
            2 => "Column: Image",
            3 => "Column: Status",
            4 => "Column: CPU",
            5 => "Column: MEM",
            6 => "Column: Disk",
            7 => "Column: Network",
            _ => "",
        }
    }

    /// Get visibility for column at index.
    pub fn is_visible(&self, index: usize) -> bool {
        match index {
            0 => self.id,
            1 => self.name,
            2 => self.image,
            3 => self.status,
            4 => self.cpu,
            5 => self.mem,
            6 => self.disk,
            7 => self.network,
            _ => false,
        }
    }

    pub const COUNT: usize = 8;
}

/// Sort order for the container list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortBy {
    #[default]
    Name,
    Status,
    Cpu,
    Memory,
    Disk,
    Network,
    ComposeProject,
}

impl SortBy {
    pub fn label(self) -> &'static str {
        match self {
            SortBy::Name => "Name",
            SortBy::Status => "Status",
            SortBy::Cpu => "CPU",
            SortBy::Memory => "Memory",
            SortBy::Disk => "Disk",
            SortBy::Network => "Network",
            SortBy::ComposeProject => "Compose Project",
        }
    }

    pub fn next(self) -> Self {
        match self {
            SortBy::Name => SortBy::Status,
            SortBy::Status => SortBy::Cpu,
            SortBy::Cpu => SortBy::Memory,
            SortBy::Memory => SortBy::Disk,
            SortBy::Disk => SortBy::Network,
            SortBy::Network => SortBy::ComposeProject,
            SortBy::ComposeProject => SortBy::Name,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            SortBy::Name => SortBy::ComposeProject,
            SortBy::Status => SortBy::Name,
            SortBy::Cpu => SortBy::Status,
            SortBy::Memory => SortBy::Cpu,
            SortBy::Disk => SortBy::Memory,
            SortBy::Network => SortBy::Disk,
            SortBy::ComposeProject => SortBy::Network,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub aggregation_mode: AggregationMode,
    pub aggregation_window: AggregationWindow,
    pub theme: ThemeName,
    pub refresh_rate: RefreshRate,
    pub log_buffer_size: LogBufferSize,
    #[serde(default)]
    pub poll_all_containers: bool,
    #[serde(default)]
    pub columns: ColumnVisibility,
    #[serde(default)]
    pub show_cpu_bar: bool,
    #[serde(default)]
    pub show_mem_bar: bool,
    #[serde(default)]
    pub show_disk_bar: bool,
    #[serde(default)]
    pub show_network_bar: bool,
    #[serde(default = "default_true")]
    pub log_color: bool,
    #[serde(default)]
    pub sort_by: SortBy,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            aggregation_mode: AggregationMode::Average,
            aggregation_window: AggregationWindow::default(),
            theme: ThemeName::Norse,
            refresh_rate: RefreshRate::Ms250,
            log_buffer_size: LogBufferSize::Lines200,
            poll_all_containers: false,
            columns: ColumnVisibility::default(),
            show_cpu_bar: false,
            show_mem_bar: false,
            show_disk_bar: false,
            show_network_bar: false,
            log_color: true,
            sort_by: SortBy::default(),
        }
    }
}

impl Settings {
    fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("wtop").join("settings.toml"))
    }

    pub fn load() -> Self {
        let path = match Self::config_path() {
            Some(p) => p,
            None => return Self::default(),
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };
        toml::from_str(&content).unwrap_or_default()
    }

    pub fn save(&self) {
        let path = match Self::config_path() {
            Some(p) => p,
            None => return,
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = toml::to_string_pretty(self) {
            let _ = std::fs::write(&path, content);
        }
    }

}
