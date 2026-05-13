use crate::settings::IconStyle;

pub struct IconSet {
    // Column headers
    pub col_cpu: &'static str,
    pub col_mem: &'static str,
    pub col_disk: &'static str,
    pub col_net: &'static str,
    // Header status counts
    pub status_running: &'static str,
    pub status_stopped: &'static str,
    // Settings section headers
    pub sec_general: &'static str,
    pub sec_sorting: &'static str,
    pub sec_logs: &'static str,
    pub sec_columns: &'static str,
    pub sec_minibars: &'static str,
    pub sec_about: &'static str,
    // Settings row labels
    pub row_aggregation_mode: &'static str,
    pub row_aggregation_window: &'static str,
    pub row_theme: &'static str,
    pub row_refresh: &'static str,
    pub row_log_buffer: &'static str,
    pub row_poll_all: &'static str,
    pub row_icons: &'static str,
    pub row_sort: &'static str,
    pub row_log_color: &'static str,
    pub row_confirm_quit: &'static str,
    pub row_bar_style: &'static str,
    pub row_graph_style: &'static str,
    pub row_cpu_bar: &'static str,
    pub row_mem_bar: &'static str,
    pub row_disk_bar: &'static str,
    pub row_network_bar: &'static str,
    // Detail page sections
    pub det_general: &'static str,
    pub det_compose: &'static str,
    pub det_network: &'static str,
    pub det_ports: &'static str,
    pub det_volumes: &'static str,
    pub det_environment: &'static str,
    // Resources page section titles
    pub res_cpu: &'static str,
    pub res_memory: &'static str,
    pub res_disk: &'static str,
    pub res_network: &'static str,
    pub res_pids: &'static str,
    // Action menu items
    pub action_details: &'static str,
    pub action_logs: &'static str,
    pub action_exec: &'static str,
    pub action_start: &'static str,
    pub action_stop: &'static str,
    pub action_restart: &'static str,
    pub action_pause: &'static str,
    pub action_unpause: &'static str,
    pub action_kill: &'static str,
    pub action_remove: &'static str,
}

pub static NO_ICONS: IconSet = IconSet {
    col_cpu: "", col_mem: "", col_disk: "", col_net: "",
    status_running: "▲ ", status_stopped: "▼ ",
    sec_general: "", sec_sorting: "", sec_logs: "",
    sec_columns: "", sec_minibars: "", sec_about: "",
    row_aggregation_mode: "", row_aggregation_window: "",
    row_theme: "", row_refresh: "", row_log_buffer: "",
    row_poll_all: "", row_icons: "", row_sort: "", row_log_color: "", row_confirm_quit: "",
    row_bar_style: "", row_graph_style: "",
    row_cpu_bar: "", row_mem_bar: "", row_disk_bar: "", row_network_bar: "",
    det_general: "", det_compose: "", det_network: "",
    det_ports: "", det_volumes: "", det_environment: "",
    res_cpu: "", res_memory: "", res_disk: "", res_network: "", res_pids: "",
    action_details: "", action_logs: "", action_exec: "",
    action_start: "", action_stop: "", action_restart: "",
    action_pause: "", action_unpause: "", action_kill: "", action_remove: "",
};

// All entries are exactly 3 terminal columns: 2-cell emoji + 1 space, or 1-cell symbol + 2 spaces.
pub static EMOJI_ICONS: IconSet = IconSet {
    col_cpu: "⚙  ", col_mem: "🧠 ", col_disk: "💽 ", col_net: "🌐 ",
    status_running: "🟢 ", status_stopped: "🔴 ",
    sec_general: "⚙  ", sec_sorting: "↕  ", sec_logs: "📋 ",
    sec_columns: "⊞  ", sec_minibars: "📈 ", sec_about: "ℹ  ",
    row_aggregation_mode: "📊 ", row_aggregation_window: "⏱  ",
    row_theme: "🎨 ", row_refresh: "🔄 ", row_log_buffer: "📝 ",
    row_poll_all: "🔍 ", row_icons: "✨ ", row_sort: "↕  ", row_log_color: "🌈 ", row_confirm_quit: "✅ ",
    row_bar_style: "▌  ", row_graph_style: "📉 ",
    row_cpu_bar: "⚙  ", row_mem_bar: "🧠 ", row_disk_bar: "💽 ", row_network_bar: "🌐 ",
    det_general: "ℹ  ", det_compose: "🐋 ", det_network: "🌐 ",
    det_ports: "🔌 ", det_volumes: "📁 ", det_environment: "🔧 ",
    res_cpu: "⚙  ", res_memory: "🧠 ", res_disk: "💽 ",
    res_network: "🌐 ", res_pids: "⚡ ",
    action_details: "📋 ", action_logs: "📄 ", action_exec: "💻 ",
    action_start: "▶  ", action_stop: "⏹  ", action_restart: "🔄 ",
    action_pause: "⏸  ", action_unpause: "▶  ", action_kill: "💀 ", action_remove: "🗑  ",
};

pub static NERD_ICONS: IconSet = IconSet {
    col_cpu:  "\u{f2db} ",   // nf-fa-microchip
    col_mem:  "\u{f538} ",   // nf-fa-memory
    col_disk: "\u{f0a0} ",   // nf-fa-hdd
    col_net:  "\u{f6ff} ",   // nf-fa-network-wired
    status_running: "\u{f04b} ",  // nf-fa-play
    status_stopped: "\u{f04d} ",  // nf-fa-stop
    sec_general:  "\u{f013} ",   // nf-fa-cog
    sec_sorting:  "\u{f0dc} ",   // nf-fa-sort
    sec_logs:     "\u{f15c} ",   // nf-fa-file-text
    sec_columns:  "\u{f0db} ",   // nf-fa-columns
    sec_minibars: "\u{f080} ",   // nf-fa-bar-chart
    sec_about:    "\u{f05a} ",   // nf-fa-info-circle
    row_aggregation_mode:   "\u{f1de} ",  // nf-fa-sliders
    row_aggregation_window: "\u{f017} ",  // nf-fa-clock-o
    row_theme:       "\u{f1fc} ",  // nf-fa-paint-brush
    row_refresh:     "\u{f021} ",  // nf-fa-refresh
    row_log_buffer:  "\u{f15c} ",  // nf-fa-file-text
    row_poll_all:    "\u{f002} ",  // nf-fa-search
    row_icons:       "\u{f11b} ",  // nf-fa-gamepad
    row_sort:        "\u{f0dc} ",  // nf-fa-sort
    row_log_color:   "\u{f53f} ",  // nf-fa-palette
    row_confirm_quit: "\u{f058} ",  // nf-fa-check-circle
    row_bar_style:   "\u{f080} ",  // nf-fa-bar-chart
    row_graph_style: "\u{f201} ",  // nf-fa-line-chart
    row_cpu_bar:     "\u{f2db} ",  // nf-fa-microchip
    row_mem_bar:     "\u{f538} ",  // nf-fa-memory
    row_disk_bar:    "\u{f0a0} ",  // nf-fa-hdd
    row_network_bar: "\u{f6ff} ",  // nf-fa-network-wired
    det_general:     "\u{f05a} ",  // nf-fa-info-circle
    det_compose:     "\u{e650} ",  // nf-dev-docker
    det_network:     "\u{f0ac} ",  // nf-fa-globe
    det_ports:       "\u{f1e6} ",  // nf-fa-plug
    det_volumes:     "\u{f07b} ",  // nf-fa-folder
    det_environment: "\u{f0ad} ",  // nf-fa-wrench
    res_cpu:     "\u{f2db} ",  // nf-fa-microchip
    res_memory:  "\u{f538} ",  // nf-fa-memory
    res_disk:    "\u{f0a0} ",  // nf-fa-hdd
    res_network: "\u{f0ac} ",  // nf-fa-globe
    res_pids:    "\u{f03a} ",  // nf-fa-list
    action_details: "\u{f05a} ",  // nf-fa-info-circle
    action_logs:    "\u{f15c} ",  // nf-fa-file-text
    action_exec:    "\u{f120} ",  // nf-fa-terminal
    action_start:   "\u{f04b} ",  // nf-fa-play
    action_stop:    "\u{f04d} ",  // nf-fa-stop
    action_restart: "\u{f01e} ",  // nf-fa-repeat
    action_pause:   "\u{f04c} ",  // nf-fa-pause
    action_unpause: "\u{f04b} ",  // nf-fa-play
    action_kill:    "\u{f1e2} ",  // nf-fa-bomb
    action_remove:  "\u{f1f8} ",  // nf-fa-trash
};

pub fn get_icons(style: &IconStyle) -> &'static IconSet {
    match style {
        IconStyle::None      => &NO_ICONS,
        IconStyle::Emoji     => &EMOJI_ICONS,
        IconStyle::NerdFonts => &NERD_ICONS,
    }
}
