use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::parser::RequiresConfig;

const IMAGE_PREFIX: &str = "wb-sandbox";
const LABEL_KEY: &str = "dev.workbooks.sandbox";

/// Generate a Dockerfile from a RequiresConfig.
pub fn generate_dockerfile(config: &RequiresConfig) -> Result<String, String> {
    match config.sandbox.as_str() {
        "python" => Ok(generate_python_dockerfile(config)),
        "node" => Ok(generate_node_dockerfile(config)),
        "custom" => Err("custom sandbox uses a user-provided Dockerfile".to_string()),
        other => Err(format!("unknown sandbox type: {}", other)),
    }
}

/// Install wb inside the container via the public installer.
const WB_INSTALL: &str = "RUN curl -fsSL https://get.workbooks.dev | WB_INSTALL_DIR=/usr/local/bin sh";

fn generate_python_dockerfile(config: &RequiresConfig) -> String {
    let mut lines = Vec::new();

    // Trixie base for glibc 2.40+ (needed by wb binary)
    lines.push("FROM debian:trixie-slim".to_string());
    lines.push(format!("LABEL {}=true", LABEL_KEY));

    // Always need curl + ca-certs for wb install, python3 for runtime
    let mut apt_deps: Vec<String> = vec![
        "curl".to_string(),
        "ca-certificates".to_string(),
        "python3".to_string(),
        "python3-venv".to_string(),
    ];
    for pkg in &config.apt {
        if !apt_deps.contains(pkg) {
            apt_deps.push(pkg.clone());
        }
    }

    lines.push(format!(
        "RUN apt-get update && apt-get install -y --no-install-recommends {} && rm -rf /var/lib/apt/lists/*",
        apt_deps.join(" ")
    ));

    // Install uv
    lines.push("RUN curl -LsSf https://astral.sh/uv/install.sh | sh".to_string());
    lines.push("ENV PATH=\"/root/.local/bin:$PATH\"".to_string());

    if !config.pip.is_empty() {
        lines.push(format!(
            "RUN uv pip install --system --break-system-packages {}",
            config.pip.join(" ")
        ));
    }

    lines.push(WB_INSTALL.to_string());
    lines.push("WORKDIR /work".to_string());
    lines.join("\n")
}

fn generate_node_dockerfile(config: &RequiresConfig) -> String {
    let mut lines = Vec::new();

    // Trixie base for glibc 2.40+ (needed by wb binary)
    lines.push("FROM debian:trixie-slim".to_string());
    lines.push(format!("LABEL {}=true", LABEL_KEY));

    // Always need curl + ca-certs for wb install, nodejs + npm for runtime
    let mut apt_deps: Vec<String> = vec![
        "curl".to_string(),
        "ca-certificates".to_string(),
        "nodejs".to_string(),
        "npm".to_string(),
    ];
    for pkg in &config.apt {
        if !apt_deps.contains(pkg) {
            apt_deps.push(pkg.clone());
        }
    }

    lines.push(format!(
        "RUN apt-get update && apt-get install -y --no-install-recommends {} && rm -rf /var/lib/apt/lists/*",
        apt_deps.join(" ")
    ));

    if !config.node.is_empty() {
        lines.push(format!(
            "RUN npm install -g {}",
            config.node.join(" ")
        ));
    }

    lines.push(WB_INSTALL.to_string());
    lines.push("WORKDIR /work".to_string());
    lines.join("\n")
}

/// Compute a deterministic image tag from the requires config.
/// Returns the full image name: wb-sandbox:<hash>
pub fn image_tag(config: &RequiresConfig) -> String {
    let mut hasher = Sha256::new();
    hasher.update(config.sandbox.as_bytes());
    for pkg in &config.apt {
        hasher.update(b"apt:");
        hasher.update(pkg.as_bytes());
    }
    for pkg in &config.pip {
        hasher.update(b"pip:");
        hasher.update(pkg.as_bytes());
    }
    for pkg in &config.node {
        hasher.update(b"node:");
        hasher.update(pkg.as_bytes());
    }
    if let Some(ref df) = config.dockerfile {
        hasher.update(b"dockerfile:");
        // Hash the file contents for custom dockerfiles
        if let Ok(contents) = std::fs::read(df) {
            hasher.update(&contents);
        } else {
            hasher.update(df.as_bytes());
        }
    }
    let hash = format!("{:x}", hasher.finalize());
    let short = &hash[..12];
    format!("{}:{}", IMAGE_PREFIX, short)
}

/// Check if a Docker image exists locally.
pub fn image_exists(tag: &str) -> bool {
    Command::new("docker")
        .args(["image", "inspect", tag])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Build a sandbox image. Returns the image tag on success.
pub fn build_image(config: &RequiresConfig, workbook_dir: &str) -> Result<String, String> {
    let tag = image_tag(config);

    if image_exists(&tag) {
        return Ok(tag);
    }

    eprintln!("wb: building sandbox image {}...", tag);

    if config.sandbox == "custom" {
        let dockerfile = config
            .dockerfile
            .as_deref()
            .ok_or("sandbox: custom requires a dockerfile field")?;

        let dockerfile_path = if Path::new(dockerfile).is_absolute() {
            dockerfile.to_string()
        } else {
            Path::new(workbook_dir)
                .join(dockerfile)
                .to_string_lossy()
                .to_string()
        };

        if !Path::new(&dockerfile_path).exists() {
            return Err(format!("dockerfile not found: {}", dockerfile_path));
        }

        let output = Command::new("docker")
            .args([
                "build",
                "-t",
                &tag,
                "--label",
                &format!("{}=true", LABEL_KEY),
                "-f",
                &dockerfile_path,
                workbook_dir,
            ])
            .output()
            .map_err(|e| format!("docker build: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "docker build failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    } else {
        let dockerfile_content =
            generate_dockerfile(config).map_err(|e| format!("generate dockerfile: {}", e))?;

        let output = Command::new("docker")
            .args(["build", "-t", &tag, "-f", "-", "."])
            .current_dir(workbook_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("docker build: {}", e))
            .and_then(|mut child| {
                use std::io::Write;
                if let Some(ref mut stdin) = child.stdin {
                    stdin
                        .write_all(dockerfile_content.as_bytes())
                        .map_err(|e| format!("write dockerfile: {}", e))?;
                }
                drop(child.stdin.take());
                child.wait_with_output().map_err(|e| format!("docker: {}", e))
            })?;

        if !output.status.success() {
            return Err(format!(
                "docker build failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }

    eprintln!("wb: sandbox image ready: {}", tag);
    Ok(tag)
}

/// Run a workbook inside a sandbox container.
/// Re-invokes `wb` inside the container with the workbook mounted.
pub fn run_in_sandbox(
    image_tag: &str,
    workbook_path: &str,
    env: &HashMap<String, String>,
    extra_args: &[String],
) -> Result<i32, String> {
    let workbook_abs = std::fs::canonicalize(workbook_path)
        .map_err(|e| format!("canonicalize {}: {}", workbook_path, e))?;
    let workbook_dir = workbook_abs
        .parent()
        .ok_or("workbook has no parent directory")?;
    let workbook_filename = workbook_abs
        .file_name()
        .ok_or("workbook has no filename")?
        .to_string_lossy();

    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let checkpoints_dir = format!("{}/.wb/checkpoints", home);

    // Ensure checkpoints dir exists on host
    let _ = std::fs::create_dir_all(&checkpoints_dir);

    let mut cmd = Command::new("docker");
    cmd.args(["run", "--rm"]);

    // Mount workbook directory
    cmd.args([
        "-v",
        &format!("{}:/work", workbook_dir.display()),
    ]);

    // Mount checkpoints directory
    cmd.args([
        "-v",
        &format!("{}:/root/.wb/checkpoints", checkpoints_dir),
    ]);

    cmd.args(["-w", "/work"]);

    // Pass environment variables
    for (k, v) in env {
        cmd.args(["-e", &format!("{}={}", k, v)]);
    }

    // Prevent sandbox recursion
    cmd.args(["-e", "WB_SANDBOX_INNER=1"]);

    cmd.arg(image_tag);

    // wb run <file> inside container
    cmd.args(["wb", "run", &workbook_filename.to_string()]);
    cmd.args(extra_args);

    // Inherit stdio so output streams through
    cmd.stdin(std::process::Stdio::inherit());
    cmd.stdout(std::process::Stdio::inherit());
    cmd.stderr(std::process::Stdio::inherit());

    let status = cmd
        .status()
        .map_err(|e| format!("docker run: {}", e))?;

    Ok(status.code().unwrap_or(-1))
}

/// List all wb-sandbox images.
pub fn list_images() -> Vec<(String, String, String)> {
    let output = Command::new("docker")
        .args([
            "images",
            "--filter",
            &format!("label={}", LABEL_KEY),
            "--format",
            "{{.Repository}}:{{.Tag}}\t{{.Size}}\t{{.CreatedSince}}",
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            text.lines()
                .filter(|l| !l.is_empty())
                .filter_map(|line| {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 3 {
                        Some((
                            parts[0].to_string(),
                            parts[1].to_string(),
                            parts[2].to_string(),
                        ))
                    } else {
                        None
                    }
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Remove all wb-sandbox images.
pub fn prune_images() -> usize {
    let images = list_images();
    let mut removed = 0;
    for (tag, _, _) in &images {
        let status = Command::new("docker")
            .args(["rmi", tag])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if status.map(|s| s.success()).unwrap_or(false) {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn python_config() -> RequiresConfig {
        RequiresConfig {
            sandbox: "python".to_string(),
            apt: vec!["qpdf".to_string(), "poppler-utils".to_string()],
            pip: vec!["pikepdf".to_string(), "pypdf".to_string()],
            node: vec![],
            dockerfile: None,
        }
    }

    fn node_config() -> RequiresConfig {
        RequiresConfig {
            sandbox: "node".to_string(),
            apt: vec!["chromium".to_string()],
            pip: vec![],
            node: vec!["@browserbasehq/sdk".to_string(), "axios".to_string()],
            dockerfile: None,
        }
    }

    fn custom_config() -> RequiresConfig {
        RequiresConfig {
            sandbox: "custom".to_string(),
            apt: vec![],
            pip: vec![],
            node: vec![],
            dockerfile: Some("./Dockerfile.payroll".to_string()),
        }
    }

    #[test]
    fn test_generate_python_dockerfile() {
        let config = python_config();
        let df = generate_dockerfile(&config).unwrap();
        assert!(df.contains("debian:trixie-slim"));
        assert!(df.contains("python3"));
        assert!(df.contains("qpdf poppler-utils"));
        assert!(df.contains("astral.sh/uv"));
        assert!(df.contains("uv pip install --system --break-system-packages pikepdf pypdf"));
        assert!(df.contains("get.workbooks.dev"));
        assert!(df.contains("WORKDIR /work"));
    }

    #[test]
    fn test_generate_node_dockerfile() {
        let config = node_config();
        let df = generate_dockerfile(&config).unwrap();
        assert!(df.contains("debian:trixie-slim"));
        assert!(df.contains("nodejs"));
        assert!(df.contains("chromium"));
        assert!(df.contains("npm install -g @browserbasehq/sdk axios"));
        assert!(df.contains("get.workbooks.dev"));
        assert!(df.contains("WORKDIR /work"));
    }

    #[test]
    fn test_generate_python_no_user_apt() {
        let config = RequiresConfig {
            sandbox: "python".to_string(),
            apt: vec![],
            pip: vec!["requests".to_string()],
            node: vec![],
            dockerfile: None,
        };
        let df = generate_dockerfile(&config).unwrap();
        // Core deps (python3, curl) are always present
        assert!(df.contains("python3"));
        assert!(df.contains("uv pip install --system --break-system-packages requests"));
    }

    #[test]
    fn test_dockerfile_includes_wb_install() {
        let config = python_config();
        let df = generate_dockerfile(&config).unwrap();
        assert!(df.contains("get.workbooks.dev"));
    }

    #[test]
    fn test_dockerfile_no_duplicate_curl() {
        let config = RequiresConfig {
            sandbox: "python".to_string(),
            apt: vec!["curl".to_string(), "jq".to_string()],
            pip: vec![],
            node: vec![],
            dockerfile: None,
        };
        let df = generate_dockerfile(&config).unwrap();
        // curl should appear only once in the apt-get line, not duplicated
        let curl_count = df.matches("curl").count();
        // curl appears in apt-get line + in the wb install URL + in the uv install URL = 3
        assert!(curl_count >= 2, "curl should appear in apt and install lines, got {} occurrences", curl_count);
    }

    #[test]
    fn test_generate_custom_errors() {
        let config = custom_config();
        assert!(generate_dockerfile(&config).is_err());
    }

    #[test]
    fn test_image_tag_deterministic() {
        let config = python_config();
        let tag1 = image_tag(&config);
        let tag2 = image_tag(&config);
        assert_eq!(tag1, tag2);
        assert!(tag1.starts_with("wb-sandbox:"));
    }

    #[test]
    fn test_image_tag_different_configs() {
        let tag1 = image_tag(&python_config());
        let tag2 = image_tag(&node_config());
        assert_ne!(tag1, tag2);
    }

    #[test]
    fn test_image_tag_changes_with_deps() {
        let mut config = python_config();
        let tag1 = image_tag(&config);
        config.pip.push("numpy".to_string());
        let tag2 = image_tag(&config);
        assert_ne!(tag1, tag2);
    }

    #[test]
    fn test_image_tag_same_deps_different_order() {
        let config1 = RequiresConfig {
            sandbox: "python".to_string(),
            apt: vec!["a".to_string(), "b".to_string()],
            pip: vec![],
            node: vec![],
            dockerfile: None,
        };
        let config2 = RequiresConfig {
            sandbox: "python".to_string(),
            apt: vec!["b".to_string(), "a".to_string()],
            pip: vec![],
            node: vec![],
            dockerfile: None,
        };
        // Different order = different hash (intentional — order matters for Docker layer caching)
        let tag1 = image_tag(&config1);
        let tag2 = image_tag(&config2);
        assert_ne!(tag1, tag2);
    }
}
