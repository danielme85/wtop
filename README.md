<p align="center">
  <h1 align="center">WhaleTop</h1>
  <p align="center">
    A fast, async Docker container monitor for your terminal.
    <br />
    Built with Rust. Powered by the Docker socket. No overhead.
  </p>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-2021-orange?style=flat-square" />
  <img src="https://img.shields.io/badge/Docker-Socket_API-2496ED?style=flat-square&logo=docker&logoColor=white" />
  <img src="https://img.shields.io/badge/TUI-Ratatui-blue?style=flat-square" />
  <img src="https://img.shields.io/badge/Built_with-Claude_Code-blueviolet?style=flat-square" />
</p>

---

## Features

- **Live container monitoring** — configurable refresh rate (250ms to 2s) with minimal resource usage
- **Multi-page views** — List, Detail, Resources (CPU/MEM/Disk/Net sparklines), and Logs
- **Background stats polling** — optional per-container stats collection preserves graph history across container switches
- **Customizable columns** — toggle ID, Name, Image, Status, CPU, MEM, Disk, Network with optional inline mini bar graphs
- **Container actions** — start, stop, restart, pause, unpause, kill, remove directly from the TUI
- **Log viewer** — auto-scrolling with configurable buffer size (100–1000 lines)
- **Direct socket access** — connects to `/var/run/docker.sock` via [Bollard](https://github.com/fussybeaver/bollard) (no shelling out)
- **Fully async** — [Tokio](https://tokio.rs) runtime drives Docker API calls and input handling concurrently
- **4 color themes** — Norse (default), Light, Dark, Monochrome
- **Persistent settings** — all preferences saved to `~/.config/wtop/settings.toml`

---

## Install

### From source

```bash
git clone https://github.com/danielme85/wtop.git
cd wtop
cargo build --release

# Or use the install script (builds + copies to ~/.local/bin):
./local-build-and-install.sh
```

### Pre-built binary (Linux amd64 / arm64)

Downloads the latest release — no authentication required:

```bash
curl -fsSL https://raw.githubusercontent.com/danielme85/wtop/main/remote-install.sh | sh
```

### Docker

```bash
docker buildx build --platform linux/amd64,linux/arm64 -t wtop .
docker run -it -v /var/run/docker.sock:/var/run/docker.sock wtop
```

### Cross-compilation

```bash
cross build --target aarch64-unknown-linux-gnu
```

---

## Quick Start

```bash
wtop          # launch the TUI
              # press 'q' to quit
```

---

## Navigation

| Key | Action |
|-----|--------|
| `Up` / `Down` | Navigate containers or scroll content |
| `Right` / `Left` | Switch pages: List &rarr; Detail &rarr; Resources &rarr; Logs |
| `Enter` | Open action menu for selected container |
| `PgUp` / `PgDn` | Switch container on detail/resources/logs pages |
| `s` | Open settings (from any page) |
| `a` | Toggle auto-scroll (logs page) |
| `q` | Quit |

---

## Settings

All settings are editable in-app via the Settings page (`s` key) and persisted to:

```
~/.config/wtop/settings.toml
```

| Setting | Options | Default |
|---------|---------|---------|
| **Aggregation Mode** | Average, Max, Last | Average |
| **Aggregation Window** | 0.25s to 5.0s (in 0.25s steps) | 1.0s |
| **Color Theme** | Norse, Light, Dark, Monochrome | Norse |
| **Refresh Rate** | 250ms, 500ms, 1s, 2s | 250ms |
| **Log Buffer Size** | 100, 200, 500, 1000 lines | 200 |
| **Poll All Containers** | Off, On | Off |
| **Column Visibility** | ID, Name, Image, Status, CPU, MEM, Disk, Network (toggle each) | ID, Name, Image, Status |
| **Mini Bars** | CPU, MEM, Disk, Network (toggle each) | All off |

> **Tip:** Enable *Poll All Containers* to collect background stats for every running container. This preserves sparkline graph history when switching between containers, and enables live activity indicators on the list page.

---

## Tech Stack

| Crate | Purpose |
|-------|---------|
| [**Ratatui**](https://ratatui.rs) + **Crossterm** | Terminal UI rendering (immediate mode) |
| [**Bollard**](https://github.com/fussybeaver/bollard) | Async Docker Engine API client |
| [**Tokio**](https://tokio.rs) | Async runtime |
| [**Serde**](https://serde.rs) + **TOML** | Settings serialization |

---

## Built with Claude Code

This project was built with [Claude Code](https://docs.anthropic.com/en/docs/claude-code) by Anthropic — an agentic coding tool that lives in the terminal. Claude Code assisted with architecture decisions, implementation, refactoring, and documentation throughout development.

---

## License

MIT
