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

/// Bar rendering style for mini inline bars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BarStyle {
    /// Modern Unicode block characters: ███░░░
    #[default]
    Block,
    /// Sub-character precision with fractional blocks: ███▌
    Smooth,
    /// Gradient fade at the fill boundary: ██▓▒░
    Gradient,
    /// Filled/empty circles: ●●●○○○
    Dot,
    /// Segmented pipe characters: |||||||.....
    Pipe,
    /// CLI-style arrow: [=========> ]
    Arrow,
    /// Dotted/colon style: ::::::::....
    Dashed,
    /// Classic ASCII: [###------]
    Classic,
}

impl BarStyle {
    const ALL: &[Self] = &[
        Self::Block,
        Self::Smooth,
        Self::Gradient,
        Self::Dot,
        Self::Pipe,
        Self::Arrow,
        Self::Dashed,
        Self::Classic,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BarStyle::Block => "Block",
            BarStyle::Smooth => "Smooth",
            BarStyle::Gradient => "Gradient",
            BarStyle::Dot => "Dot",
            BarStyle::Pipe => "Pipe",
            BarStyle::Arrow => "Arrow",
            BarStyle::Dashed => "Dashed",
            BarStyle::Classic => "Classic",
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&s| s == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|&s| s == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Sparkline graph rendering style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphStyle {
    /// Nine-level vertical bars: ▁▂▃▄▅▆▇█
    #[default]
    Smooth,
    /// Three-level vertical bars: ▄█ (retro/chunky)
    Chunky,
    /// Braille dot patterns building from bottom: ⣀⣤⣶⣿
    Braille,
    /// Density shading (heatmap): ░▒▓█
    Shade,
    /// Connected line graph (no fill)
    Line,
    /// Line graph with shaded area beneath
    Area,
}

impl GraphStyle {
    const ALL: &[Self] = &[
        Self::Smooth,
        Self::Chunky,
        Self::Braille,
        Self::Shade,
        Self::Line,
        Self::Area,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GraphStyle::Smooth => "Smooth",
            GraphStyle::Chunky => "Chunky",
            GraphStyle::Braille => "Braille",
            GraphStyle::Shade => "Shade",
            GraphStyle::Line => "Line",
            GraphStyle::Area => "Area",
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&s| s == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|&s| s == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Line spacing for text-heavy pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineSpacing {
    /// No extra spacing (current default).
    #[default]
    Compact,
    /// One blank line between rows for easier reading.
    Comfortable,
}

impl LineSpacing {
    pub fn label(self) -> &'static str {
        match self {
            LineSpacing::Compact => "Compact",
            LineSpacing::Comfortable => "Comfortable",
        }
    }

    pub fn next(self) -> Self {
        match self {
            LineSpacing::Compact => LineSpacing::Comfortable,
            LineSpacing::Comfortable => LineSpacing::Compact,
        }
    }

    pub fn prev(self) -> Self {
        self.next() // only two options, toggle
    }

    /// Row height for table rows (1 = compact, 2 = comfortable).
    pub fn row_height(self) -> u16 {
        match self {
            LineSpacing::Compact => 1,
            LineSpacing::Comfortable => 2,
        }
    }
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
    #[serde(default)]
    pub bar_style: BarStyle,
    #[serde(default)]
    pub graph_style: GraphStyle,
    #[serde(default = "default_true")]
    pub log_color: bool,
    #[serde(default)]
    pub sort_by: SortBy,
    #[serde(default)]
    pub line_spacing: LineSpacing,
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
            bar_style: BarStyle::default(),
            graph_style: GraphStyle::default(),
            log_color: true,
            sort_by: SortBy::default(),
            line_spacing: LineSpacing::default(),
        }
    }
}

impl Settings {
    fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("wtop").join("settings.toml"))
    }

    /// Load settings, returning a warning if the file was corrupted.
    pub fn load_with_warning() -> (Self, Option<String>) {
        let path = match Self::config_path() {
            Some(p) => p,
            None => return (Self::default(), None),
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return (Self::default(), None),
        };
        match toml::from_str(&content) {
            Ok(s) => (s, None),
            Err(e) => {
                let msg = format!(
                    "Settings file corrupted, using defaults.\n{}\nFix or delete: {}",
                    e,
                    path.display()
                );
                (Self::default(), Some(msg))
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- Enum cycling ---

    #[test]
    fn aggregation_mode_cycles_forward() {
        assert_eq!(AggregationMode::Average.next(), AggregationMode::Max);
        assert_eq!(AggregationMode::Max.next(), AggregationMode::Last);
        assert_eq!(AggregationMode::Last.next(), AggregationMode::Average);
    }

    #[test]
    fn aggregation_mode_cycles_backward() {
        assert_eq!(AggregationMode::Average.prev(), AggregationMode::Last);
        assert_eq!(AggregationMode::Last.prev(), AggregationMode::Max);
        assert_eq!(AggregationMode::Max.prev(), AggregationMode::Average);
    }

    #[test]
    fn theme_cycles_both_directions() {
        // forward wraps: Norse → Light → Dark → Mono → Norse
        let themes = [ThemeName::Norse, ThemeName::Light, ThemeName::Dark, ThemeName::Mono];
        for i in 0..themes.len() {
            assert_eq!(themes[i].next(), themes[(i + 1) % themes.len()]);
            assert_eq!(themes[(i + 1) % themes.len()].prev(), themes[i]);
        }
    }

    #[test]
    fn refresh_rate_cycles_both_directions() {
        let rates = [RefreshRate::Ms250, RefreshRate::Ms500, RefreshRate::S1, RefreshRate::S2];
        for i in 0..rates.len() {
            assert_eq!(rates[i].next(), rates[(i + 1) % rates.len()]);
            assert_eq!(rates[(i + 1) % rates.len()].prev(), rates[i]);
        }
    }

    #[test]
    fn log_buffer_size_cycles_both_directions() {
        let sizes = [
            LogBufferSize::Lines100,
            LogBufferSize::Lines200,
            LogBufferSize::Lines500,
            LogBufferSize::Lines1000,
        ];
        for i in 0..sizes.len() {
            assert_eq!(sizes[i].next(), sizes[(i + 1) % sizes.len()]);
            assert_eq!(sizes[(i + 1) % sizes.len()].prev(), sizes[i]);
        }
    }

    #[test]
    fn sort_by_cycles_both_directions() {
        let variants = [
            SortBy::Name, SortBy::Status, SortBy::Cpu,
            SortBy::Memory, SortBy::Disk, SortBy::Network, SortBy::ComposeProject,
        ];
        for i in 0..variants.len() {
            assert_eq!(variants[i].next(), variants[(i + 1) % variants.len()]);
            assert_eq!(variants[(i + 1) % variants.len()].prev(), variants[i]);
        }
    }

    #[test]
    fn bar_style_cycles_both_directions() {
        // spot-check wrap-around
        assert_eq!(BarStyle::Classic.next(), BarStyle::Block);
        assert_eq!(BarStyle::Block.prev(), BarStyle::Classic);
    }

    #[test]
    fn graph_style_cycles_both_directions() {
        assert_eq!(GraphStyle::Area.next(), GraphStyle::Smooth);
        assert_eq!(GraphStyle::Smooth.prev(), GraphStyle::Area);
    }

    // --- AggregationWindow ---

    #[test]
    fn aggregation_window_clamps_at_max() {
        let mut w = AggregationWindow(20);
        w.increment();
        assert_eq!(w.as_secs_f64(), 5.0);
    }

    #[test]
    fn aggregation_window_clamps_at_min() {
        let mut w = AggregationWindow(1);
        w.decrement();
        assert_eq!(w.as_secs_f64(), 0.25);
    }

    #[test]
    fn aggregation_window_increments_correctly() {
        let mut w = AggregationWindow(4); // 1.0s
        w.increment();
        assert_eq!(w.as_secs_f64(), 1.25);
    }

    #[test]
    fn aggregation_window_as_ticks() {
        // 1.0s window (4 quarter-seconds), 250ms tick → 4 ticks
        let w = AggregationWindow(4);
        assert_eq!(w.as_ticks(250), 4);
        // 500ms tick → 2 ticks
        assert_eq!(w.as_ticks(500), 2);
        // tick larger than window → clamps to 1
        assert_eq!(w.as_ticks(2000), 1);
    }

    // --- ColumnVisibility ---

    #[test]
    fn column_toggle_works() {
        let mut cols = ColumnVisibility::default();
        assert!(cols.id); // on by default
        cols.toggle(0);   // turn off ID
        assert!(!cols.id);
        cols.toggle(0);   // turn back on
        assert!(cols.id);
    }

    #[test]
    fn column_toggle_refuses_last_visible() {
        let mut cols = ColumnVisibility {
            id: true,
            name: false,
            image: false,
            status: false,
            cpu: false,
            mem: false,
            disk: false,
            network: false,
        };
        assert_eq!(cols.visible_count(), 1);
        cols.toggle(0); // should be refused
        assert!(cols.id, "last visible column must not be toggled off");
        assert_eq!(cols.visible_count(), 1);
    }

    #[test]
    fn column_visible_count_is_accurate() {
        let mut cols = ColumnVisibility::default();
        let initial = cols.visible_count();
        cols.toggle(4); // enable cpu (was off)
        assert_eq!(cols.visible_count(), initial + 1);
        cols.toggle(4); // disable again
        assert_eq!(cols.visible_count(), initial);
    }

    // --- Settings round-trip ---

    #[test]
    fn settings_toml_round_trip() {
        let original = Settings::default();
        let serialized = toml::to_string_pretty(&original).expect("serialize");
        let deserialized: Settings = toml::from_str(&serialized).expect("deserialize");

        assert_eq!(deserialized.aggregation_mode, original.aggregation_mode);
        assert_eq!(deserialized.theme, original.theme);
        assert_eq!(deserialized.refresh_rate, original.refresh_rate);
        assert_eq!(deserialized.log_buffer_size, original.log_buffer_size);
        assert_eq!(deserialized.poll_all_containers, original.poll_all_containers);
        assert_eq!(deserialized.bar_style, original.bar_style);
        assert_eq!(deserialized.graph_style, original.graph_style);
        assert_eq!(deserialized.log_color, original.log_color);
        assert_eq!(deserialized.sort_by, original.sort_by);
    }

    #[test]
    fn settings_deserialize_missing_fields_uses_defaults() {
        // A minimal TOML with only the required fields — serde defaults fill the rest
        let toml = r#"
            aggregation_mode = "Average"
            aggregation_window = 4
            theme = "Norse"
            refresh_rate = "Ms250"
            log_buffer_size = "Lines200"
        "#;
        let s: Settings = toml::from_str(toml).expect("deserialize partial settings");
        assert!(!s.poll_all_containers);
        assert!(!s.show_cpu_bar);
        assert!(s.log_color); // default_true
        assert_eq!(s.sort_by, SortBy::default());
    }
}
