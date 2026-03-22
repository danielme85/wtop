use bollard::container::{
    KillContainerOptions, ListContainersOptions, LogOutput, LogsOptions, MemoryStatsStats,
    RemoveContainerOptions, StatsOptions,
};
use bollard::Docker;
use futures_util::StreamExt;

use crate::app::{ComposeInfo, ContainerDetail, ContainerInfo, ContainerStats};

/// Connect to the local Docker daemon via the default socket.
pub fn connect() -> Result<Docker, bollard::errors::Error> {
    Docker::connect_with_local_defaults()
}

/// List all containers and return simplified info.
pub async fn list_containers(docker: &Docker) -> Vec<ContainerInfo> {
    let options = ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };

    match docker.list_containers(Some(options)).await {
        Ok(containers) => containers
            .into_iter()
            .map(|c| ContainerInfo {
                id: c.id.unwrap_or_default().chars().take(12).collect(),
                name: c
                    .names
                    .and_then(|n| n.first().cloned())
                    .unwrap_or_default()
                    .trim_start_matches('/')
                    .to_string(),
                image: c.image.unwrap_or_default(),
                status: c.status.unwrap_or_default(),
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Fetch the last `tail` lines of logs for a container.
pub async fn fetch_logs(docker: &Docker, container_id: &str, tail: usize) -> Vec<String> {
    let options = LogsOptions::<String> {
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
            lines.push(line.to_string());
        }
    }

    lines
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
    result.cpu_total = Some(stats.cpu_stats.cpu_usage.total_usage);
    result.system_total = stats.cpu_stats.system_cpu_usage;
    let num_cpus = stats.cpu_stats.online_cpus.unwrap_or(1);
    result.num_cpus = Some(num_cpus);

    // Per-CPU usage (for per-core display)
    if let Some(ref percpu) = stats.cpu_stats.cpu_usage.percpu_usage {
        result.percpu_total = Some(percpu.clone());
    }

    // Memory
    if let Some(mem_usage) = stats.memory_stats.usage {
        let cache = stats
            .memory_stats
            .stats
            .map(|s| match s {
                MemoryStatsStats::V1(v1) => v1.cache,
                MemoryStatsStats::V2(v2) => v2.inactive_file,
            })
            .unwrap_or(0);
        let used = mem_usage.saturating_sub(cache);
        let limit = stats.memory_stats.limit.unwrap_or(0);
        result.mem_used = Some(used);
        result.mem_limit = Some(limit);
        result.mem_usage = Some(format!("{} / {}", format_bytes(used), format_bytes(limit)));
        if limit > 0 {
            result.mem_percent = Some(used as f64 / limit as f64 * 100.0);
        }
    }

    // Block I/O (cumulative totals)
    // Try cgroup v1 blkio stats first, fall back to storage_stats for cgroup v2
    let mut read_total: u64 = 0;
    let mut write_total: u64 = 0;
    let mut have_blkio = false;
    if let Some(ref io_stats) = stats.blkio_stats.io_service_bytes_recursive {
        if !io_stats.is_empty() {
            have_blkio = true;
            for entry in io_stats {
                match entry.op.as_str() {
                    "read" | "Read" => read_total += entry.value,
                    "write" | "Write" => write_total += entry.value,
                    _ => {}
                }
            }
        }
    }
    if !have_blkio {
        // cgroup v2: fall back to storage_stats
        if let Some(r) = stats.storage_stats.read_size_bytes {
            read_total = r;
            have_blkio = true;
        }
        if let Some(w) = stats.storage_stats.write_size_bytes {
            write_total = w;
            have_blkio = true;
        }
    }
    if have_blkio {
        result.block_read = Some(read_total);
        result.block_write = Some(write_total);
    }

    // Network I/O (cumulative across all interfaces)
    // Try `networks` (per-interface map) first, fall back to `network` (aggregate)
    if let Some(ref networks) = stats.networks {
        let mut rx_total: u64 = 0;
        let mut tx_total: u64 = 0;
        for net in networks.values() {
            rx_total += net.rx_bytes;
            tx_total += net.tx_bytes;
        }
        result.net_rx = Some(rx_total);
        result.net_tx = Some(tx_total);
    } else if let Some(ref network) = stats.network {
        result.net_rx = Some(network.rx_bytes);
        result.net_tx = Some(network.tx_bytes);
    }

    // PIDs
    result.pids = Some(stats.pids_stats.current.unwrap_or(0));

    Some(result)
}

/// Start a stopped container.
pub async fn start_container(docker: &Docker, id: &str) -> Result<(), String> {
    docker
        .start_container::<String>(id, None)
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
        .kill_container(id, None::<KillContainerOptions<String>>)
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
