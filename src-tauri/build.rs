use std::path::{Path, PathBuf};
use std::process::Command;

const DAEMON_PACKAGE: &str = "uc-daemon";
const DAEMON_BINARY: &str = "uniclipboard-daemon";
const DAEMON_BUILD_INPUT_CRATES: &[&str] = &[
    "uc-daemon",
    "uc-bootstrap",
    "uc-app",
    "uc-core",
    "uc-infra",
    "uc-platform",
    "uc-observability",
];

fn main() {
    prepare_daemon_sidecar().unwrap_or_else(|error| {
        panic!("failed to prepare daemon sidecar: {error}");
    });
    tauri_build::build();
}

fn prepare_daemon_sidecar() -> Result<(), String> {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let target_triple = std::env::var("TARGET")
        .or_else(|_| std::env::var("TAURI_ENV_TARGET_TRIPLE"))
        .unwrap_or_else(|_| construct_triple_from_cfg());

    emit_rerun_directives(&manifest_dir);

    let built_binary = build_daemon_binary(&manifest_dir, &profile, &target_triple)?;
    stage_daemon_binary(&manifest_dir, &built_binary, &target_triple)?;
    Ok(())
}

fn emit_rerun_directives(manifest_dir: &Path) {
    for env_key in ["CARGO", "PROFILE", "TARGET", "TAURI_ENV_TARGET_TRIPLE"] {
        println!("cargo:rerun-if-env-changed={env_key}");
    }

    for path in [
        manifest_dir.join("Cargo.toml"),
        manifest_dir.join("Cargo.lock"),
    ] {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    for crate_name in DAEMON_BUILD_INPUT_CRATES {
        for path in [
            manifest_dir
                .join("crates")
                .join(crate_name)
                .join("Cargo.toml"),
            manifest_dir.join("crates").join(crate_name).join("src"),
        ] {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

fn build_daemon_binary(
    manifest_dir: &Path,
    profile: &str,
    target_triple: &str,
) -> Result<PathBuf, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let daemon_target_dir = manifest_dir.join("target").join("daemon-sidecar");

    let mut command = Command::new(cargo);
    command
        .current_dir(manifest_dir)
        .arg("build")
        .arg("-p")
        .arg(DAEMON_PACKAGE)
        .arg("--bin")
        .arg(DAEMON_BINARY)
        .arg("--target")
        .arg(target_triple)
        .arg("--target-dir")
        .arg(&daemon_target_dir);

    match profile {
        "debug" => {}
        "release" => {
            command.arg("--release");
        }
        other => {
            command.arg("--profile").arg(other);
        }
    }

    let output = command
        .output()
        .map_err(|error| format!("failed to invoke cargo for daemon build: {error}"))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!(
            "daemon build command failed with status {}.\nstdout:\n{}\nstderr:\n{}",
            output.status, stdout, stderr
        ));
    }

    let built_binary = daemon_target_dir
        .join(target_triple)
        .join(profile)
        .join(daemon_binary_name());

    if !built_binary.exists() {
        return Err(format!(
            "daemon build finished without producing {}",
            built_binary.display()
        ));
    }

    Ok(built_binary)
}

fn stage_daemon_binary(
    manifest_dir: &Path,
    built_binary: &Path,
    target_triple: &str,
) -> Result<(), String> {
    let binaries_dir = manifest_dir.join("binaries");
    std::fs::create_dir_all(&binaries_dir).map_err(|error| {
        format!(
            "failed to create Tauri binaries directory {}: {error}",
            binaries_dir.display()
        )
    })?;

    let dest = binaries_dir.join(format!(
        "{DAEMON_BINARY}-{target_triple}{}",
        executable_suffix()
    ));
    std::fs::copy(built_binary, &dest).map_err(|error| {
        format!(
            "failed to stage daemon sidecar from {} to {}: {error}",
            built_binary.display(),
            dest.display()
        )
    })?;

    println!("cargo:warning=Daemon binary staged to {}", dest.display());
    Ok(())
}

fn daemon_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "uniclipboard-daemon.exe"
    } else {
        "uniclipboard-daemon"
    }
}

fn executable_suffix() -> &'static str {
    if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    }
}

fn construct_triple_from_cfg() -> String {
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    match (arch.as_str(), os.as_str(), env.as_str()) {
        ("aarch64", "macos", _) => "aarch64-apple-darwin".to_string(),
        ("x86_64", "macos", _) => "x86_64-apple-darwin".to_string(),
        ("x86_64", "linux", "gnu") => "x86_64-unknown-linux-gnu".to_string(),
        ("aarch64", "linux", "gnu") => "aarch64-unknown-linux-gnu".to_string(),
        ("x86_64", "windows", "msvc") => "x86_64-pc-windows-msvc".to_string(),
        ("aarch64", "windows", "msvc") => "aarch64-pc-windows-msvc".to_string(),
        _ => format!("{arch}-unknown-{os}-{env}"),
    }
}
