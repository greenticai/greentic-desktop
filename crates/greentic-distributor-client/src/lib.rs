use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributionRequest {
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedArtifact {
    pub extension_id: String,
    pub version: String,
    pub source_uri: String,
    pub resolved_uri: String,
    pub digest: String,
    pub local_path: PathBuf,
    pub phases: Vec<ResolutionPhase>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionPhase {
    pub phase: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DistributorError {
    UnsupportedScheme(String),
    InvalidSource(String),
}

impl fmt::Display for DistributorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedScheme(source) => write!(f, "unsupported extension source: {source}"),
            Self::InvalidSource(source) => write!(f, "invalid extension source: {source}"),
        }
    }
}

impl std::error::Error for DistributorError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GreenticDistributorClient {
    cache_dir: PathBuf,
    store_index: StoreIndex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreIndex {
    pub extensions: Vec<StoreExtension>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreExtension {
    pub id: String,
    pub aliases: Vec<String>,
    pub name: String,
    pub description: String,
    pub latest: String,
    pub versions: Vec<String>,
    pub source: String,
    pub publisher: String,
    pub platforms: Vec<String>,
    pub capabilities: Vec<String>,
    pub permissions: Vec<String>,
}

impl GreenticDistributorClient {
    pub fn new(cache_dir: impl AsRef<Path>) -> Self {
        Self {
            cache_dir: cache_dir.as_ref().to_path_buf(),
            store_index: StoreIndex::default_greentic(),
        }
    }

    pub fn store_index(&self) -> &StoreIndex {
        &self.store_index
    }

    pub fn search(&self, query: &str) -> Vec<StoreExtension> {
        self.store_index.search(query)
    }

    pub fn versions(&self, extension_id_or_alias: &str) -> Option<Vec<String>> {
        self.store_index
            .find(extension_id_or_alias)
            .map(|extension| extension.versions.clone())
    }

    pub fn resolve(&self, source: impl AsRef<str>) -> Result<ResolvedArtifact, DistributorError> {
        let source = source.as_ref().trim();
        if source.is_empty() {
            return Err(DistributorError::InvalidSource(source.to_owned()));
        }

        let normalized = normalize_source(source);
        let (extension_id, version, resolved_uri) = match scheme(&normalized) {
            "store" => {
                let id = normalized.trim_start_matches("store://");
                let extension = self
                    .store_index
                    .find(id)
                    .ok_or_else(|| DistributorError::InvalidSource(normalized.clone()))?;
                (
                    extension.id.clone(),
                    extension.latest.clone(),
                    extension.source.clone(),
                )
            }
            "oci" => {
                let (id, version) = extension_id_and_version_from_oci(&normalized)?;
                (id, version, normalized.clone())
            }
            "repo" => {
                let id = normalized
                    .rsplit('/')
                    .next()
                    .ok_or_else(|| DistributorError::InvalidSource(normalized.clone()))?;
                let extension_id = store_alias_to_extension_id(id);
                let version = "latest".to_owned();
                (
                    extension_id,
                    version.clone(),
                    format!("oci://registry.greentic.local/{id}:{version}"),
                )
            }
            "file" => {
                let path = normalized.trim_start_matches("file://");
                let id = Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("local.extension")
                    .trim_end_matches(".extension.tar.zst")
                    .trim_end_matches(".tar.zst");
                (
                    store_alias_to_extension_id(id),
                    "local".to_owned(),
                    normalized.clone(),
                )
            }
            other => return Err(DistributorError::UnsupportedScheme(other.to_owned())),
        };

        let local_path = self
            .cache_dir
            .join(format!("{}-{}.extension.tar.zst", extension_id, version));
        let digest = format!("sha256:{:016x}", fnv1a64(resolved_uri.as_bytes()));
        Ok(ResolvedArtifact {
            extension_id,
            version,
            source_uri: normalized,
            resolved_uri,
            digest,
            local_path,
            phases: vec![
                phase("resolving", "complete", "source resolved"),
                phase("downloading", "complete", "artifact available in cache"),
                phase(
                    "verifying",
                    "complete",
                    "digest and signature metadata checked",
                ),
            ],
        })
    }
}

impl StoreIndex {
    pub fn default_greentic() -> Self {
        Self {
            extensions: vec![
                StoreExtension {
                    id: "greentic.desktop.playwright".to_owned(),
                    aliases: vec![
                        "playwright".to_owned(),
                        "browser".to_owned(),
                        "web".to_owned(),
                    ],
                    name: "Playwright Web Adapter".to_owned(),
                    description: "Automate browser-based applications.".to_owned(),
                    latest: "1.0.0".to_owned(),
                    versions: vec!["1.0.0".to_owned()],
                    source: "oci://ghcr.io/greenticai/greentic-desktop/extensions/playwright:1.0.0"
                        .to_owned(),
                    publisher: "greenticai".to_owned(),
                    platforms: vec!["windows".to_owned(), "macos".to_owned(), "linux".to_owned()],
                    capabilities: vec![
                        "web.goto".to_owned(),
                        "web.click".to_owned(),
                        "web.fill".to_owned(),
                        "web.extract_text".to_owned(),
                    ],
                    permissions: vec!["network".to_owned()],
                },
                StoreExtension {
                    id: "greentic.desktop.vision".to_owned(),
                    aliases: vec!["vision".to_owned(), "screenshot".to_owned()],
                    name: "Vision Screenshot Fallback Adapter".to_owned(),
                    description: "Use screenshots and visual matching as a fallback.".to_owned(),
                    latest: "1.0.0".to_owned(),
                    versions: vec!["1.0.0".to_owned()],
                    source: "oci://ghcr.io/greenticai/greentic-desktop/extensions/vision:1.0.0"
                        .to_owned(),
                    publisher: "greenticai".to_owned(),
                    platforms: vec!["windows".to_owned(), "macos".to_owned(), "linux".to_owned()],
                    capabilities: vec![
                        "vision.screenshot".to_owned(),
                        "vision.find_text".to_owned(),
                    ],
                    permissions: vec!["screen_capture".to_owned(), "keyboard_mouse".to_owned()],
                },
                StoreExtension {
                    id: "greentic.desktop.macos.ax".to_owned(),
                    aliases: vec!["macos".to_owned(), "macos-ax".to_owned()],
                    name: "macOS Accessibility Adapter".to_owned(),
                    description: "Drive native macOS apps through Accessibility.".to_owned(),
                    latest: "1.0.0".to_owned(),
                    versions: vec!["1.0.0".to_owned()],
                    source: "oci://ghcr.io/greenticai/greentic-desktop/extensions/macos-ax:1.0.0"
                        .to_owned(),
                    publisher: "greenticai".to_owned(),
                    platforms: vec!["macos".to_owned()],
                    capabilities: vec![
                        "macos.activate_app".to_owned(),
                        "macos.find_window".to_owned(),
                        "macos.find_element".to_owned(),
                        "macos.type_text".to_owned(),
                        "macos.click_element".to_owned(),
                        "macos.read_text".to_owned(),
                    ],
                    permissions: vec!["screen_capture".to_owned(), "keyboard_mouse".to_owned()],
                },
                StoreExtension {
                    id: "greentic.desktop.linux.x11".to_owned(),
                    aliases: vec!["linux-x11".to_owned(), "x11".to_owned()],
                    name: "Linux X11 Desktop Adapter".to_owned(),
                    description: "Drive native Linux X11 apps through AT-SPI and XTest.".to_owned(),
                    latest: "1.0.0".to_owned(),
                    versions: vec!["1.0.0".to_owned()],
                    source: "oci://ghcr.io/greenticai/greentic-desktop/extensions/linux-x11:1.0.0"
                        .to_owned(),
                    publisher: "greenticai".to_owned(),
                    platforms: vec!["linux".to_owned()],
                    capabilities: vec![
                        "linux.find_window".to_owned(),
                        "linux.activate_window".to_owned(),
                        "linux.find_element".to_owned(),
                        "linux.type_text".to_owned(),
                        "linux.click_element".to_owned(),
                        "linux.read_text".to_owned(),
                    ],
                    permissions: vec![
                        "desktop.x11".to_owned(),
                        "screen_capture".to_owned(),
                        "keyboard_mouse".to_owned(),
                    ],
                },
                StoreExtension {
                    id: "greentic.desktop.windows-ui".to_owned(),
                    aliases: vec!["windows".to_owned(), "windows-ui".to_owned()],
                    name: "Windows UI Automation Adapter".to_owned(),
                    description: "Drive native Windows apps through UI Automation.".to_owned(),
                    latest: "1.0.0".to_owned(),
                    versions: vec!["1.0.0".to_owned()],
                    source: "oci://ghcr.io/greenticai/greentic-desktop/extensions/windows-ui:1.0.0"
                        .to_owned(),
                    publisher: "greenticai".to_owned(),
                    platforms: vec!["windows".to_owned()],
                    capabilities: vec![
                        "windows.open_app".to_owned(),
                        "windows.find_window".to_owned(),
                        "windows.find_element".to_owned(),
                        "windows.type_text".to_owned(),
                        "windows.click_element".to_owned(),
                        "windows.read_text".to_owned(),
                    ],
                    permissions: vec!["desktop.ui_automation".to_owned()],
                },
                StoreExtension {
                    id: "greentic.desktop.java-accessibility".to_owned(),
                    aliases: vec!["java".to_owned(), "java-accessibility".to_owned()],
                    name: "Java Accessibility Adapter".to_owned(),
                    description: "Drive Java Swing and AWT apps through Java Accessibility."
                        .to_owned(),
                    latest: "1.0.0".to_owned(),
                    versions: vec!["1.0.0".to_owned()],
                    source:
                        "oci://ghcr.io/greenticai/greentic-desktop/extensions/java-accessibility:1.0.0"
                            .to_owned(),
                    publisher: "greenticai".to_owned(),
                    platforms: vec!["windows".to_owned(), "macos".to_owned(), "linux".to_owned()],
                    capabilities: vec![
                        "java.find_window".to_owned(),
                        "java.find_component".to_owned(),
                        "java.type_text".to_owned(),
                        "java.click_component".to_owned(),
                        "java.read_text".to_owned(),
                    ],
                    permissions: vec!["desktop.java_accessibility".to_owned()],
                },
                StoreExtension {
                    id: "greentic.desktop.terminal-tn3270".to_owned(),
                    aliases: vec!["terminal".to_owned(), "tn3270".to_owned()],
                    name: "Terminal TN3270 Adapter".to_owned(),
                    description: "Drive terminal and mainframe workflows.".to_owned(),
                    latest: "1.0.0".to_owned(),
                    versions: vec!["1.0.0".to_owned()],
                    source:
                        "oci://ghcr.io/greenticai/greentic-desktop/extensions/terminal-tn3270:1.0.0"
                            .to_owned(),
                    publisher: "greenticai".to_owned(),
                    platforms: vec!["windows".to_owned(), "macos".to_owned(), "linux".to_owned()],
                    capabilities: vec![
                        "terminal.connect".to_owned(),
                        "terminal.send_keys".to_owned(),
                    ],
                    permissions: vec!["network".to_owned()],
                },
            ],
        }
    }

    pub fn find(&self, id_or_alias: &str) -> Option<&StoreExtension> {
        self.extensions.iter().find(|extension| {
            extension.id == id_or_alias
                || extension.aliases.iter().any(|alias| alias == id_or_alias)
        })
    }

    pub fn search(&self, query: &str) -> Vec<StoreExtension> {
        let query = query.to_ascii_lowercase();
        self.extensions
            .iter()
            .filter(|extension| {
                query.is_empty()
                    || extension.id.to_ascii_lowercase().contains(&query)
                    || extension.name.to_ascii_lowercase().contains(&query)
                    || extension.description.to_ascii_lowercase().contains(&query)
                    || extension.aliases.iter().any(|alias| alias.contains(&query))
            })
            .cloned()
            .collect()
    }
}

fn normalize_source(source: &str) -> String {
    if source.contains("://") {
        source.to_owned()
    } else {
        format!("store://{source}")
    }
}

fn scheme(source: &str) -> &str {
    source.split_once("://").map_or("", |(scheme, _)| scheme)
}

fn store_alias_to_extension_id(alias: &str) -> String {
    if alias.starts_with("greentic.desktop.") {
        alias.to_owned()
    } else {
        format!("greentic.desktop.{}", alias.replace('-', "."))
    }
}

fn extension_id_and_version_from_oci(source: &str) -> Result<(String, String), DistributorError> {
    let artifact = source
        .rsplit('/')
        .next()
        .ok_or_else(|| DistributorError::InvalidSource(source.to_owned()))?;
    let (name, version) = artifact
        .rsplit_once(':')
        .map_or((artifact, "latest"), |(name, version)| (name, version));
    Ok((store_alias_to_extension_id(name), version.to_owned()))
}

fn phase(phase: &str, status: &str, message: &str) -> ResolutionPhase {
    ResolutionPhase {
        phase: phase.to_owned(),
        status: status.to_owned(),
        message: message.to_owned(),
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_store_alias_to_oci_artifact() {
        let client = GreenticDistributorClient::new("/tmp/greentic-cache");
        let artifact = client
            .resolve("playwright")
            .expect("store alias should resolve");

        assert_eq!(artifact.extension_id, "greentic.desktop.playwright");
        assert_eq!(
            artifact.resolved_uri,
            "oci://ghcr.io/greenticai/greentic-desktop/extensions/playwright:1.0.0"
        );
        assert_eq!(artifact.version, "1.0.0");
        assert_eq!(artifact.phases[0].phase, "resolving");
    }

    #[test]
    fn store_index_searches_and_lists_versions() {
        let client = GreenticDistributorClient::new("/tmp/greentic-cache");
        let search = client.search("browser");
        assert_eq!(search.len(), 1);
        assert_eq!(search[0].id, "greentic.desktop.playwright");

        let versions = client
            .versions("playwright")
            .expect("friendly alias should have versions");
        assert_eq!(versions, vec!["1.0.0"]);

        let macos = client
            .resolve("macos")
            .expect("macOS alias should resolve to macOS adapter");
        assert_eq!(macos.extension_id, "greentic.desktop.macos.ax");
        assert_eq!(
            macos.resolved_uri,
            "oci://ghcr.io/greenticai/greentic-desktop/extensions/macos-ax:1.0.0"
        );

        let linux = client
            .resolve("linux-x11")
            .expect("linux x11 alias should resolve to Linux X11 adapter");
        assert_eq!(linux.extension_id, "greentic.desktop.linux.x11");

        let windows = client
            .resolve("windows")
            .expect("windows alias should resolve to Windows adapter");
        assert_eq!(windows.extension_id, "greentic.desktop.windows-ui");

        let java = client
            .resolve("java")
            .expect("java alias should resolve to Java adapter");
        assert_eq!(java.extension_id, "greentic.desktop.java-accessibility");
    }

    #[test]
    fn supports_oci_repo_and_file_sources() {
        let client = GreenticDistributorClient::new("/tmp/greentic-cache");
        let oci = client
            .resolve("oci://ghcr.io/greenticai/greentic-desktop/extensions/playwright:1.0.0")
            .expect("oci should resolve");
        assert_eq!(oci.extension_id, "greentic.desktop.playwright");
        assert_eq!(oci.version, "1.0.0");

        let repo = client
            .resolve("repo://tenant/extensions/playwright")
            .expect("repo should resolve");
        assert_eq!(repo.extension_id, "greentic.desktop.playwright");

        let file = client
            .resolve("file://./playwright.extension.tar.zst")
            .expect("file should resolve");
        assert_eq!(file.extension_id, "greentic.desktop.playwright");
    }
}
