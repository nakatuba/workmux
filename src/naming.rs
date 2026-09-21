use anyhow::{Result, bail};
use slug::slugify;

use crate::config::Config;

/// Derives the "handle" (worktree dir name + tmux window base name)
/// from the branch name, optional explicit override, and config.
///
/// The branch-derived part is always slugified to ensure filesystem/tmux
/// compatibility. A configured `worktree_prefix` is kept verbatim so
/// separators such as `=` survive, and is validated instead.
///
/// Priority:
/// 1. Explicit name (--name flag) - bypasses all config (including prefix)
/// 2. Config-based derivation: worktree_naming strategy + worktree_prefix
/// 3. Branch name as-is (default fallback)
pub fn derive_handle(
    branch_name: &str,
    explicit_name: Option<&str>,
    config: &Config,
) -> Result<String> {
    let handle = if let Some(name) = explicit_name {
        derive_target_name(name)?
    } else {
        // Apply naming strategy, slugifying only the branch-derived part
        let derived = slugify(config.worktree_naming.derive_name(branch_name));

        // Apply prefix if configured
        if let Some(ref prefix) = config.worktree_prefix {
            validate_worktree_prefix(prefix)?;
            format!("{}{}", prefix, derived)
        } else {
            derived
        }
    };

    validate_handle(&handle)?;
    Ok(handle)
}

/// Validates a configured `worktree_prefix`.
///
/// The prefix is inserted verbatim into worktree directory names and
/// multiplexer window/session names, so it is restricted to characters that
/// are safe for both: ASCII alphanumerics, `-`, `_` and `=`. This rejects
/// path separators, tmux-hostile `.`/`:`, and whitespace.
fn validate_worktree_prefix(prefix: &str) -> Result<()> {
    if prefix.contains('{') || prefix.contains('}') {
        bail!(
            "worktree_prefix '{}' contains an unknown placeholder \
             (only '{{project}}' is supported)",
            prefix
        );
    }

    // A leading '-' looks like a CLI flag, and tmux treats leading '=' and '$'
    // as target name syntax.
    if prefix.starts_with(['-', '=', '$']) {
        bail!(
            "worktree_prefix '{}' cannot start with '-', '=' or '$'",
            prefix
        );
    }

    if let Some(invalid) = prefix
        .chars()
        .find(|c| !crate::util::is_worktree_prefix_char(*c))
    {
        bail!(
            "worktree_prefix '{}' contains an invalid character '{}' \
             (allowed: A-Z, a-z, 0-9, '-', '_', '=')",
            prefix,
            invalid
        );
    }

    Ok(())
}

pub fn derive_target_name(name: &str) -> Result<String> {
    let handle = slugify(name);
    validate_handle(&handle)?;
    Ok(handle)
}

pub fn validate_parent_session(name: &str) -> Result<String> {
    if name.is_empty() {
        bail!("Parent session cannot be empty");
    }
    if name.contains(':') {
        bail!("Parent session cannot contain ':'");
    }
    if name.starts_with('$') || name.starts_with('=') {
        bail!("Parent session cannot start with '$' or '='");
    }
    if name.chars().any(char::is_control) {
        bail!("Parent session cannot contain control characters");
    }

    Ok(name.to_string())
}

/// Validates that a handle is safe for filesystem and tmux use.
fn validate_handle(handle: &str) -> Result<()> {
    if handle.is_empty() {
        bail!("Handle cannot be empty");
    }

    // Slugify should have removed these, but double check for safety
    if handle.contains("..") || handle.starts_with('/') {
        bail!("Handle cannot contain path traversal");
    }

    if handle.chars().any(char::is_whitespace) {
        bail!("Handle cannot contain whitespace");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WorktreeNaming;

    fn default_config() -> Config {
        Config::default()
    }

    fn config_with_basename() -> Config {
        Config {
            worktree_naming: WorktreeNaming::Basename,
            ..Config::default()
        }
    }

    fn config_with_prefix(prefix: &str) -> Config {
        Config {
            worktree_prefix: Some(prefix.to_string()),
            ..Config::default()
        }
    }

    fn config_with_basename_and_prefix(prefix: &str) -> Config {
        Config {
            worktree_naming: WorktreeNaming::Basename,
            worktree_prefix: Some(prefix.to_string()),
            ..Config::default()
        }
    }

    // === Explicit name tests (bypass all config) ===

    #[test]
    fn derive_handle_explicit_name() {
        let result =
            derive_handle("prj-4120/feature", Some("cool-feature"), &default_config()).unwrap();
        assert_eq!(result, "cool-feature");
    }

    #[test]
    fn derive_handle_explicit_name_with_spaces() {
        let result = derive_handle("branch", Some("My Cool Feature"), &default_config()).unwrap();
        assert_eq!(result, "my-cool-feature");
    }

    #[test]
    fn derive_handle_explicit_name_with_special_chars() {
        let result = derive_handle("branch", Some("Feature! @#$%"), &default_config()).unwrap();
        assert_eq!(result, "feature");
    }

    #[test]
    fn derive_handle_explicit_name_bypasses_prefix() {
        let result = derive_handle("branch", Some("custom"), &config_with_prefix("web-")).unwrap();
        assert_eq!(result, "custom"); // NOT web-custom
    }

    #[test]
    fn derive_handle_explicit_name_bypasses_basename() {
        let result = derive_handle("prj/feature", Some("custom"), &config_with_basename()).unwrap();
        assert_eq!(result, "custom"); // NOT feature
    }

    // === Default (full) strategy tests ===

    #[test]
    fn derive_handle_branch_name_slugified() {
        let result = derive_handle("prj-4120/create-new-tags", None, &default_config()).unwrap();
        assert_eq!(result, "prj-4120-create-new-tags");
    }

    #[test]
    fn derive_handle_simple_branch() {
        let result = derive_handle("main", None, &default_config()).unwrap();
        assert_eq!(result, "main");
    }

    #[test]
    fn derive_handle_nested_branch() {
        let result = derive_handle("feature/auth/oauth", None, &default_config()).unwrap();
        assert_eq!(result, "feature-auth-oauth");
    }

    // === Basename strategy tests ===

    #[test]
    fn derive_handle_basename_extracts_last_segment() {
        let result = derive_handle("prj-4120/feature", None, &config_with_basename()).unwrap();
        assert_eq!(result, "feature");
    }

    #[test]
    fn derive_handle_basename_handles_trailing_slash() {
        let result = derive_handle("prj-4120/feature/", None, &config_with_basename()).unwrap();
        assert_eq!(result, "feature");
    }

    #[test]
    fn derive_handle_basename_simple_branch_unchanged() {
        let result = derive_handle("main", None, &config_with_basename()).unwrap();
        assert_eq!(result, "main");
    }

    #[test]
    fn derive_handle_basename_multiple_segments() {
        let result = derive_handle("prj/sub/feature", None, &config_with_basename()).unwrap();
        assert_eq!(result, "feature");
    }

    // === Prefix tests ===

    #[test]
    fn derive_handle_prefix_applied() {
        let result = derive_handle("feature", None, &config_with_prefix("web-")).unwrap();
        assert_eq!(result, "web-feature");
    }

    #[test]
    fn derive_handle_prefix_with_slash_branch() {
        let result = derive_handle("prj/feature", None, &config_with_prefix("api-")).unwrap();
        assert_eq!(result, "api-prj-feature");
    }

    #[test]
    fn derive_handle_prefix_preserves_equals_separator() {
        let result =
            derive_handle("feat/add-login", None, &config_with_prefix("workmux=")).unwrap();
        assert_eq!(result, "workmux=feat-add-login");
    }

    #[test]
    fn derive_handle_prefix_preserves_underscore_and_case() {
        let result = derive_handle("feature", None, &config_with_prefix("Web_")).unwrap();
        assert_eq!(result, "Web_feature");
    }

    #[test]
    fn derive_handle_prefix_rejects_whitespace() {
        let err = derive_handle("feature", None, &config_with_prefix("web ")).unwrap_err();
        assert!(err.to_string().contains("invalid character"), "{err}");
    }

    #[test]
    fn derive_handle_prefix_rejects_path_separator() {
        let err = derive_handle("feature", None, &config_with_prefix("web/")).unwrap_err();
        assert!(err.to_string().contains("invalid character"), "{err}");
    }

    #[test]
    fn derive_handle_prefix_rejects_tmux_hostile_characters() {
        for prefix in ["web.", "web:"] {
            let err = derive_handle("feature", None, &config_with_prefix(prefix)).unwrap_err();
            assert!(err.to_string().contains("invalid character"), "{err}");
        }
    }

    #[test]
    fn derive_handle_prefix_rejects_leading_target_syntax() {
        for prefix in ["-web", "=web", "$web"] {
            let err = derive_handle("feature", None, &config_with_prefix(prefix)).unwrap_err();
            assert!(err.to_string().contains("cannot start with"), "{err}");
        }
    }

    #[test]
    fn derive_handle_prefix_rejects_unexpanded_placeholder() {
        let err = derive_handle("feature", None, &config_with_prefix("{proj}=")).unwrap_err();
        assert!(err.to_string().contains("unknown placeholder"), "{err}");
    }

    // === Combined basename + prefix tests ===

    #[test]
    fn derive_handle_basename_and_prefix() {
        let result = derive_handle(
            "prj-4120/feature",
            None,
            &config_with_basename_and_prefix("web-"),
        )
        .unwrap();
        assert_eq!(result, "web-feature");
    }

    #[test]
    fn derive_handle_basename_and_prefix_simple_branch() {
        let result =
            derive_handle("feature", None, &config_with_basename_and_prefix("api-")).unwrap();
        assert_eq!(result, "api-feature");
    }

    #[test]
    fn derive_handle_basename_and_project_style_prefix() {
        let result = derive_handle(
            "prj-4120/feature",
            None,
            &config_with_basename_and_prefix("workmux="),
        )
        .unwrap();
        assert_eq!(result, "workmux=feature");
    }

    // === Error cases ===

    #[test]
    fn derive_handle_empty_explicit_name_fails() {
        let result = derive_handle("branch", Some(""), &default_config());
        assert!(result.is_err());
    }

    #[test]
    fn validate_parent_session_preserves_valid_name() {
        let result = validate_parent_session("WalkingMate").unwrap();
        assert_eq!(result, "WalkingMate");
    }

    #[test]
    fn validate_parent_session_rejects_empty_name() {
        assert!(validate_parent_session("").is_err());
    }

    #[test]
    fn validate_parent_session_rejects_tmux_target_syntax() {
        for name in ["parent:window", "$1", "=parent"] {
            assert!(validate_parent_session(name).is_err(), "accepted {name:?}");
        }
    }

    #[test]
    fn validate_parent_session_rejects_control_characters() {
        assert!(validate_parent_session("parent\nsession").is_err());
    }

    #[test]
    fn validate_handle_empty_fails() {
        let result = validate_handle("");
        assert!(result.is_err());
    }

    #[test]
    fn validate_handle_valid() {
        let result = validate_handle("my-feature");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_handle_with_numbers() {
        let result = validate_handle("feature-123");
        assert!(result.is_ok());
    }

    // === WorktreeNaming::derive_name tests ===

    #[test]
    fn worktree_naming_full_preserves_branch() {
        assert_eq!(
            WorktreeNaming::Full.derive_name("prj/feature"),
            "prj/feature"
        );
    }

    #[test]
    fn worktree_naming_basename_extracts_last() {
        assert_eq!(
            WorktreeNaming::Basename.derive_name("prj/feature"),
            "feature"
        );
    }

    #[test]
    fn worktree_naming_basename_handles_trailing_slash() {
        assert_eq!(
            WorktreeNaming::Basename.derive_name("prj/feature/"),
            "feature"
        );
    }

    #[test]
    fn worktree_naming_basename_simple_branch() {
        assert_eq!(WorktreeNaming::Basename.derive_name("main"), "main");
    }
}
