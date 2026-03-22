# WhaleTop

Asynchronous, low-resource Docker container monitor TUI built in Rust. Named after the Docker whale.

![Rust](https://img.shields.io/badge/Rust-2021-orange) ![Docker](https://img.shields.io/badge/Docker-Socket-blue)

## Features

- **Live container monitoring** — configurable refresh rate (250ms–2s) with minimal overhead
- **Multi-page views** — container list, detail, resources (CPU/MEM/Disk/Net sparklines), and logs
- **Background stats polling** — optional per-container stats collection preserves graph history across container switches
- **Customizable list columns** — toggle ID, Name, Image, Status, CPU, MEM, Disk, Network columns; live activity indicators when polling is enabled
- **Status summary** — header banner shows running, stopped, and total container counts at a glance
- **Container actions** — start, stop, restart, pause, unpause, kill, remove via action menu
- **Log viewer** — auto-scroll with bottom padding, configurable buffer size
- **Direct socket access** — connects to `/var/run/docker.sock` via Bollard (no shell overhead)
- **Fully async** — Tokio runtime drives both Docker API calls and the input event loop
- **Themes** — Norse (default), Light, Dark, Monochrome
- **Persistent settings** — saved to `~/.config/wtop/settings.toml`

## Quick Start

```bash
cargo build --release
cargo run
# Press 'q' to quit
```

### Docker

```bash
docker buildx build --platform linux/amd64,linux/arm64 -t wtop .
docker run -it -v /var/run/docker.sock:/var/run/docker.sock wtop
```

## Tech Stack

- **[Ratatui](https://ratatui.rs)** + **Crossterm** — terminal UI rendering
- **[Bollard](https://github.com/fussybeaver/bollard)** — async Docker API client
- **[Tokio](https://tokio.rs)** — async runtime

## Navigation

| Key | Action |
|-----|--------|
| `Up/Down` | Navigate containers / scroll |
| `Right/Left` | Switch pages (List → Detail → Resources → Logs) |
| `Enter` | Open action menu |
| `PgUp/PgDn` | Switch container (on detail/resources/logs pages) |
| `a` | Toggle auto-scroll (logs page) |
| `s` | Open settings |
| `q` | Quit |

## Cross-compilation

```bash
cross build --target aarch64-unknown-linux-gnu
```

## License

MIT
