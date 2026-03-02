use super::model::UpdateChannel;

/// Detects the update channel from a version string by inspecting the semver pre-release tag.
///
/// The version string is expected to follow semver conventions, e.g. `"0.1.0"` or
/// `"0.1.0-alpha.1"`. The pre-release segment (the part after the first `-`) is matched
/// case-insensitively against known channel names.
///
/// # Rules
///
/// | Pre-release segment | Returned channel    |
/// |---------------------|---------------------|
/// | *(absent)*          | `UpdateChannel::Stable` |
/// | `alpha`             | `UpdateChannel::Alpha`  |
/// | `beta`              | `UpdateChannel::Beta`   |
/// | `rc`                | `UpdateChannel::Rc`     |
/// | *(anything else)*   | `UpdateChannel::Stable` |
///
/// An empty input string also returns `UpdateChannel::Stable`.
///
/// # Examples
///
/// ```
/// use uc_core::settings::channel::detect_channel;
/// use uc_core::settings::model::UpdateChannel;
///
/// assert_eq!(detect_channel("0.1.0"), UpdateChannel::Stable);
/// assert_eq!(detect_channel("0.1.0-alpha.1"), UpdateChannel::Alpha);
/// assert_eq!(detect_channel("0.1.0-beta.2"), UpdateChannel::Beta);
/// assert_eq!(detect_channel("0.1.0-rc.1"), UpdateChannel::Rc);
/// assert_eq!(detect_channel("0.1.0-unknown.1"), UpdateChannel::Stable);
/// assert_eq!(detect_channel(""), UpdateChannel::Stable);
/// ```
pub fn detect_channel(version: &str) -> UpdateChannel {
    let prerelease = match version.find('-') {
        Some(idx) => &version[idx + 1..],
        None => return UpdateChannel::Stable,
    };

    // Extract the first segment before any `.` within the pre-release tag.
    let tag = match prerelease.find('.') {
        Some(idx) => &prerelease[..idx],
        None => prerelease,
    };

    match tag.to_ascii_lowercase().as_str() {
        "alpha" => UpdateChannel::Alpha,
        "beta" => UpdateChannel::Beta,
        "rc" => UpdateChannel::Rc,
        _ => UpdateChannel::Stable,
    }
}

#[cfg(test)]
mod tests {
    use super::detect_channel;
    use crate::settings::model::UpdateChannel;

    #[test]
    fn test_stable_release() {
        assert_eq!(detect_channel("0.1.0"), UpdateChannel::Stable);
    }

    #[test]
    fn test_alpha_release() {
        assert_eq!(detect_channel("0.1.0-alpha.1"), UpdateChannel::Alpha);
    }

    #[test]
    fn test_beta_release() {
        assert_eq!(detect_channel("0.1.0-beta.2"), UpdateChannel::Beta);
    }

    #[test]
    fn test_rc_release() {
        assert_eq!(detect_channel("0.1.0-rc.1"), UpdateChannel::Rc);
    }

    #[test]
    fn test_unknown_prerelease_returns_stable() {
        assert_eq!(detect_channel("0.1.0-unknown.1"), UpdateChannel::Stable);
    }

    #[test]
    fn test_empty_string_returns_stable() {
        assert_eq!(detect_channel(""), UpdateChannel::Stable);
    }

    #[test]
    fn test_prerelease_without_dot_suffix() {
        assert_eq!(detect_channel("1.0.0-alpha"), UpdateChannel::Alpha);
        assert_eq!(detect_channel("1.0.0-beta"), UpdateChannel::Beta);
        assert_eq!(detect_channel("1.0.0-rc"), UpdateChannel::Rc);
    }

    #[test]
    fn test_case_insensitive_matching() {
        assert_eq!(detect_channel("1.0.0-Alpha.1"), UpdateChannel::Alpha);
        assert_eq!(detect_channel("1.0.0-BETA.1"), UpdateChannel::Beta);
        assert_eq!(detect_channel("1.0.0-RC.1"), UpdateChannel::Rc);
    }
}
