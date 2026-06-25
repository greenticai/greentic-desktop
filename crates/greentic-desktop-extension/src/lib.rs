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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarProcess {
    pub extension_id: String,
    pub command: String,
    pub args: Vec<String>,
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
        self.verify(manifest)?;
        let dir = self.extension_dir(&manifest.id);
        fs::create_dir_all(&dir)?;
        let path = dir.join("extension.toml");
        fs::write(&path, manifest.render_toml())?;
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
            let manifest_path = entry.path().join("extension.toml");
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
