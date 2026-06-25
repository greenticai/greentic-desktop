use greentic_desktop_adapter::AdapterCapabilities;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub runtime: ExtensionRuntime,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub capabilities: Vec<String>,
    pub permissions: Vec<String>,
    pub signed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionRuntime {
    Native,
    Sidecar,
}

impl ExtensionRuntime {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Sidecar => "sidecar",
        }
    }
}

impl ExtensionManifest {
    pub fn adapter_capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities::new(&self.id, &self.version, self.capabilities.iter().cloned())
    }

    pub fn render_toml(&self) -> String {
        let runtime = match self.runtime {
            ExtensionRuntime::Native => "native",
            ExtensionRuntime::Sidecar => "sidecar",
        };

        format!(
            "id = \"{}\"\nname = \"{}\"\nversion = \"{}\"\nruntime = \"{}\"\ncommand = \"{}\"\nargs = [{}]\nsigned = {}\n\n[capabilities]\ntools = [{}]\n\n[permissions]\nallow = [{}]\n",
            self.id,
            self.name,
            self.version,
            runtime,
            self.command.clone().unwrap_or_default(),
            render_array(&self.args),
            self.signed,
            render_array(&self.capabilities),
            render_array(&self.permissions)
        )
    }
}

pub const EXTENSION_PACKAGE_LAYOUT: &[&str] = &[
    "extension.toml",
    "manifest.cbor",
    "permissions.cbor",
    "capabilities.cbor",
    "README.md",
    "SBOM.spdx.json",
    "signatures/",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionPackageMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub runtime: ExtensionRuntime,
    pub entrypoint: String,
    pub distribution_source: String,
    pub platforms: ExtensionPlatforms,
    pub capabilities: Vec<String>,
    pub permissions: ExtensionPermissions,
    pub sbom_path: String,
    pub signature_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionPlatforms {
    pub windows: bool,
    pub macos: bool,
    pub linux: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionPermissions {
    pub network: bool,
    pub filesystem: String,
    pub screen_capture: bool,
    pub keyboard_mouse: bool,
}

impl ExtensionPackageMetadata {
    pub fn oci_artifact_media_type(&self) -> &'static str {
        "application/vnd.greentic.desktop.extension.layer.v1+tar+zstd"
    }

    pub fn manifest_media_type(&self) -> &'static str {
        "application/vnd.greentic.desktop.extension.manifest.v1+json"
    }

    pub fn to_install_manifest(&self, signed: bool) -> ExtensionManifest {
        ExtensionManifest {
            id: self.id.clone(),
            name: self.name.clone(),
            version: self.version.clone(),
            runtime: self.runtime,
            command: (self.runtime == ExtensionRuntime::Sidecar).then(|| self.entrypoint.clone()),
            args: Vec::new(),
            capabilities: self.capabilities.clone(),
            permissions: self.permissions.as_allow_list(),
            signed,
        }
    }
}

impl ExtensionPermissions {
    pub fn as_allow_list(&self) -> Vec<String> {
        let mut permissions = Vec::new();
        if self.network {
            permissions.push("network".to_owned());
        }
        if self.filesystem != "none" {
            permissions.push(format!("filesystem.{}", self.filesystem));
        }
        if self.screen_capture {
            permissions.push("screen_capture".to_owned());
        }
        if self.keyboard_mouse {
            permissions.push("keyboard_mouse".to_owned());
        }
        permissions
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarProcess {
    pub extension_id: String,
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledExtensionRecord {
    pub id: String,
    pub version: String,
    pub source: String,
    pub digest: String,
    pub installed_at: String,
    pub enabled: bool,
    pub publisher: String,
    pub signature_status: String,
    pub sbom_present: bool,
    pub trust_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionHealth {
    pub id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionTrustPolicy {
    pub allow_unsigned: bool,
    pub allow_local_unsigned_drafts: bool,
    pub trusted_publishers: Vec<String>,
    pub require_approval_for_screen_capture: bool,
    pub require_approval_for_keyboard_mouse: bool,
    pub require_approval_for_filesystem_write: bool,
}

impl Default for ExtensionTrustPolicy {
    fn default() -> Self {
        Self {
            allow_unsigned: false,
            allow_local_unsigned_drafts: true,
            trusted_publishers: vec!["greenticai".to_owned()],
            require_approval_for_screen_capture: true,
            require_approval_for_keyboard_mouse: true,
            require_approval_for_filesystem_write: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionApproval {
    pub screen_capture: bool,
    pub keyboard_mouse: bool,
    pub filesystem_write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionVerificationResult {
    pub allowed: bool,
    pub reasons: Vec<String>,
    pub publisher: String,
    pub signature_status: String,
    pub sbom_present: bool,
}

pub fn verify_extension_package_trust(
    metadata: &ExtensionPackageMetadata,
    policy: &ExtensionTrustPolicy,
    signed: bool,
    sbom_present: bool,
    approval: &PermissionApproval,
) -> ExtensionVerificationResult {
    let mut reasons = Vec::new();
    let local_draft = metadata.distribution_source.starts_with("file://");
    if !(signed || policy.allow_unsigned || local_draft && policy.allow_local_unsigned_drafts) {
        reasons.push("extension package is unsigned".to_owned());
    }
    if !policy
        .trusted_publishers
        .iter()
        .any(|publisher| publisher == &metadata.publisher)
    {
        reasons.push(format!("publisher {} is not trusted", metadata.publisher));
    }
    if !sbom_present && !local_draft {
        reasons.push("production extension package must include an SBOM".to_owned());
    }
    if policy.require_approval_for_screen_capture
        && metadata.permissions.screen_capture
        && !approval.screen_capture
    {
        reasons.push("screen capture permission requires approval".to_owned());
    }
    if policy.require_approval_for_keyboard_mouse
        && metadata.permissions.keyboard_mouse
        && !approval.keyboard_mouse
    {
        reasons.push("keyboard and mouse control requires approval".to_owned());
    }
    if policy.require_approval_for_filesystem_write
        && metadata.permissions.filesystem == "write"
        && !approval.filesystem_write
    {
        reasons.push("filesystem write permission requires approval".to_owned());
    }

    ExtensionVerificationResult {
        allowed: reasons.is_empty(),
        reasons,
        publisher: metadata.publisher.clone(),
        signature_status: if signed { "valid" } else { "unsigned" }.to_owned(),
        sbom_present,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionError {
    Io(String),
    InvalidManifest(String),
    UnsignedExtension(String),
    NotFound(String),
    NotSidecar(String),
}

impl fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message)
            | Self::InvalidManifest(message)
            | Self::UnsignedExtension(message)
            | Self::NotFound(message)
            | Self::NotSidecar(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ExtensionError {}

impl From<std::io::Error> for ExtensionError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct ExtensionManager {
    home: PathBuf,
    require_signed_extensions: bool,
}

impl ExtensionManager {
    pub fn new(home: impl AsRef<Path>, require_signed_extensions: bool) -> Self {
        Self {
            home: home.as_ref().to_path_buf(),
            require_signed_extensions,
        }
    }

    pub fn install(&self, manifest: &ExtensionManifest) -> Result<PathBuf, ExtensionError> {
        self.install_resolved(
            manifest,
            &format!("store://{}", manifest.id),
            "sha256:local",
        )
    }

    pub fn install_resolved(
        &self,
        manifest: &ExtensionManifest,
        source: &str,
        digest: &str,
    ) -> Result<PathBuf, ExtensionError> {
        self.verify(manifest)?;
        let dir = self.extension_version_dir(&manifest.id, &manifest.version);
        fs::create_dir_all(&dir)?;
        let path = dir.join("extension.toml");
        fs::write(&path, manifest.render_toml())?;
        fs::write(
            self.extension_dir(&manifest.id).join("current"),
            &manifest.version,
        )?;
        let mut records = self.installed_records()?;
        upsert_record(
            &mut records,
            InstalledExtensionRecord {
                id: manifest.id.clone(),
                version: manifest.version.clone(),
                source: source.to_owned(),
                digest: digest.to_owned(),
                installed_at: "local".to_owned(),
                enabled: true,
                publisher: "greenticai".to_owned(),
                signature_status: if manifest.signed { "valid" } else { "unsigned" }.to_owned(),
                sbom_present: true,
                trust_reasons: Vec::new(),
            },
        );
        self.write_installed_records(&records)?;
        Ok(path)
    }

    pub fn list(&self) -> Result<Vec<ExtensionManifest>, ExtensionError> {
        let extensions_dir = self.home.join("extensions");
        if !extensions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut manifests = Vec::new();
        for entry in fs::read_dir(extensions_dir)? {
            let entry = entry?;
            let manifest_path = current_manifest_path(&entry.path());
            if manifest_path.exists() {
                manifests.push(parse_manifest(&fs::read_to_string(manifest_path)?)?);
            }
        }
        manifests.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(manifests)
    }

    pub fn verify(&self, manifest: &ExtensionManifest) -> Result<(), ExtensionError> {
        if manifest.id.trim().is_empty() {
            return Err(ExtensionError::InvalidManifest(
                "extension id must not be empty".to_owned(),
            ));
        }

        if manifest.capabilities.is_empty() {
            return Err(ExtensionError::InvalidManifest(format!(
                "extension {} must declare capabilities",
                manifest.id
            )));
        }

        if self.require_signed_extensions && !manifest.signed {
            return Err(ExtensionError::UnsignedExtension(format!(
                "unsigned extension {} refused by policy",
                manifest.id
            )));
        }

        if manifest.runtime == ExtensionRuntime::Sidecar
            && manifest.command.as_deref().unwrap_or("").trim().is_empty()
        {
            return Err(ExtensionError::InvalidManifest(format!(
                "sidecar extension {} must declare command",
                manifest.id
            )));
        }

        Ok(())
    }

    pub fn installed_records(&self) -> Result<Vec<InstalledExtensionRecord>, ExtensionError> {
        let path = self.installed_lock_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        Ok(parse_installed_lock(&fs::read_to_string(path)?))
    }

    pub fn remove(&self, extension_id: &str) -> Result<(), ExtensionError> {
        let dir = self.extension_dir(extension_id);
        if !dir.exists() {
            return Err(ExtensionError::NotFound(format!(
                "extension {extension_id} not found"
            )));
        }
        fs::remove_dir_all(dir)?;
        let mut records = self.installed_records()?;
        records.retain(|record| record.id != extension_id);
        self.write_installed_records(&records)
    }

    pub fn set_enabled(&self, extension_id: &str, enabled: bool) -> Result<(), ExtensionError> {
        let mut records = self.installed_records()?;
        let record = records
            .iter_mut()
            .find(|record| record.id == extension_id)
            .ok_or_else(|| {
                ExtensionError::NotFound(format!("extension {extension_id} not found"))
            })?;
        record.enabled = enabled;
        self.write_installed_records(&records)
    }

    pub fn health(&self, extension_id: &str) -> Result<ExtensionHealth, ExtensionError> {
        let manifest = self
            .list()?
            .into_iter()
            .find(|manifest| manifest.id == extension_id)
            .ok_or_else(|| {
                ExtensionError::NotFound(format!("extension {extension_id} not found"))
            })?;
        self.verify(&manifest)?;
        Ok(ExtensionHealth {
            id: extension_id.to_owned(),
            status: "healthy".to_owned(),
            message: "Manifest and local store entry are valid.".to_owned(),
        })
    }

    pub fn start_sidecar(&self, extension_id: &str) -> Result<SidecarProcess, ExtensionError> {
        let manifest = self
            .list()?
            .into_iter()
            .find(|manifest| manifest.id == extension_id)
            .ok_or_else(|| {
                ExtensionError::NotFound(format!("extension {extension_id} not found"))
            })?;
        self.verify(&manifest)?;

        if manifest.runtime != ExtensionRuntime::Sidecar {
            return Err(ExtensionError::NotSidecar(format!(
                "extension {extension_id} is not a sidecar"
            )));
        }

        Ok(SidecarProcess {
            extension_id: manifest.id,
            command: manifest.command.unwrap_or_default(),
            args: manifest.args,
        })
    }

    fn extension_dir(&self, extension_id: &str) -> PathBuf {
        self.home.join("extensions").join(extension_id)
    }

    fn extension_version_dir(&self, extension_id: &str, version: &str) -> PathBuf {
        self.extension_dir(extension_id).join(version)
    }

    fn installed_lock_path(&self) -> PathBuf {
        self.home.join("extensions").join("installed.lock")
    }

    fn write_installed_records(
        &self,
        records: &[InstalledExtensionRecord],
    ) -> Result<(), ExtensionError> {
        let path = self.installed_lock_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, render_installed_lock(records))?;
        Ok(())
    }
}

pub fn parse_manifest(input: &str) -> Result<ExtensionManifest, ExtensionError> {
    let id = string_value(input, "id")
        .ok_or_else(|| ExtensionError::InvalidManifest("missing extension id".to_owned()))?;
    let name = string_value(input, "name").unwrap_or_else(|| id.clone());
    let version = string_value(input, "version").unwrap_or_else(|| "0.0.0".to_owned());
    let runtime = match string_value(input, "runtime").as_deref() {
        Some("native") => ExtensionRuntime::Native,
        Some("sidecar") => ExtensionRuntime::Sidecar,
        Some(other) => {
            return Err(ExtensionError::InvalidManifest(format!(
                "unknown extension runtime {other}"
            )))
        }
        None => ExtensionRuntime::Native,
    };
    let command = string_value(input, "command").filter(|value| !value.is_empty());
    let args = array_value(input, "args");
    let capabilities = array_value(input, "tools");
    let permissions = array_value(input, "allow");
    let signed = bool_value(input, "signed").unwrap_or(false);

    Ok(ExtensionManifest {
        id,
        name,
        version,
        runtime,
        command,
        args,
        capabilities,
        permissions,
        signed,
    })
}

pub fn parse_package_metadata(input: &str) -> Result<ExtensionPackageMetadata, ExtensionError> {
    let id = string_value(input, "id")
        .ok_or_else(|| ExtensionError::InvalidManifest("missing extension id".to_owned()))?;
    let name = string_value(input, "name").unwrap_or_else(|| id.clone());
    let version = string_value(input, "version").unwrap_or_else(|| "0.0.0".to_owned());
    let publisher = string_value(input, "publisher")
        .ok_or_else(|| ExtensionError::InvalidManifest("missing extension publisher".to_owned()))?;
    let runtime = match string_value(input, "runtime").as_deref() {
        Some("native") => ExtensionRuntime::Native,
        Some("sidecar") => ExtensionRuntime::Sidecar,
        Some(other) => {
            return Err(ExtensionError::InvalidManifest(format!(
                "unknown extension runtime {other}"
            )))
        }
        None => ExtensionRuntime::Native,
    };
    let entrypoint = string_value(input, "entrypoint").unwrap_or_default();
    let distribution_source = string_value(input, "source").unwrap_or_default();
    let metadata = ExtensionPackageMetadata {
        id,
        name,
        version,
        publisher,
        runtime,
        entrypoint,
        distribution_source,
        platforms: ExtensionPlatforms {
            windows: bool_value(input, "windows").unwrap_or(false),
            macos: bool_value(input, "macos").unwrap_or(false),
            linux: bool_value(input, "linux").unwrap_or(false),
        },
        capabilities: array_value(input, "tools"),
        permissions: ExtensionPermissions {
            network: bool_value(input, "network").unwrap_or(false),
            filesystem: string_value(input, "filesystem").unwrap_or_else(|| "none".to_owned()),
            screen_capture: bool_value(input, "screen_capture").unwrap_or(false),
            keyboard_mouse: bool_value(input, "keyboard_mouse").unwrap_or(false),
        },
        sbom_path: "SBOM.spdx.json".to_owned(),
        signature_dir: "signatures/".to_owned(),
    };
    validate_package_metadata(&metadata)?;
    Ok(metadata)
}

pub fn validate_package_metadata(
    metadata: &ExtensionPackageMetadata,
) -> Result<(), ExtensionError> {
    if metadata.id.trim().is_empty() {
        return Err(ExtensionError::InvalidManifest(
            "extension id must not be empty".to_owned(),
        ));
    }
    if metadata.version.trim().is_empty() {
        return Err(ExtensionError::InvalidManifest(format!(
            "extension {} must declare version",
            metadata.id
        )));
    }
    if metadata.publisher.trim().is_empty() {
        return Err(ExtensionError::InvalidManifest(format!(
            "extension {} must declare publisher",
            metadata.id
        )));
    }
    if metadata.capabilities.is_empty() {
        return Err(ExtensionError::InvalidManifest(format!(
            "extension {} must declare capabilities",
            metadata.id
        )));
    }
    if metadata.runtime == ExtensionRuntime::Sidecar && metadata.entrypoint.trim().is_empty() {
        return Err(ExtensionError::InvalidManifest(format!(
            "sidecar extension {} must declare entrypoint",
            metadata.id
        )));
    }
    if !metadata.platforms.windows && !metadata.platforms.macos && !metadata.platforms.linux {
        return Err(ExtensionError::InvalidManifest(format!(
            "extension {} must support at least one platform",
            metadata.id
        )));
    }
    if !metadata.distribution_source.is_empty()
        && !metadata.distribution_source.starts_with("oci://")
        && !metadata.distribution_source.starts_with("file://")
        && !metadata.distribution_source.starts_with("store://")
    {
        return Err(ExtensionError::InvalidManifest(format!(
            "extension {} has unsupported distribution source",
            metadata.id
        )));
    }
    Ok(())
}

pub fn built_in_extension(extension_id: &str) -> Option<ExtensionManifest> {
    match extension_id {
        "greentic.desktop.playwright" => Some(ExtensionManifest {
            id: extension_id.to_owned(),
            name: "Playwright Web Adapter".to_owned(),
            version: "1.0.0".to_owned(),
            runtime: ExtensionRuntime::Sidecar,
            command: Some("node".to_owned()),
            args: vec!["./index.js".to_owned()],
            capabilities: vec![
                "web.goto".to_owned(),
                "web.click".to_owned(),
                "web.fill".to_owned(),
                "web.select".to_owned(),
                "web.wait_for_text".to_owned(),
                "web.assert_visible".to_owned(),
                "web.assert_url".to_owned(),
                "web.extract_text".to_owned(),
                "web.extract_regex".to_owned(),
                "web.screenshot".to_owned(),
                "web.download_file".to_owned(),
            ],
            permissions: vec!["network.localhost".to_owned()],
            signed: true,
        }),
        "greentic.desktop.terminal-tn3270" => Some(ExtensionManifest {
            id: extension_id.to_owned(),
            name: "Terminal TN3270 Adapter".to_owned(),
            version: "1.0.0".to_owned(),
            runtime: ExtensionRuntime::Sidecar,
            command: Some("greentic-tn3270-adapter".to_owned()),
            args: Vec::new(),
            capabilities: vec![
                "terminal.connect".to_owned(),
                "terminal.disconnect".to_owned(),
                "terminal.read_screen".to_owned(),
                "terminal.send_keys".to_owned(),
                "terminal.send_text".to_owned(),
                "terminal.type_text".to_owned(),
                "terminal.wait_for_screen".to_owned(),
                "terminal.assert_text".to_owned(),
                "terminal.extract_field".to_owned(),
                "terminal.capture_screen".to_owned(),
            ],
            permissions: vec!["network.tenant".to_owned()],
            signed: true,
        }),
        "greentic.desktop.windows-ui" => Some(ExtensionManifest {
            id: extension_id.to_owned(),
            name: "Windows UI Automation Adapter".to_owned(),
            version: "1.0.0".to_owned(),
            runtime: ExtensionRuntime::Native,
            command: None,
            args: Vec::new(),
            capabilities: vec![
                "windows.open_app".to_owned(),
                "windows.find_window".to_owned(),
                "windows.find_element".to_owned(),
                "windows.click_element".to_owned(),
                "windows.type_text".to_owned(),
                "windows.read_text".to_owned(),
                "windows.read_window_tree".to_owned(),
                "windows.assert_visible".to_owned(),
                "windows.screenshot".to_owned(),
                "windows.close_app".to_owned(),
            ],
            permissions: vec!["desktop.ui_automation".to_owned()],
            signed: true,
        }),
        "greentic.desktop.java-accessibility" => Some(ExtensionManifest {
            id: extension_id.to_owned(),
            name: "Java Accessibility Adapter".to_owned(),
            version: "1.0.0".to_owned(),
            runtime: ExtensionRuntime::Sidecar,
            command: Some("greentic-java-accessibility-adapter".to_owned()),
            args: Vec::new(),
            capabilities: vec![
                "java.find_window".to_owned(),
                "java.find_component".to_owned(),
                "java.click_component".to_owned(),
                "java.type_text".to_owned(),
                "java.read_text".to_owned(),
                "java.assert_visible".to_owned(),
                "java.capture_tree".to_owned(),
            ],
            permissions: vec!["desktop.java_accessibility".to_owned()],
            signed: true,
        }),
        "greentic.desktop.vision" => Some(ExtensionManifest {
            id: extension_id.to_owned(),
            name: "Vision Screenshot Fallback Adapter".to_owned(),
            version: "1.0.0".to_owned(),
            runtime: ExtensionRuntime::Sidecar,
            command: Some("greentic-vision-adapter".to_owned()),
            args: Vec::new(),
            capabilities: vec![
                "vision.screenshot".to_owned(),
                "vision.find_text".to_owned(),
                "vision.find_button".to_owned(),
                "vision.click_region".to_owned(),
                "vision.compare_baseline".to_owned(),
                "vision.assert_visual".to_owned(),
                "vision.extract_text".to_owned(),
            ],
            permissions: vec!["desktop.screenshot".to_owned()],
            signed: true,
        }),
        "greentic.desktop.macos.ax" => Some(ExtensionManifest {
            id: extension_id.to_owned(),
            name: "macOS Accessibility Adapter".to_owned(),
            version: "1.0.0".to_owned(),
            runtime: ExtensionRuntime::Native,
            command: None,
            args: Vec::new(),
            capabilities: vec![
                "macos.find_app".to_owned(),
                "macos.find_window".to_owned(),
                "macos.read_window_tree".to_owned(),
                "macos.find_element".to_owned(),
                "macos.click_element".to_owned(),
                "macos.type_text".to_owned(),
                "macos.read_text".to_owned(),
                "macos.assert_visible".to_owned(),
                "macos.screenshot".to_owned(),
                "macos.activate_app".to_owned(),
                "macos.close_app".to_owned(),
            ],
            permissions: vec![
                "desktop.accessibility".to_owned(),
                "desktop.screen_recording".to_owned(),
                "desktop.input_monitoring".to_owned(),
            ],
            signed: true,
        }),
        "greentic.desktop.linux.x11" => Some(ExtensionManifest {
            id: extension_id.to_owned(),
            name: "Linux X11 Desktop Adapter".to_owned(),
            version: "1.0.0".to_owned(),
            runtime: ExtensionRuntime::Native,
            command: None,
            args: Vec::new(),
            capabilities: vec![
                "linux.find_window".to_owned(),
                "linux.read_window_tree".to_owned(),
                "linux.find_element".to_owned(),
                "linux.click_element".to_owned(),
                "linux.type_text".to_owned(),
                "linux.read_text".to_owned(),
                "linux.assert_visible".to_owned(),
                "linux.screenshot".to_owned(),
                "linux.activate_window".to_owned(),
                "linux.close_window".to_owned(),
            ],
            permissions: vec![
                "desktop.x11".to_owned(),
                "desktop.window_management".to_owned(),
                "desktop.screenshot".to_owned(),
                "desktop.input".to_owned(),
            ],
            signed: true,
        }),
        "greentic.desktop.linux.wayland" => Some(ExtensionManifest {
            id: extension_id.to_owned(),
            name: "Linux Wayland Compatibility Adapter".to_owned(),
            version: "1.0.0".to_owned(),
            runtime: ExtensionRuntime::Native,
            command: None,
            args: Vec::new(),
            capabilities: vec![
                "linux.wayland.detect".to_owned(),
                "linux.wayland.portal_screenshot".to_owned(),
                "linux.wayland.accessibility_tree".to_owned(),
                "linux.wayland.assert_visible".to_owned(),
                "linux.wayland.safe_keyboard_shortcut".to_owned(),
            ],
            permissions: vec![
                "desktop.wayland".to_owned(),
                "desktop.portal_screenshot".to_owned(),
                "desktop.accessibility".to_owned(),
            ],
            signed: true,
        }),
        _ => None,
    }
}

fn string_value(input: &str, key: &str) -> Option<String> {
    input.lines().find_map(|line| {
        let line = line.trim();
        let rest = line.strip_prefix(key)?.trim_start();
        let rest = rest.strip_prefix('=')?.trim_start();
        Some(rest.trim_matches('"').to_owned())
    })
}

fn bool_value(input: &str, key: &str) -> Option<bool> {
    string_value(input, key).and_then(|value| match value.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}

fn array_value(input: &str, key: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut collecting = false;

    for line in input.lines() {
        let line = line.trim();
        if line.starts_with(&format!("{key} = [")) {
            collecting = true;
            collect_quoted_values(line, &mut values);
            if line.ends_with(']') {
                break;
            }
            continue;
        }

        if collecting {
            collect_quoted_values(line, &mut values);
            if line.ends_with(']') {
                break;
            }
        }
    }

    values.sort();
    values.dedup();
    values
}

fn collect_quoted_values(line: &str, values: &mut Vec<String>) {
    let mut rest = line;
    while let Some(start) = rest.find('"') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('"') else {
            break;
        };
        values.push(rest[..end].to_owned());
        rest = &rest[end + 1..];
    }
}

fn current_manifest_path(extension_dir: &Path) -> PathBuf {
    let legacy = extension_dir.join("extension.toml");
    if legacy.exists() {
        return legacy;
    }
    let current = extension_dir.join("current");
    let version = fs::read_to_string(current).unwrap_or_default();
    extension_dir.join(version.trim()).join("extension.toml")
}

fn upsert_record(records: &mut Vec<InstalledExtensionRecord>, record: InstalledExtensionRecord) {
    if let Some(existing) = records.iter_mut().find(|existing| existing.id == record.id) {
        *existing = record;
    } else {
        records.push(record);
    }
    records.sort_by(|left, right| left.id.cmp(&right.id));
}

fn render_installed_lock(records: &[InstalledExtensionRecord]) -> String {
    records
        .iter()
        .map(|record| {
            format!(
                "[[extensions]]\nid = \"{}\"\nversion = \"{}\"\nsource = \"{}\"\ndigest = \"{}\"\ninstalled_at = \"{}\"\nenabled = {}\npublisher = \"{}\"\nsignature_status = \"{}\"\nsbom_present = {}\ntrust_reasons = [{}]\n",
                record.id,
                record.version,
                record.source,
                record.digest,
                record.installed_at,
                record.enabled,
                record.publisher,
                record.signature_status,
                record.sbom_present,
                render_array(&record.trust_reasons)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_installed_lock(input: &str) -> Vec<InstalledExtensionRecord> {
    input
        .split("[[extensions]]")
        .filter_map(|chunk| {
            let id = string_value(chunk, "id")?;
            Some(InstalledExtensionRecord {
                id,
                version: string_value(chunk, "version").unwrap_or_else(|| "0.0.0".to_owned()),
                source: string_value(chunk, "source").unwrap_or_default(),
                digest: string_value(chunk, "digest").unwrap_or_default(),
                installed_at: string_value(chunk, "installed_at").unwrap_or_default(),
                enabled: bool_value(chunk, "enabled").unwrap_or(true),
                publisher: string_value(chunk, "publisher")
                    .unwrap_or_else(|| "greenticai".to_owned()),
                signature_status: string_value(chunk, "signature_status")
                    .unwrap_or_else(|| "valid".to_owned()),
                sbom_present: bool_value(chunk, "sbom_present").unwrap_or(true),
                trust_reasons: array_value(chunk, "trust_reasons"),
            })
        })
        .collect()
}

fn render_array(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("\"{value}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_home() -> PathBuf {
        std::env::temp_dir().join(format!(
            "greentic-extension-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ))
    }

    #[test]
    fn parses_pr_03_manifest_shape() {
        let manifest = parse_manifest(
            r#"
id = "greentic.desktop.playwright"
name = "Playwright Web Adapter"
version = "1.0.0"
runtime = "sidecar"
command = "node"
args = ["./index.js"]
signed = true

[capabilities]
tools = [
  "web.goto",
  "web.click",
  "web.fill"
]
"#,
        )
        .expect("manifest should parse");

        assert_eq!(manifest.id, "greentic.desktop.playwright");
        assert_eq!(manifest.runtime, ExtensionRuntime::Sidecar);
        assert!(manifest.capabilities.contains(&"web.click".to_owned()));
    }

    #[test]
    fn parses_extension_package_metadata_for_oci_artifacts() {
        let metadata = parse_package_metadata(
            r#"
id = "greentic.desktop.playwright"
name = "Playwright Web Adapter"
version = "1.0.0"
publisher = "greenticai"
runtime = "sidecar"
entrypoint = "sidecar/index.js"

[distribution]
source = "oci://ghcr.io/greenticai/greentic-desktop/extensions/playwright:1.0.0"

[platforms]
windows = true
macos = true
linux = true

[capabilities]
tools = [
  "web.goto",
  "web.click",
  "web.fill",
  "web.extract_text",
  "web.assert_visible",
  "evidence.screenshot"
]

[permissions]
network = true
filesystem = "limited"
screen_capture = false
keyboard_mouse = false
"#,
        )
        .expect("package metadata should parse");

        assert_eq!(metadata.id, "greentic.desktop.playwright");
        assert_eq!(metadata.publisher, "greenticai");
        assert_eq!(metadata.runtime, ExtensionRuntime::Sidecar);
        assert_eq!(metadata.entrypoint, "sidecar/index.js");
        assert!(metadata.platforms.windows);
        assert!(metadata.capabilities.contains(&"web.click".to_owned()));
        assert_eq!(
            metadata.oci_artifact_media_type(),
            "application/vnd.greentic.desktop.extension.layer.v1+tar+zstd"
        );

        let install_manifest = metadata.to_install_manifest(true);
        assert_eq!(
            install_manifest.command.as_deref(),
            Some("sidecar/index.js")
        );
        assert!(install_manifest
            .permissions
            .contains(&"filesystem.limited".to_owned()));
    }

    #[test]
    fn rejects_invalid_extension_package_metadata() {
        let missing_publisher = parse_package_metadata(
            r#"
id = "greentic.desktop.bad"
version = "1.0.0"
runtime = "sidecar"
entrypoint = "sidecar/index.js"

[platforms]
linux = true

[capabilities]
tools = ["bad.run"]
"#,
        )
        .expect_err("publisher is required");
        assert!(missing_publisher.to_string().contains("publisher"));

        let missing_entrypoint = parse_package_metadata(
            r#"
id = "greentic.desktop.bad"
version = "1.0.0"
publisher = "greenticai"
runtime = "sidecar"

[platforms]
linux = true

[capabilities]
tools = ["bad.run"]
"#,
        )
        .expect_err("sidecar entrypoint is required");
        assert!(missing_entrypoint.to_string().contains("entrypoint"));
    }

    #[test]
    fn trust_policy_blocks_unsigned_untrusted_and_unapproved_permissions() {
        let mut metadata = parse_package_metadata(
            r#"
id = "greentic.desktop.vision"
name = "Vision"
version = "1.0.0"
publisher = "unknown"
runtime = "sidecar"
entrypoint = "sidecar/index.js"

[distribution]
source = "oci://ghcr.io/greenticai/greentic-desktop/extensions/vision:1.0.0"

[platforms]
linux = true

[capabilities]
tools = ["vision.screenshot"]

[permissions]
network = false
filesystem = "write"
screen_capture = true
keyboard_mouse = true
"#,
        )
        .expect("metadata should parse");

        let policy = ExtensionTrustPolicy::default();
        let result = verify_extension_package_trust(
            &metadata,
            &policy,
            false,
            false,
            &PermissionApproval {
                screen_capture: false,
                keyboard_mouse: false,
                filesystem_write: false,
            },
        );

        assert!(!result.allowed);
        assert!(result
            .reasons
            .iter()
            .any(|reason| reason.contains("unsigned")));
        assert!(result
            .reasons
            .iter()
            .any(|reason| reason.contains("not trusted")));
        assert!(result
            .reasons
            .iter()
            .any(|reason| reason.contains("screen capture")));

        metadata.publisher = "greenticai".to_owned();
        let approved = verify_extension_package_trust(
            &metadata,
            &policy,
            true,
            true,
            &PermissionApproval {
                screen_capture: true,
                keyboard_mouse: true,
                filesystem_write: true,
            },
        );
        assert!(approved.allowed);
    }

    #[test]
    fn trust_policy_allows_local_unsigned_drafts_in_dev_mode() {
        let metadata = parse_package_metadata(
            r#"
id = "greentic.desktop.local"
name = "Local"
version = "0.1.0"
publisher = "greenticai"
runtime = "sidecar"
entrypoint = "sidecar/index.js"

[distribution]
source = "file://./local.extension.tar.zst"

[platforms]
linux = true

[capabilities]
tools = ["local.run"]

[permissions]
filesystem = "none"
"#,
        )
        .expect("metadata should parse");
        let result = verify_extension_package_trust(
            &metadata,
            &ExtensionTrustPolicy::default(),
            false,
            false,
            &PermissionApproval {
                screen_capture: false,
                keyboard_mouse: false,
                filesystem_write: false,
            },
        );

        assert!(result.allowed);
        assert_eq!(result.signature_status, "unsigned");
    }

    #[test]
    fn installs_signed_extension_and_lists_capabilities() {
        let home = temp_home();
        let manager = ExtensionManager::new(&home, true);
        let manifest = built_in_extension("greentic.desktop.playwright")
            .expect("built-in extension should exist");

        manager
            .install(&manifest)
            .expect("signed install should pass");
        let installed = manager.list().expect("extensions should list");

        assert_eq!(installed.len(), 1);
        assert!(installed[0]
            .adapter_capabilities()
            .supports("web.extract_text"));

        fs::remove_dir_all(home).expect("test dir should be removable");
    }

    #[test]
    fn local_store_tracks_install_enable_disable_health_and_remove() {
        let home = temp_home();
        let manager = ExtensionManager::new(&home, true);
        let manifest = built_in_extension("greentic.desktop.playwright")
            .expect("built-in extension should exist");

        let path = manager
            .install_resolved(
                &manifest,
                "oci://ghcr.io/greenticai/greentic-desktop/extensions/playwright:1.0.0",
                "sha256:test",
            )
            .expect("install should pass");
        assert!(path.ends_with("1.0.0/extension.toml"));
        assert!(home
            .join("extensions")
            .join("greentic.desktop.playwright")
            .join("current")
            .exists());

        let records = manager
            .installed_records()
            .expect("installed records should parse");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].digest, "sha256:test");
        assert_eq!(records[0].publisher, "greenticai");
        assert_eq!(records[0].signature_status, "valid");
        assert!(records[0].sbom_present);
        assert!(records[0].enabled);

        manager
            .set_enabled("greentic.desktop.playwright", false)
            .expect("disable should pass");
        assert!(
            !manager
                .installed_records()
                .expect("records")
                .first()
                .expect("record")
                .enabled
        );

        let health = manager
            .health("greentic.desktop.playwright")
            .expect("health should pass");
        assert_eq!(health.status, "healthy");

        manager
            .remove("greentic.desktop.playwright")
            .expect("remove should pass");
        assert!(manager.list().expect("list should pass").is_empty());
        assert!(manager
            .installed_records()
            .expect("records should parse")
            .is_empty());

        fs::remove_dir_all(home).expect("test dir should be removable");
    }

    #[test]
    fn refuses_unsigned_extension_when_required() {
        let home = temp_home();
        let manager = ExtensionManager::new(&home, true);
        let mut manifest =
            built_in_extension("greentic.desktop.playwright").expect("built-in extension");
        manifest.signed = false;

        let err = manager
            .install(&manifest)
            .expect_err("unsigned extension should fail");

        assert!(err.to_string().contains("unsigned extension"));
    }

    #[test]
    fn prepares_sidecar_process_metadata() {
        let home = temp_home();
        let manager = ExtensionManager::new(&home, true);
        let manifest = built_in_extension("greentic.desktop.playwright")
            .expect("built-in extension should exist");
        manager.install(&manifest).expect("install should pass");

        let sidecar = manager
            .start_sidecar("greentic.desktop.playwright")
            .expect("sidecar should prepare");

        assert_eq!(sidecar.command, "node");
        assert_eq!(sidecar.args, vec!["./index.js"]);

        fs::remove_dir_all(home).expect("test dir should be removable");
    }

    #[test]
    fn parses_defaults_and_multiline_arrays() {
        let manifest = parse_manifest(
            r#"
id = "greentic.desktop.local"

[capabilities]
tools = [
  "local.click",
  "local.click",
  "local.type"
]

[permissions]
allow = [
  "desktop.input"
]
"#,
        )
        .expect("minimal manifest should parse");

        assert_eq!(manifest.name, "greentic.desktop.local");
        assert_eq!(manifest.version, "0.0.0");
        assert_eq!(manifest.runtime, ExtensionRuntime::Native);
        assert_eq!(
            manifest.capabilities,
            vec!["local.click".to_owned(), "local.type".to_owned()]
        );
        assert_eq!(manifest.permissions, vec!["desktop.input".to_owned()]);
        assert!(!manifest.signed);
    }

    #[test]
    fn rejects_invalid_manifest_shapes() {
        let missing_id =
            parse_manifest("runtime = \"native\"").expect_err("manifest without id should fail");
        assert!(missing_id.to_string().contains("missing extension id"));

        let unknown_runtime = parse_manifest(
            r#"
id = "bad"
runtime = "container"
"#,
        )
        .expect_err("unknown runtime should fail");
        assert!(unknown_runtime
            .to_string()
            .contains("unknown extension runtime"));
    }

    #[test]
    fn verify_rejects_empty_capabilities_and_missing_sidecar_command() {
        let manager = ExtensionManager::new(temp_home(), false);
        let empty_capabilities = ExtensionManifest {
            id: "greentic.desktop.empty".to_owned(),
            name: "Empty".to_owned(),
            version: "1.0.0".to_owned(),
            runtime: ExtensionRuntime::Native,
            command: None,
            args: Vec::new(),
            capabilities: Vec::new(),
            permissions: Vec::new(),
            signed: true,
        };
        let err = manager
            .verify(&empty_capabilities)
            .expect_err("capabilities are required");
        assert!(err.to_string().contains("must declare capabilities"));

        let missing_command = ExtensionManifest {
            runtime: ExtensionRuntime::Sidecar,
            capabilities: vec!["web.click".to_owned()],
            ..empty_capabilities
        };
        let err = manager
            .verify(&missing_command)
            .expect_err("sidecar command is required");
        assert!(err.to_string().contains("must declare command"));
    }

    #[test]
    fn start_sidecar_reports_not_found_and_native_extensions() {
        let home = temp_home();
        let manager = ExtensionManager::new(&home, true);

        let missing = manager
            .start_sidecar("greentic.desktop.missing")
            .expect_err("missing extension should fail");
        assert!(missing.to_string().contains("not found"));

        let native = built_in_extension("greentic.desktop.windows-ui")
            .expect("native built-in extension should exist");
        manager
            .install(&native)
            .expect("native install should pass");
        let err = manager
            .start_sidecar("greentic.desktop.windows-ui")
            .expect_err("native extension is not a sidecar");
        assert!(err.to_string().contains("is not a sidecar"));

        fs::remove_dir_all(home).expect("test dir should be removable");
    }

    #[test]
    fn built_in_registry_contains_all_documented_extensions() {
        for extension_id in [
            "greentic.desktop.playwright",
            "greentic.desktop.terminal-tn3270",
            "greentic.desktop.windows-ui",
            "greentic.desktop.java-accessibility",
            "greentic.desktop.vision",
            "greentic.desktop.macos.ax",
            "greentic.desktop.linux.x11",
            "greentic.desktop.linux.wayland",
        ] {
            let manifest = built_in_extension(extension_id).expect("extension should exist");
            assert_eq!(manifest.id, extension_id);
            assert!(!manifest.capabilities.is_empty());
            assert!(manifest.signed);
        }

        assert!(built_in_extension("greentic.desktop.unknown").is_none());
    }
}
