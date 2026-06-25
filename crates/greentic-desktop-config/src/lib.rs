use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub runner: RunnerConfig,
    pub security: SecurityConfig,
    pub mcp: McpConfig,
    pub evidence: EvidenceConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerConfig {
    pub home: PathBuf,
    pub registry_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecurityConfig {
    pub require_signed_runners: bool,
    pub allow_unsigned_drafts: bool,
    pub require_signed_extensions: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpConfig {
    pub bind: String,
    pub transport: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceConfig {
    pub store: PathBuf,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let home = default_home();

        Self {
            runner: RunnerConfig {
                home: home.clone(),
                registry_url: "https://runners.greentic.cloud".to_owned(),
            },
            security: SecurityConfig {
                require_signed_runners: true,
                allow_unsigned_drafts: true,
                require_signed_extensions: true,
            },
            mcp: McpConfig {
                bind: "127.0.0.1:8799".to_owned(),
                transport: "streamable_http".to_owned(),
            },
            evidence: EvidenceConfig {
                store: home.join("evidence"),
            },
        }
    }
}

impl RuntimeConfig {
    pub fn render_toml(&self) -> String {
        format!(
            "[runner]\nhome = \"{}\"\nregistry_url = \"{}\"\n\n[security]\nrequire_signed_runners = {}\nallow_unsigned_drafts = {}\nrequire_signed_extensions = {}\n\n[mcp]\nbind = \"{}\"\ntransport = \"{}\"\n\n[evidence]\nstore = \"{}\"\n",
            self.runner.home.display(),
            self.runner.registry_url,
            self.security.require_signed_runners,
            self.security.allow_unsigned_drafts,
            self.security.require_signed_extensions,
            self.mcp.bind,
            self.mcp.transport,
            self.evidence.store.display()
        )
    }
}

pub fn default_home() -> PathBuf {
    env::var_os("GREENTIC_DESKTOP_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".greentic/desktop")))
        .unwrap_or_else(|| PathBuf::from(".greentic/desktop"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_pr_01_values() {
        let config = RuntimeConfig::default();
        assert_eq!(config.runner.registry_url, "https://runners.greentic.cloud");
        assert!(config.security.require_signed_runners);
        assert!(config.security.allow_unsigned_drafts);
        assert!(config.security.require_signed_extensions);
        assert_eq!(config.mcp.bind, "127.0.0.1:8799");
        assert_eq!(config.mcp.transport, "streamable_http");
    }

    #[test]
    fn renders_expected_sections() {
        let rendered = RuntimeConfig::default().render_toml();
        assert!(rendered.contains("[runner]"));
        assert!(rendered.contains("[security]"));
        assert!(rendered.contains("[mcp]"));
        assert!(rendered.contains("[evidence]"));
    }
}
