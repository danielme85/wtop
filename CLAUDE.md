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
- **Direct socket access**: Bollard talks to the Docker API directly (no shell overhead). The endpoint is resolved like the `docker` CLI does (`DOCKER_HOST` → docker context → `/var/run/docker.sock`) in `docker_host.rs`, and Exec Shell passes the same endpoint to `docker exec`
- **Fully async**: Tokio runtime drives both Docker API calls and the input event loop
- **Configurable tick rate**: Default 250ms refresh; input polled with 50ms timeout
- **Per-container stats history**: Optional background polling stores stats for all running containers in a HashMap, preserving graph history across container switches

## Project Structure

```
src/
  main.rs              — tokio entrypoint, terminal setup/teardown, event loop
  app.rs               — App state (container list, stats history, per-container data)
  docker_client.rs     — Bollard client: connect, list, inspect, stats, logs, lifecycle
  docker_host.rs       — Resolve the daemon endpoint (DOCKER_HOST / docker context / default)
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

## Homebrew

Available via a custom tap. The release workflow automatically updates the tap formula when a new version tag is pushed.

```bash
brew install danielme85/tap/wtop
```

The formula and tap repo live at [danielme85/homebrew-tap](https://github.com/danielme85/homebrew-tap). The `update-homebrew` job in `.github/workflows/release.yml` handles automatic updates — it requires the `HOMEBREW_TAP_TOKEN` repository secret.

Every release must publish `SHA256SUMS` (the `release` job generates it): `wtop --update` and `remote-install.sh` refuse to install a binary they cannot verify against it.

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
