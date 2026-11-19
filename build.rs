fn main() {
    emit_build_metadata();
    #[cfg(windows)]
    windows_resource();
}

/// Record the profile, target triple and (when available) the source commit so
/// Help > About can show an honest build line without any network access.
fn emit_build_metadata() {
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());
    let commit = git_commit().unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=BEAMBURIN_TARGET={target}");
    println!("cargo:rustc-env=BEAMBURIN_PROFILE={profile}");
    println!("cargo:rustc-env=BEAMBURIN_COMMIT={commit}");
}

fn git_commit() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let commit = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!commit.is_empty()).then_some(commit)
}

#[cfg(windows)]
fn windows_resource() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/icons/beamburin.ico");
    if let Err(error) = res.compile() {
        eprintln!("winres: {error}");
    }
}
