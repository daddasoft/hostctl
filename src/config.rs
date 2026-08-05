use crate::storage;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs, io,
    net::IpAddr,
    path::{Path, PathBuf},
};

pub const CONFIG_VERSION: u32 = 1;
const START_MARKER: &str = "# >>> hostctl managed >>>";
const END_MARKER: &str = "# <<< hostctl managed <<<";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectConfig {
    pub version: u32,
    pub active_profile: Option<String>,
    pub groups: BTreeMap<String, HostGroup>,
    pub profiles: BTreeMap<String, Profile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HostGroup {
    pub enabled: bool,
    pub entries: Vec<ManagedEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Profile {
    pub groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedEntry {
    pub ip: String,
    pub hostnames: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            active_profile: None,
            groups: BTreeMap::new(),
            profiles: BTreeMap::new(),
        }
    }
}

impl Default for HostGroup {
    fn default() -> Self {
        Self {
            enabled: true,
            entries: Vec::new(),
        }
    }
}

impl ProjectConfig {
    pub fn validate(&self) -> io::Result<()> {
        if self.version != CONFIG_VERSION {
            return invalid(format!(
                "unsupported config version {}; expected {CONFIG_VERSION}",
                self.version
            ));
        }
        if let Some(active) = &self.active_profile
            && !self.profiles.contains_key(active)
        {
            return invalid(format!("active profile '{active}' does not exist"));
        }
        for (name, group) in &self.groups {
            validate_name("group", name)?;
            let mut seen = HashSet::new();
            for entry in &group.entries {
                entry.ip.parse::<IpAddr>().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("group '{name}' contains invalid IP '{}'", entry.ip),
                    )
                })?;
                if entry.hostnames.is_empty() {
                    return invalid(format!(
                        "group '{name}' contains an entry without hostnames"
                    ));
                }
                for hostname in &entry.hostnames {
                    crate::hosts::validate_hostname(hostname)?;
                    if !seen.insert(hostname.to_ascii_lowercase()) {
                        return invalid(format!(
                            "group '{name}' contains duplicate hostname '{hostname}'"
                        ));
                    }
                }
                if entry
                    .comment
                    .as_deref()
                    .is_some_and(|value| value.chars().any(char::is_control))
                {
                    return invalid(format!("group '{name}' contains an invalid comment"));
                }
            }
        }
        for (name, profile) in &self.profiles {
            validate_name("profile", name)?;
            for group in &profile.groups {
                if !self.groups.contains_key(group) {
                    return invalid(format!(
                        "profile '{name}' references missing group '{group}'"
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn effective_groups(&self) -> Vec<(&str, &HostGroup)> {
        let profile_groups = self.active_profile.as_ref().and_then(|name| {
            self.profiles
                .get(name)
                .map(|profile| profile.groups.iter().collect::<HashSet<_>>())
        });
        self.groups
            .iter()
            .filter(|(name, group)| {
                group.enabled
                    && profile_groups
                        .as_ref()
                        .is_none_or(|selected| selected.contains(name))
            })
            .map(|(name, group)| (name.as_str(), group))
            .collect()
    }

    pub fn render_managed_block(&self, ending: &str) -> String {
        let mut lines = vec![START_MARKER.to_string()];
        for (name, group) in self.effective_groups() {
            lines.push(format!("# group: {name}"));
            for entry in &group.entries {
                let mut line = format!("{}\t{}", entry.ip, entry.hostnames.join(" "));
                if let Some(comment) = &entry.comment {
                    line.push_str(&format!("\t# {comment}"));
                }
                lines.push(line);
            }
        }
        lines.push(END_MARKER.to_string());
        lines.join(ending)
    }

    pub fn activate_profile(&mut self, name: &str) -> io::Result<()> {
        if !self.profiles.contains_key(name) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("profile '{name}' not found"),
            ));
        }
        self.active_profile = Some(name.to_string());
        Ok(())
    }
}

pub fn default_config_path() -> io::Result<PathBuf> {
    Ok(std::env::current_dir()?.join(".hostctl.toml"))
}

pub fn load_config(path: &Path) -> io::Result<ProjectConfig> {
    let content = fs::read_to_string(path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("cannot read config '{}': {error}", path.display()),
        )
    })?;
    let config: ProjectConfig = toml::from_str(&content).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid config '{}': {error}", path.display()),
        )
    })?;
    config.validate()?;
    Ok(config)
}

pub fn load_or_default(path: &Path) -> io::Result<ProjectConfig> {
    if path.exists() {
        load_config(path)
    } else {
        Ok(ProjectConfig::default())
    }
}

pub fn save_config(path: &Path, config: &ProjectConfig) -> io::Result<()> {
    config.validate()?;
    let content = toml::to_string_pretty(config)
        .map_err(|error| io::Error::other(format!("cannot serialize config: {error}")))?;
    fs::write(path, content).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("cannot write config '{}': {error}", path.display()),
        )
    })
}

pub fn init_config(path: &Path) -> io::Result<()> {
    if path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("config '{}' already exists", path.display()),
        ));
    }
    save_config(path, &ProjectConfig::default())
}

pub fn apply_config(
    hosts_path: &Path,
    config: &ProjectConfig,
    dry_run: bool,
) -> io::Result<storage::MutationResult> {
    config.validate()?;
    storage::mutate(hosts_path, dry_run, |content| {
        Ok(replace_managed_block(content, config))
    })
}

pub fn managed_entries(config: &ProjectConfig) -> Vec<crate::hosts::Entry> {
    config
        .effective_groups()
        .into_iter()
        .flat_map(|(_, group)| &group.entries)
        .map(|entry| crate::hosts::Entry {
            ip: entry.ip.clone(),
            hostnames: entry.hostnames.clone(),
            comment: entry.comment.clone(),
            disabled: false,
        })
        .collect()
}

fn replace_managed_block(content: &str, config: &ProjectConfig) -> String {
    let ending = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let block = config.render_managed_block(ending);
    if let Some(start) = content.find(START_MARKER)
        && let Some(relative_end) = content[start..].find(END_MARKER)
    {
        let marker_end = start + relative_end + END_MARKER.len();
        return format!("{}{}{}", &content[..start], block, &content[marker_end..]);
    }

    if content.is_empty() {
        return format!("{block}{ending}");
    }
    if content.ends_with('\n') {
        format!("{content}{block}{ending}")
    } else {
        format!("{content}{ending}{block}")
    }
}

fn validate_name(kind: &str, name: &str) -> io::Result<()> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return invalid(format!(
            "{kind} name must be 1-64 letters, digits, hyphens, or underscores"
        ));
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_block_replacement_preserves_manual_content() {
        let mut config = ProjectConfig::default();
        config.groups.insert(
            "dev".to_string(),
            HostGroup {
                enabled: true,
                entries: vec![ManagedEntry {
                    ip: "127.0.0.1".to_string(),
                    hostnames: vec!["app.local".to_string()],
                    comment: None,
                }],
            },
        );
        let first = replace_managed_block("# manual\n10.0.0.1 router\n", &config);
        let second = replace_managed_block(&first, &config);
        assert_eq!(first, second);
        assert!(first.starts_with("# manual\n10.0.0.1 router\n"));
        assert!(first.contains("127.0.0.1\tapp.local"));
    }

    #[test]
    fn config_rejects_missing_profile_group() {
        let mut config = ProjectConfig::default();
        config.profiles.insert(
            "work".to_string(),
            Profile {
                groups: vec!["missing".to_string()],
            },
        );
        assert!(config.validate().is_err());
    }
}
