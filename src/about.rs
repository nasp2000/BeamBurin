//! Version, build and project link shown by Help > About.
//!
//! The values are compile-time constants: the dialog never contacts the
//! network and there is no update check.

/// Public project page.
pub const REPOSITORY: &str = "https://github.com/nasp2000/BeamBurin";

/// Licence expression used by the package.
pub const LICENSE: &str = "MIT OR Apache-2.0";

/// This build deliberately has no automatic update check.
pub const UPDATE_CHECK: bool = false;

/// What the About dialog shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AboutInfo {
    pub version: &'static str,
    pub profile: &'static str,
    pub target: &'static str,
    pub commit: Option<&'static str>,
    pub repository: &'static str,
    pub license: &'static str,
}

/// Metadata recorded by `build.rs` when the binary was compiled.
pub fn info() -> AboutInfo {
    AboutInfo {
        version: env!("CARGO_PKG_VERSION"),
        profile: option_env!("BEAMBURIN_PROFILE").unwrap_or(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }),
        target: option_env!("BEAMBURIN_TARGET").unwrap_or(std::env::consts::ARCH),
        commit: option_env!("BEAMBURIN_COMMIT")
            .filter(|commit| !commit.is_empty() && *commit != "unknown"),
        repository: REPOSITORY,
        license: LICENSE,
    }
}

impl AboutInfo {
    /// One readable line with the profile, target and, when known, the commit.
    pub fn build_line(&self) -> String {
        match self.commit {
            Some(commit) => format!("{} build · {} · commit {commit}", self.profile, self.target),
            None => format!("{} build · {}", self.profile, self.target),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_the_package() {
        assert_eq!(info().version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn the_project_link_and_licence_are_fixed() {
        let about = info();
        assert_eq!(about.repository, "https://github.com/nasp2000/BeamBurin");
        assert_eq!(about.license, "MIT OR Apache-2.0");
    }

    #[test]
    fn the_build_line_names_the_profile_and_target() {
        let about = info();
        let line = about.build_line();
        assert!(line.contains(about.profile), "{line}");
        assert!(line.contains(about.target), "{line}");
        assert!(line.contains("build"), "{line}");
        if let Some(commit) = about.commit {
            assert!(line.contains(commit), "{line}");
        }
    }
}
