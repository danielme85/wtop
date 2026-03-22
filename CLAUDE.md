# WhaleTop (wtop)

Asynchronous, low-resource Docker container monitor TUI. Named after the Docker whale logo.

## Build & Run

```bash
cargo build              # dev build
cargo build --release    # optimized build
cargo run                # launch TUI (press 'q' to quit)
cargo clippy             # lint
cargo test               # run tests
```

## Architecture

- **Immediate-mode TUI**: Ratatui renders from a snapshot of state each frame — no persistent widget tree
- **Direct socket access**: Bollard connects to `/var/run/docker.sock` directly (no shell overhead)
- **Fully async**: Tokio runtime drives both Docker API calls and the input event loop
- **Configurable tick rate**: Default 250ms refresh; input polled with 50ms timeout
- **Per-container stats history**: Optional background polling stores stats for all running containers in a HashMap, preserving graph history across container switches

## Project Structure

```
src/
  main.rs              — tokio entrypoint, terminal setup/teardown, event loop
  app.rs               — App state (container list, stats history, per-container data)
  docker_client.rs     — Bollard client: connect, list, inspect, stats, logs, lifecycle
  ui.rs                — Ratatui rendering: list, detail, resources, logs, settings
  settings.rs          — Persisted settings: aggregation, theme, refresh rate, column visibility
  theme.rs             — Color themes (Norse, Light, Dark, Mono)
  aggregation.rs       — Sliding-window aggregation (Average, Max, Last) for sparkline graphs
```

## Tech Stack

- **Rust** (edition 2021)
- **ratatui** + **crossterm** — TUI rendering
- **bollard** — async Docker API client
- **tokio** — async runtime

## Cross-compilation

```bash
cross build --target aarch64-unknown-linux-gnu    # uses Cross.toml config
```

## Docker Build

```bash
docker buildx build --platform linux/amd64,linux/arm64 -t wtop .
# Runtime requires socket mount:
docker run -it -v /var/run/docker.sock:/var/run/docker.sock wtop
```

## Settings

Settings are persisted to `~/.config/wtop/settings.toml` and editable in-app via the Settings page (`s` key):

- **Aggregation Mode** — Average / Max / Last (for sparkline smoothing)
- **Aggregation Window** — 0.25s to 5.0s
- **Color Theme** — Norse / Light / Dark / Monochrome
- **Refresh Rate** — 250ms / 500ms / 1s / 2s
- **Log Buffer Size** — 100 / 200 / 500 / 1000 lines
- **Poll All Containers** — Off / On (background stats polling for all running containers)
- **Column Visibility** — Toggle ID, Name, Image, Status, CPU, MEM, Disk, Network columns on the list page (minimum 1 required)
- **Mini Bars** — Toggle inline bar graphs for CPU, MEM, Disk, Network columns on the list page (4 separate settings)

## Style

- Norse palette (default): ocean blue borders, amber/gold titles, green for "Running" status, black background
- Rounded Unicode borders
- Whale emoji (🐋) in header
