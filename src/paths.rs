use std::env;
use std::path::{Path, PathBuf};

pub fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn ephor_home() -> PathBuf {
    env::var_os("EPHOR_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join("f").join("ephor"))
}

/// §FS-001-forge-interface.5: `$XDG_CONFIG_HOME/ephor` or `~/.config/ephor` —
/// a person's own configuration lives here, never in the checkout.
pub fn config_dir() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("ephor")
}

/// Resolve a configuration file: the user config directory when it holds one,
/// else the legacy in-checkout location, which stays supported so an existing
/// setup keeps working.
pub fn resolve_config(file: &str) -> PathBuf {
    let user = config_dir().join(file);
    if user.is_file() {
        return user;
    }
    let legacy = ephor_home().join("config").join(file);
    if legacy.is_file() {
        return legacy;
    }
    // Neither exists: name the preferred location so the error points at the
    // file the user should create.
    user
}

pub fn default_registry_path() -> PathBuf {
    env::var_os("EPHOR_REGISTRY")
        .map(PathBuf::from)
        .unwrap_or_else(|| resolve_config("workspaces.json"))
}

/// State directory for feed caches: $XDG_STATE_HOME/ephor or ~/.local/state/ephor.
#[allow(dead_code)] // wired up by the status feed
pub fn state_dir() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home_dir().join(".local").join("state"))
        .join("ephor")
}

#[allow(dead_code)] // wired up by the status feed
pub fn secrets_dir() -> PathBuf {
    home_dir().join("config").join("secrets").join("ephor")
}

/// Expand `~` and `$VAR`/`${VAR}` like Python's expanduser + expandvars:
/// unknown variables are left untouched.
pub fn expand_user_vars(value: &str) -> String {
    expand_vars(&expand_user(value))
}

pub fn resolve_path(value: &str) -> PathBuf {
    PathBuf::from(expand_user_vars(value))
}

/// Resolve a path from the registry relative to the registry file's directory.
/// Absolute paths pass through unchanged.
pub fn resolve_registry_relative(registry_path: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        return path;
    }
    registry_path.parent().unwrap_or(Path::new(".")).join(path)
}

fn expand_user(value: &str) -> String {
    if value == "~" {
        return home_dir().to_string_lossy().into_owned();
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return format!("{}/{}", home_dir().to_string_lossy(), rest);
    }
    value.to_string()
}

fn expand_vars(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut chars = value.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if ch != '$' {
            result.push(ch);
            continue;
        }
        let rest = &value[idx + 1..];
        if let Some(stripped) = rest.strip_prefix('{') {
            if let Some(end) = stripped.find('}') {
                let name = &stripped[..end];
                match env::var(name) {
                    Ok(val) => result.push_str(&val),
                    Err(_) => {
                        result.push_str("${");
                        result.push_str(name);
                        result.push('}');
                    }
                }
                for _ in 0..name.len() + 2 {
                    chars.next();
                }
                continue;
            }
            result.push(ch);
            continue;
        }
        let name_len = rest
            .char_indices()
            .take_while(|(_, c)| c.is_ascii_alphanumeric() || *c == '_')
            .map(|(i, c)| i + c.len_utf8())
            .last()
            .unwrap_or(0);
        if name_len == 0 {
            result.push(ch);
            continue;
        }
        let name = &rest[..name_len];
        match env::var(name) {
            Ok(val) => result.push_str(&val),
            Err(_) => {
                result.push('$');
                result.push_str(name);
            }
        }
        for _ in 0..name.len() {
            chars.next();
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_braced_and_plain_vars() {
        env::set_var("EPHOR_TEST_VAR", "xyz");
        assert_eq!(expand_user_vars("$EPHOR_TEST_VAR/a"), "xyz/a");
        assert_eq!(expand_user_vars("${EPHOR_TEST_VAR}/a"), "xyz/a");
        assert_eq!(
            expand_user_vars("$EPHOR_MISSING_VAR/a"),
            "$EPHOR_MISSING_VAR/a"
        );
    }

    #[test]
    fn expands_tilde() {
        let home = home_dir().to_string_lossy().into_owned();
        assert_eq!(expand_user_vars("~/x"), format!("{home}/x"));
    }
}
