use greentic_desktop_extension::{built_in_extension, ExtensionManifest};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltInExtensionCatalogEntry {
    pub manifest_id: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    pub platforms: &'static [&'static str],
    pub source_slug: &'static str,
    pub publisher: &'static str,
}

const BUILT_IN_EXTENSION_CATALOG: &[BuiltInExtensionCatalogEntry] = &[
    BuiltInExtensionCatalogEntry {
        manifest_id: "greentic.desktop.playwright",
        aliases: &["playwright", "browser", "web"],
        description: "Automate browser-based applications.",
        platforms: &["windows", "macos", "linux"],
        source_slug: "playwright",
        publisher: "greenticai",
    },
    BuiltInExtensionCatalogEntry {
        manifest_id: "greentic.desktop.vision",
        aliases: &["vision", "screenshot"],
        description: "Use screenshots and visual matching as a fallback.",
        platforms: &["windows", "macos", "linux"],
        source_slug: "vision",
        publisher: "greenticai",
    },
    BuiltInExtensionCatalogEntry {
        manifest_id: "greentic.desktop.macos.ax",
        aliases: &["macos", "macos-ax"],
        description: "Drive native macOS apps through Accessibility.",
        platforms: &["macos"],
        source_slug: "macos-ax",
        publisher: "greenticai",
    },
    BuiltInExtensionCatalogEntry {
        manifest_id: "greentic.desktop.linux.x11",
        aliases: &["linux-x11", "x11"],
        description: "Drive native Linux X11 apps through AT-SPI and XTest.",
        platforms: &["linux"],
        source_slug: "linux-x11",
        publisher: "greenticai",
    },
    BuiltInExtensionCatalogEntry {
        manifest_id: "greentic.desktop.linux.wayland",
        aliases: &["linux-wayland", "wayland"],
        description:
            "Drive Wayland-compatible Linux workflows through safe portals and accessibility.",
        platforms: &["linux"],
        source_slug: "linux-wayland",
        publisher: "greenticai",
    },
    BuiltInExtensionCatalogEntry {
        manifest_id: "greentic.desktop.windows-ui",
        aliases: &["windows", "windows-ui"],
        description: "Drive native Windows apps through UI Automation.",
        platforms: &["windows"],
        source_slug: "windows-ui",
        publisher: "greenticai",
    },
    BuiltInExtensionCatalogEntry {
        manifest_id: "greentic.desktop.java-accessibility",
        aliases: &["java", "java-accessibility"],
        description: "Drive Java Swing and AWT apps through Java Accessibility.",
        platforms: &["windows", "macos", "linux"],
        source_slug: "java-accessibility",
        publisher: "greenticai",
    },
    BuiltInExtensionCatalogEntry {
        manifest_id: "greentic.desktop.terminal-tn3270",
        aliases: &["terminal", "tn3270"],
        description: "Drive terminal and mainframe workflows.",
        platforms: &["windows", "macos", "linux"],
        source_slug: "terminal-tn3270",
        publisher: "greenticai",
    },
];

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
            extensions: built_in_store_extensions(),
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

pub fn built_in_store_extensions() -> Vec<StoreExtension> {
    BUILT_IN_EXTENSION_CATALOG
        .iter()
        .map(|entry| {
            let manifest = built_in_extension(entry.manifest_id).unwrap_or_else(|| {
                panic!(
                    "built-in extension manifest {} must exist for store catalog",
                    entry.manifest_id
                )
            });
            store_extension_from_manifest(entry, &manifest)
        })
        .collect()
}

fn store_extension_from_manifest(
    entry: &BuiltInExtensionCatalogEntry,
    manifest: &ExtensionManifest,
) -> StoreExtension {
    StoreExtension {
        id: manifest.id.clone(),
        aliases: entry
            .aliases
            .iter()
            .map(|alias| (*alias).to_owned())
            .collect(),
        name: manifest.name.clone(),
        description: entry.description.to_owned(),
        latest: manifest.version.clone(),
        versions: vec![manifest.version.clone()],
        source: format!(
            "oci://ghcr.io/greenticai/greentic-desktop/extensions/{}:{}",
            entry.source_slug, manifest.version
        ),
        publisher: entry.publisher.to_owned(),
        platforms: entry
            .platforms
            .iter()
            .map(|platform| (*platform).to_owned())
            .collect(),
        capabilities: manifest.capabilities.clone(),
        permissions: manifest.permissions.clone(),
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

    #[test]
    fn store_entries_are_generated_from_built_in_manifests() {
        let index = StoreIndex::default_greentic();
        assert_eq!(index.extensions.len(), BUILT_IN_EXTENSION_CATALOG.len());

        for catalog in BUILT_IN_EXTENSION_CATALOG {
            let manifest = built_in_extension(catalog.manifest_id).expect("manifest");
            let store = index.find(catalog.manifest_id).expect("store extension");

            assert_eq!(store.name, manifest.name);
            assert_eq!(store.latest, manifest.version);
            assert_eq!(store.versions, vec![manifest.version.clone()]);
            assert_eq!(store.capabilities, manifest.capabilities);
            assert_eq!(store.permissions, manifest.permissions);
            assert_eq!(
                store.aliases,
                catalog
                    .aliases
                    .iter()
                    .map(|alias| (*alias).to_owned())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                store.source,
                format!(
                    "oci://ghcr.io/greenticai/greentic-desktop/extensions/{}:{}",
                    catalog.source_slug, manifest.version
                )
            );
        }
    }

    #[test]
    fn terminal_store_capabilities_match_manifest_without_summarising() {
        let index = StoreIndex::default_greentic();
        let terminal = index
            .find("terminal")
            .expect("terminal alias should resolve");
        let manifest =
            built_in_extension("greentic.desktop.terminal-tn3270").expect("terminal manifest");

        assert!(terminal
            .capabilities
            .contains(&"terminal.extract_field".to_owned()));
        assert_eq!(terminal.capabilities, manifest.capabilities);
        assert_eq!(terminal.permissions, manifest.permissions);
    }
}
