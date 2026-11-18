//! Resolve which Docker daemon to talk to, the same way the `docker` CLI does.
//!
//! wtop reads container data through bollard but runs `docker exec` through the CLI.
//! If the two pick different daemons (Colima, OrbStack, rootless Docker, Docker Desktop
//! contexts, a `tcp://` DOCKER_HOST, ...), the container list and Exec Shell disagree.
//! Resolving the endpoint once here and handing the same value to both keeps them in sync.
//!
//! Resolution order (mirrors the docker CLI):
//! 1. `DOCKER_HOST`
//! 2. the context named by `DOCKER_CONTEXT`, else `currentContext` in the CLI config
//!    (`$DOCKER_CONFIG/config.json`, default `~/.docker/config.json`)
//! 3. the platform default socket

use std::path::{Path, PathBuf};

/// The daemon address used when nothing else is configured (same default as bollard).
#[cfg(unix)]
pub const DEFAULT_HOST: &str = "unix:///var/run/docker.sock";
#[cfg(windows)]
pub const DEFAULT_HOST: &str = "npipe:////./pipe/docker_engine";

/// A resolved daemon endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockerHost {
    /// Endpoint URL, e.g. `unix:///var/run/docker.sock` or `tcp://10.0.0.5:2375`.
    pub host: String,
    /// Whether the endpoint requires TLS (not supported by wtop's bollard build).
    pub tls: bool,
}

/// Resolve the daemon endpoint from the real environment.
pub fn resolve() -> Result<DockerHost, String> {
    let config_dir = std::env::var_os("DOCKER_CONFIG")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".docker")));
    resolve_with(|k| std::env::var(k).ok(), config_dir.as_deref())
}

fn resolve_with(
    env: impl Fn(&str) -> Option<String>,
    config_dir: Option<&Path>,
) -> Result<DockerHost, String> {
    let env = |k: &str| env(k).filter(|v| !v.is_empty());

    if let Some(host) = env("DOCKER_HOST") {
        return Ok(DockerHost {
            host,
            tls: env("DOCKER_TLS_VERIFY").is_some(),
        });
    }

    let context = env("DOCKER_CONTEXT").or_else(|| config_dir.and_then(current_context));
    match context {
        Some(name) if name != "default" => {
            let dir = config_dir.ok_or_else(|| {
                format!("Docker context '{}' is set but no docker config dir was found", name)
            })?;
            context_host(dir, &name)
        }
        _ => Ok(DockerHost {
            host: DEFAULT_HOST.to_string(),
            tls: false,
        }),
    }
}

/// Read `currentContext` from the CLI's config.json.
fn current_context(config_dir: &Path) -> Option<String> {
    let content = std::fs::read_to_string(config_dir.join("config.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("currentContext")?.as_str().map(str::to_string)
}

/// Look up a named context's docker endpoint in `<config>/contexts/meta/*/meta.json`.
///
/// The CLI names each directory after the SHA-256 of the context name; scanning and
/// matching on `Name` avoids needing a hash implementation here.
fn context_host(config_dir: &Path, name: &str) -> Result<DockerHost, String> {
    let not_found = || format!("Docker context '{}' not found (see `docker context ls`)", name);
    let meta_root = config_dir.join("contexts").join("meta");
    let entries = std::fs::read_dir(&meta_root).map_err(|_| not_found())?;

    for entry in entries.flatten() {
        let Ok(content) = std::fs::read_to_string(entry.path().join("meta.json")) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };
        if json.get("Name").and_then(|n| n.as_str()) != Some(name) {
            continue;
        }
        let host = json
            .pointer("/Endpoints/docker/Host")
            .and_then(|h| h.as_str())
            .filter(|h| !h.is_empty())
            .ok_or_else(|| format!("Docker context '{}' has no docker endpoint", name))?;
        // TLS material for a context lives in contexts/tls/<same dir name>/docker/.
        let tls = config_dir
            .join("contexts")
            .join("tls")
            .join(entry.file_name())
            .join("docker")
            .is_dir();
        return Ok(DockerHost {
            host: host.to_string(),
            tls,
        });
    }
    Err(not_found())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    /// Build a fake docker config dir in a unique temp location.
    fn fake_config(test: &str, current: Option<&str>, contexts: &[(&str, &str, bool)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wtop-test-{}-{}", test, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(c) = current {
            std::fs::write(dir.join("config.json"), format!(r#"{{"currentContext":"{}"}}"#, c))
                .unwrap();
        }
        for (i, (name, host, tls)) in contexts.iter().enumerate() {
            let id = format!("hash{}", i);
            let meta = dir.join("contexts/meta").join(&id);
            std::fs::create_dir_all(&meta).unwrap();
            std::fs::write(
                meta.join("meta.json"),
                format!(
                    r#"{{"Name":"{}","Metadata":{{}},"Endpoints":{{"docker":{{"Host":"{}","SkipTLSVerify":false}}}}}}"#,
                    name, host
                ),
            )
            .unwrap();
            if *tls {
                std::fs::create_dir_all(dir.join("contexts/tls").join(&id).join("docker")).unwrap();
            }
        }
        dir
    }

    #[test]
    fn defaults_to_platform_socket() {
        let r = resolve_with(env_from(&[]), None).unwrap();
        assert_eq!(r.host, DEFAULT_HOST);
        assert!(!r.tls);
    }

    #[test]
    fn docker_host_env_wins_over_context() {
        let dir = fake_config("env-wins", Some("colima"), &[("colima", "unix:///c.sock", false)]);
        let env = env_from(&[("DOCKER_HOST", "tcp://10.0.0.5:2375"), ("DOCKER_CONTEXT", "colima")]);
        let r = resolve_with(env, Some(&dir)).unwrap();
        assert_eq!(r.host, "tcp://10.0.0.5:2375");
        assert!(!r.tls);
    }

    #[test]
    fn docker_tls_verify_marks_tls() {
        let env = env_from(&[("DOCKER_HOST", "tcp://h:2376"), ("DOCKER_TLS_VERIFY", "1")]);
        assert!(resolve_with(env, None).unwrap().tls);
    }

    #[test]
    fn current_context_from_config_json() {
        let dir = fake_config(
            "current",
            Some("colima"),
            &[("other", "unix:///o.sock", false), ("colima", "unix:///c.sock", false)],
        );
        let r = resolve_with(env_from(&[]), Some(&dir)).unwrap();
        assert_eq!(r.host, "unix:///c.sock");
    }

    #[test]
    fn docker_context_env_overrides_config() {
        let dir = fake_config(
            "ctx-env",
            Some("colima"),
            &[("colima", "unix:///c.sock", false), ("orb", "unix:///orb.sock", false)],
        );
        let r = resolve_with(env_from(&[("DOCKER_CONTEXT", "orb")]), Some(&dir)).unwrap();
        assert_eq!(r.host, "unix:///orb.sock");
    }

    #[test]
    fn default_context_uses_default_socket() {
        let dir = fake_config("default-ctx", Some("default"), &[]);
        assert_eq!(resolve_with(env_from(&[]), Some(&dir)).unwrap().host, DEFAULT_HOST);
    }

    #[test]
    fn missing_context_is_an_error() {
        let dir = fake_config("missing", Some("ghost"), &[]);
        assert!(resolve_with(env_from(&[]), Some(&dir)).unwrap_err().contains("ghost"));
    }

    #[test]
    fn context_with_tls_material_is_flagged() {
        let dir = fake_config("tls", Some("remote"), &[("remote", "tcp://r:2376", true)]);
        assert!(resolve_with(env_from(&[]), Some(&dir)).unwrap().tls);
    }
}
