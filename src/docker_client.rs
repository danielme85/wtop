use bollard::container::LogOutput;
use bollard::query_parameters::{
    KillContainerOptions, ListContainersOptions, LogsOptions, RemoveContainerOptions, StatsOptions,
};
use bollard::Docker;
use futures_util::StreamExt;

use crate::app::{ComposeInfo, ContainerDetail, ContainerInfo, ContainerStats};

/// Connect to the local Docker daemon via the default socket.
pub fn connect() -> Result<Docker, bollard::errors::Error> {
    Docker::connect_with_local_defaults()
}

/// Check if the Docker daemon is reachable.
pub async fn ping(docker: &Docker) -> bool {
    docker.ping().await.is_ok()
}

/// List all containers and return simplified info.
pub async fn list_containers(docker: &Docker) -> Vec<ContainerInfo> {
    let options = ListContainersOptions {
        all: true,
        ..Default::default()
    };

    match docker.list_containers(Some(options)).await {
        Ok(containers) => containers
            .into_iter()
            .map(|c| {
                let compose_project = c
                    .labels
                    .as_ref()
                    .and_then(|l| l.get("com.docker.compose.project"))
                    .cloned();
                let status = c.status.unwrap_or_default();
                // Parse health from status string (e.g. "Up 2 hours (healthy)")
                let health = if status.contains("(healthy)") {
                    Some("healthy".to_string())
                } else if status.contains("(unhealthy)") {
                    Some("unhealthy".to_string())
                } else if status.contains("(health: starting)") {
                    Some("starting".to_string())
                } else {
                    None
                };
                ContainerInfo {
                    id: c.id.unwrap_or_default().chars().take(12).collect(),
                    name: c
                        .names
                        .and_then(|n| n.first().cloned())
                        .unwrap_or_default()
                        .trim_start_matches('/')
                        .to_string(),
                    image: c.image.unwrap_or_default(),
                    status,
                    compose_project,
                    health,
                }
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Fetch the last `tail` lines of logs for a container.
pub async fn fetch_logs(docker: &Docker, container_id: &str, tail: usize) -> Vec<String> {
    let options = LogsOptions {
        stdout: true,
        stderr: true,
        tail: tail.to_string(),
        ..Default::default()
    };

    let mut stream = docker.logs(container_id, Some(options));
    let mut lines = Vec::new();

    while let Some(Ok(output)) = stream.next().await {
        let text = match output {
            LogOutput::StdOut { message } | LogOutput::StdErr { message } => {
                String::from_utf8_lossy(&message).to_string()
            }
            _ => continue,
        };
        for line in text.lines() {
            lines.push(strip_ansi_and_control(line));
        }
    }

    lines
}

/// Strip ANSI escape sequences and control characters from a log line.
fn strip_ansi_and_control(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip ESC sequences: ESC [ ... final_byte  or  ESC followed by one char
            if let Some(next) = chars.next() {
                if next == '[' {
                    // CSI sequence: consume until a letter or @ through ~
                    for sc in chars.by_ref() {
                        if sc.is_ascii_alphabetic() || ('@'..='~').contains(&sc) {
                            break;
                        }
                    }
                }
                // OSC, single-char escape, etc.: already consumed the next char
            }
        } else if c == '\r' {
            // Skip carriage returns
        } else if c.is_control() && c != '\t' {
            // Replace other control chars (except tab) with space
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// Inspect a container and return static detail info (ports, volumes, env, etc.).
/// Called once when entering the detail/logs page.
pub async fn inspect_container(docker: &Docker, container_id: &str) -> Option<ContainerDetail> {
    let inspect = docker.inspect_container(container_id, None).await.ok()?;

    let config = inspect.config.as_ref();
    let state = inspect.state.as_ref();
    let host_config = inspect.host_config.as_ref();
    let network_settings = inspect.network_settings.as_ref();

    let mut detail = ContainerDetail {
        full_id: inspect.id.unwrap_or_default(),
        image: config
            .and_then(|c| c.image.clone())
            .unwrap_or_default(),
        command: config
            .and_then(|c| c.cmd.as_ref())
            .map(|v| v.join(" "))
            .unwrap_or_default(),
        created: inspect.created.map(|t| t.to_string()).unwrap_or_default(),
        state: state
            .and_then(|s| s.status)
            .map(|s| format!("{:?}", s))
            .unwrap_or_default(),
        env: config
            .and_then(|c| c.env.clone())
            .unwrap_or_default(),
        host_network: host_config
            .and_then(|h| h.network_mode.as_deref())
            .map(|m| m == "host")
            .unwrap_or(false),
        restart_count: inspect.restart_count,
        started_at: state.and_then(|s| s.started_at.clone()),
        health: state
            .and_then(|s| s.health.as_ref())
            .and_then(|h| h.status)
            .map(|s| format!("{:?}", s)),
        ..Default::default()
    };

    // Compose metadata from labels
    if let Some(labels) = config.and_then(|c| c.labels.as_ref()) {
        if let (Some(project), Some(service)) = (
            labels.get("com.docker.compose.project"),
            labels.get("com.docker.compose.service"),
        ) {
            detail.compose = Some(ComposeInfo {
                project: project.clone(),
                service: service.clone(),
                working_dir: labels
                    .get("com.docker.compose.project.working_dir")
                    .cloned(),
                config_files: labels
                    .get("com.docker.compose.project.config_files")
                    .cloned(),
            });
        }
    }

    // Ports (sorted for stable display)
    if let Some(ports) = network_settings.and_then(|ns| ns.ports.as_ref()) {
        let mut port_list: Vec<String> = Vec::new();
        for (container_port, bindings) in ports {
            if let Some(bindings) = bindings {
                for b in bindings {
                    let host = b.host_ip.as_deref().unwrap_or("0.0.0.0");
                    let hp = b.host_port.as_deref().unwrap_or("?");
                    port_list.push(format!("{}:{} -> {}", host, hp, container_port));
                }
            } else {
                port_list.push(format!("{} (not bound)", container_port));
            }
        }
        port_list.sort();
        detail.ports = port_list;
    }

    // Volumes / Mounts
    if let Some(mounts) = inspect.mounts.as_ref() {
        for m in mounts {
            let src = m.source.as_deref().unwrap_or("?");
            let dst = m.destination.as_deref().unwrap_or("?");
            let mode = m.mode.as_deref().unwrap_or("rw");
            detail.volumes.push(format!("{} -> {} ({})", src, dst, mode));
        }
    }

    // Networks (sorted for stable display)
    if let Some(networks) = network_settings.and_then(|ns| ns.networks.as_ref()) {
        let mut net_list: Vec<String> = Vec::new();
        for (name, net) in networks {
            let ip = net.ip_address.as_deref().unwrap_or("n/a");
            let gw = net.gateway.as_deref().unwrap_or("n/a");
            net_list.push(format!("{}: ip={} gw={}", name, ip, gw));
        }
        net_list.sort();
        detail.networks = net_list;
    }

    Some(detail)
}

/// Fetch a one-shot CPU/memory/IO stats snapshot.
pub async fn fetch_stats(docker: &Docker, container_id: &str) -> Option<ContainerStats> {
    let stats_opts = StatsOptions {
        stream: false,
        one_shot: true,
    };
    let mut stream = docker.stats(container_id, Some(stats_opts));
    let stats = stream.next().await?.ok()?;

    let mut result = ContainerStats::default();

    // CPU: store cumulative counters; delta is computed across ticks in StatsHistory
    let cpu_stats = stats.cpu_stats?;
    let cpu_usage = cpu_stats.cpu_usage?;
    result.cpu_total = cpu_usage.total_usage;
    result.system_total = cpu_stats.system_cpu_usage;
    let num_cpus = cpu_stats.online_cpus.unwrap_or(1);
    result.num_cpus = Some(num_cpus as u64);

    // Per-CPU usage (for per-core display)
    if let Some(ref percpu) = cpu_usage.percpu_usage {
        result.percpu_total = Some(percpu.clone());
    }

    // Memory
    let mem_stats = stats.memory_stats?;
    if let Some(mem_usage) = mem_stats.usage {
        // In bollard 0.20, stats is a HashMap<String, u64> covering both cgroup v1 and v2
        let cache = mem_stats
            .stats
            .as_ref()
            .map(|s| {
                // cgroup v2 uses "inactive_file", cgroup v1 uses "cache"
                s.get("inactive_file")
                    .or_else(|| s.get("cache"))
                    .copied()
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        let used = mem_usage.saturating_sub(cache);
        let limit = mem_stats.limit.unwrap_or(0);
        result.mem_used = Some(used);
        result.mem_limit = Some(limit);
        result.mem_cache = Some(cache);
        result.mem_usage = Some(format!("{} / {}", format_bytes(used), format_bytes(limit)));
        if limit > 0 {
            result.mem_percent = Some(used as f64 / limit as f64 * 100.0);
        }

        // Swap usage (cgroup v2: "swap" in stats map; cgroup v1: swap - usage)
        if let Some(ref s) = mem_stats.stats {
            if let Some(&swap) = s.get("swap") {
                result.swap_used = Some(swap);
            }
        }
        result.swap_limit = mem_stats.max_usage; // cgroup v1 swap limit
    }

    // Block I/O (cumulative totals)
    // Try cgroup v1 blkio stats first, fall back to storage_stats for cgroup v2
    let mut read_total: u64 = 0;
    let mut write_total: u64 = 0;
    let mut have_blkio = false;
    if let Some(ref blkio) = stats.blkio_stats {
        if let Some(ref io_stats) = blkio.io_service_bytes_recursive {
            if !io_stats.is_empty() {
                have_blkio = true;
                for entry in io_stats {
                    match entry.op.as_deref().unwrap_or("") {
                        "read" | "Read" => read_total += entry.value.unwrap_or(0),
                        "write" | "Write" => write_total += entry.value.unwrap_or(0),
                        _ => {}
                    }
                }
            }
        }
    }
    if !have_blkio {
        // cgroup v2: fall back to storage_stats
        if let Some(ref storage) = stats.storage_stats {
            if let Some(r) = storage.read_size_bytes {
                read_total = r;
                have_blkio = true;
            }
            if let Some(w) = storage.write_size_bytes {
                write_total = w;
                have_blkio = true;
            }
        }
    }
    if have_blkio {
        result.block_read = Some(read_total);
        result.block_write = Some(write_total);
    }

    // Network I/O (cumulative across all interfaces)
    if let Some(ref networks) = stats.networks {
        let mut rx_total: u64 = 0;
        let mut tx_total: u64 = 0;
        for net in networks.values() {
            rx_total += net.rx_bytes.unwrap_or(0);
            tx_total += net.tx_bytes.unwrap_or(0);
        }
        result.net_rx = Some(rx_total);
        result.net_tx = Some(tx_total);
    }

    // PIDs
    result.pids = Some(stats.pids_stats.and_then(|p| p.current).unwrap_or(0));

    Some(result)
}

/// Start a stopped container.
pub async fn start_container(docker: &Docker, id: &str) -> Result<(), String> {
    docker
        .start_container(id, None)
        .await
        .map_err(|e| e.to_string())
}

/// Stop a running container.
pub async fn stop_container(docker: &Docker, id: &str) -> Result<(), String> {
    docker
        .stop_container(id, None)
        .await
        .map_err(|e| e.to_string())
}

/// Restart a container.
pub async fn restart_container(docker: &Docker, id: &str) -> Result<(), String> {
    docker
        .restart_container(id, None)
        .await
        .map_err(|e| e.to_string())
}

/// Pause a running container.
pub async fn pause_container(docker: &Docker, id: &str) -> Result<(), String> {
    docker
        .pause_container(id)
        .await
        .map_err(|e| e.to_string())
}

/// Unpause a paused container.
pub async fn unpause_container(docker: &Docker, id: &str) -> Result<(), String> {
    docker
        .unpause_container(id)
        .await
        .map_err(|e| e.to_string())
}

/// Kill a container (SIGKILL).
pub async fn kill_container(docker: &Docker, id: &str) -> Result<(), String> {
    docker
        .kill_container(id, None::<KillContainerOptions>)
        .await
        .map_err(|e| e.to_string())
}

/// Remove a container (force).
pub async fn remove_container(docker: &Docker, id: &str) -> Result<(), String> {
    let options = RemoveContainerOptions {
        force: true,
        ..Default::default()
    };
    docker
        .remove_container(id, Some(options))
        .await
        .map_err(|e| e.to_string())
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} B", bytes)
    }
}
