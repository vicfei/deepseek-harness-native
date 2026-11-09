//! Sidecar provisioning: stage the pinned official-bundle closure and a
//! minimal harness home, then install node_modules.
//!
//! The staged manifest and bootstrap are the SAME files proven by the P0
//! spike (`examples/spike-sidecar/`) — embedded at compile time so the binary
//! is self-sufficient and the repo keeps a single source of truth.
//!
//! Install prefers `bun` when present (the 306-package closure installs in
//! ~7s vs minutes), falls back to `npm` with `--legacy-peer-deps` (the rc
//! package set pins peers strictly). Registry mirrors can be passed through.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const SIDECAR_PACKAGE_JSON: &str = include_str!("../../../examples/spike-sidecar/package.json");
const SIDECAR_BOOT_MJS: &str = include_str!("../../../examples/spike-sidecar/boot.mjs");
const PROFILE_PACKAGE_JSON: &str =
    include_str!("../../../examples/spike-sidecar/home/profiles/headless/package.json");

pub fn provision(sidecar_dir: &Path, registry: Option<&str>) -> Result<PathBuf, String> {
    if sidecar_dir.join("node_modules").is_dir() {
        return Ok(sidecar_dir.to_path_buf());
    }
    fs::create_dir_all(sidecar_dir).map_err(|e| format!("mkdir {}: {e}", sidecar_dir.display()))?;
    fs::write(sidecar_dir.join("package.json"), SIDECAR_PACKAGE_JSON)
        .map_err(|e| format!("write manifest: {e}"))?;
    fs::write(sidecar_dir.join("boot.mjs"), SIDECAR_BOOT_MJS)
        .map_err(|e| format!("write boot.mjs: {e}"))?;

    // Minimal harness home: one headless profile; skills/ starts empty and is
    // where users drop or symlink SKILL.md directories.
    let home = sidecar_dir
        .parent()
        .ok_or("sidecar dir has no parent")?
        .join("home");
    let profile = home.join("profiles/headless");
    fs::create_dir_all(&profile).map_err(|e| format!("mkdir profile: {e}"))?;
    fs::write(profile.join("package.json"), PROFILE_PACKAGE_JSON)
        .map_err(|e| format!("write profile manifest: {e}"))?;
    fs::write(
        profile.join("cordis.yml"),
        "# dshn profile root — empty entry list\n[]\n",
    )
    .map_err(|e| format!("write cordis.yml: {e}"))?;
    fs::write(
        profile.join("cordis.patch.yml"),
        "# dshn user patch layer\n[]\n",
    )
    .map_err(|e| format!("write patch layer: {e}"))?;
    fs::create_dir_all(home.join("skills")).map_err(|e| format!("mkdir skills: {e}"))?;

    // Install the closure. Both installers see the same manifest; bun's
    // postinstall blocking is fine here (native bits ship as optional
    // platform prebuilds, not install scripts).
    let bun = which("bun");
    let (program, args): (&str, Vec<String>) = if bun.is_some() {
        let mut a = vec!["install".to_string()];
        if let Some(r) = registry {
            a.push("--registry".into());
            a.push(r.into());
        }
        ("bun", a)
    } else {
        let mut a = vec!["install".to_string(), "--legacy-peer-deps".into()];
        if let Some(r) = registry {
            a.push("--registry".into());
            a.push(r.into());
        }
        ("npm", a)
    };

    let status = Command::new(program)
        .args(&args)
        .current_dir(sidecar_dir)
        .status()
        .map_err(|e| format!("run {program}: {e}"))?;
    if !status.success() {
        return Err(format!("{program} {} failed", args.join(" ")));
    }
    Ok(sidecar_dir.to_path_buf())
}

fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}
