use greentic_desktop_adapter::{
    select_best_adapter, validate_required_capabilities, AdapterCapabilities, CapabilityValidation,
};
use greentic_desktop_config::RuntimeConfig;
use greentic_desktop_core::{
    evaluate_runner_package, normalize_capabilities, Capability, PackageDecision, RiskLevel,
    RunnerPackageRef,
};
use greentic_desktop_extension::{
    built_in_extension, ExtensionError, ExtensionManager, ExtensionManifest, SidecarProcess,
};
use greentic_desktop_mcp::McpServerState;
use greentic_desktop_registry::{RegistryError, SignedRunnerManifest, SigningKey};
use greentic_desktop_session::DesktopSession;
use greentic_desktop_telemetry::TelemetryLog;
use greentic_distributor_client::{
    DistributorError, GreenticDistributorClient, ResolvedArtifact, StoreExtension,
};
use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct DesktopRuntime {
    config: RuntimeConfig,
    telemetry: TelemetryLog,
    adapters: Vec<AdapterCapabilities>,
}

#[derive(Debug)]
pub enum RuntimeError {
    Io(std::io::Error),
    Extension(ExtensionError),
    Distributor(DistributorError),
    Registry(RegistryError),
    Security(String),
    InvalidCapabilities(String),
    Pack(String),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Extension(err) => write!(f, "{err}"),
            Self::Distributor(err) => write!(f, "{err}"),
            Self::Registry(err) => write!(f, "{err}"),
            Self::Security(message) | Self::InvalidCapabilities(message) | Self::Pack(message) => {
                write!(f, "{message}")
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

impl From<std::io::Error> for RuntimeError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ExtensionError> for RuntimeError {
    fn from(value: ExtensionError) -> Self {
        Self::Extension(value)
    }
}

impl From<DistributorError> for RuntimeError {
    fn from(value: DistributorError) -> Self {
        Self::Distributor(value)
    }
}

impl From<RegistryError> for RuntimeError {
    fn from(value: RegistryError) -> Self {
        Self::Registry(value)
    }
}

impl DesktopRuntime {
    pub fn new(config: RuntimeConfig) -> Self {
        Self {
            config,
            telemetry: TelemetryLog::default(),
            adapters: vec![AdapterCapabilities::new(
                "greentic.desktop.core",
                env!("CARGO_PKG_VERSION"),
                [
                    "desktop.info",
                    "desktop.runner.load",
                    "desktop.mcp.serve",
                    "evidence.log",
                ],
            )],
        }
    }

    pub fn default_capabilities(&self) -> Vec<Capability> {
        normalize_capabilities([
            Capability {
                name: "desktop.info".to_owned(),
                adapter: "core".to_owned(),
                risk: RiskLevel::Low,
            },
            Capability {
                name: "desktop.runner.load".to_owned(),
                adapter: "runtime".to_owned(),
                risk: RiskLevel::Medium,
            },
            Capability {
                name: "desktop.mcp.serve".to_owned(),
                adapter: "runtime".to_owned(),
                risk: RiskLevel::Medium,
            },
        ])
        .expect("built-in capabilities are valid")
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    pub fn telemetry(&self) -> TelemetryLog {
        self.telemetry.clone()
    }

    pub fn installed_adapters(&self) -> &[AdapterCapabilities] {
        &self.adapters
    }

    pub fn validate_required_capabilities(
        &self,
        required: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> CapabilityValidation {
        validate_required_capabilities(&self.adapters, required)
    }

    pub fn select_adapter(
        &self,
        required: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Option<&AdapterCapabilities> {
        select_best_adapter(&self.adapters, required)
    }

    pub fn info(&self) -> RuntimeInfo {
        self.telemetry.record("tool_call", "desktop.info");
        RuntimeInfo {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            os: std::env::consts::OS.to_owned(),
            installed_adapters: self
                .adapters
                .iter()
                .map(|adapter| adapter.adapter_id.clone())
                .collect(),
            registry_path: self.config.runner.home.clone(),
        }
    }

    pub fn init(&self) -> Result<(), RuntimeError> {
        self.telemetry.record("tool_call", "desktop.init");
        fs::create_dir_all(&self.config.runner.home)?;
        fs::create_dir_all(&self.config.evidence.store)?;
        fs::create_dir_all(self.config.runner.home.join("extensions"))?;
        Ok(())
    }

    pub fn extension_manager(&self) -> ExtensionManager {
        ExtensionManager::new(
            &self.config.runner.home,
            self.config.security.require_signed_extensions,
        )
    }

    pub fn resolve_extension_source(&self, source: &str) -> Result<ResolvedArtifact, RuntimeError> {
        let client =
            GreenticDistributorClient::new(self.config.runner.home.join("extension-cache"));
        client.resolve(source).map_err(Into::into)
    }

    pub fn search_extension_store(&self, query: &str) -> Vec<StoreExtension> {
        GreenticDistributorClient::new(self.config.runner.home.join("extension-cache"))
            .search(query)
    }

    pub fn extension_versions(&self, id_or_alias: &str) -> Option<Vec<String>> {
        GreenticDistributorClient::new(self.config.runner.home.join("extension-cache"))
            .versions(id_or_alias)
    }

    pub fn install_extension(&self, extension_id: &str) -> Result<ExtensionManifest, RuntimeError> {
        self.telemetry
            .record("extension_install", extension_id.to_owned());
        let artifact = self.resolve_extension_source(extension_id)?;
        let manifest = built_in_extension(&artifact.extension_id).ok_or_else(|| {
            RuntimeError::Extension(ExtensionError::NotFound(format!(
                "extension {} not found in configured registries",
                artifact.extension_id
            )))
        })?;
        self.extension_manager().install_resolved(
            &manifest,
            &artifact.resolved_uri,
            &artifact.digest,
        )?;
        Ok(manifest)
    }

    pub fn update_extension(&self, extension_id: &str) -> Result<ExtensionManifest, RuntimeError> {
        self.install_extension(extension_id)
    }

    pub fn remove_extension(&self, extension_id: &str) -> Result<(), RuntimeError> {
        self.extension_manager()
            .remove(extension_id)
            .map_err(Into::into)
    }

    pub fn set_extension_enabled(
        &self,
        extension_id: &str,
        enabled: bool,
    ) -> Result<(), RuntimeError> {
        self.extension_manager()
            .set_enabled(extension_id, enabled)
            .map_err(Into::into)
    }

    pub fn extension_health(
        &self,
        extension_id: &str,
    ) -> Result<greentic_desktop_extension::ExtensionHealth, RuntimeError> {
        self.extension_manager()
            .health(extension_id)
            .map_err(Into::into)
    }

    pub fn list_extensions(&self) -> Result<Vec<ExtensionManifest>, RuntimeError> {
        self.extension_manager().list().map_err(Into::into)
    }

    pub fn verify_extensions(&self) -> Result<Vec<ExtensionManifest>, RuntimeError> {
        let manager = self.extension_manager();
        let manifests = manager.list()?;
        for manifest in &manifests {
            manager.verify(manifest)?;
        }
        Ok(manifests)
    }

    pub fn start_sidecar(&self, extension_id: &str) -> Result<SidecarProcess, RuntimeError> {
        self.telemetry
            .record("sidecar_start", extension_id.to_owned());
        self.extension_manager()
            .start_sidecar(extension_id)
            .map_err(Into::into)
    }

    pub fn start_session(&self, id: impl Into<String>) -> DesktopSession {
        self.telemetry
            .record("session_start", "desktop session created");
        DesktopSession::new(id)
    }

    pub fn load_runner_package(
        &self,
        package: RunnerPackageRef,
    ) -> Result<LoadedRunnerPackage, RuntimeError> {
        self.telemetry
            .record("runner_load", package.path.to_string_lossy().into_owned());

        match evaluate_runner_package(
            &package,
            self.config.security.require_signed_runners,
            self.config.security.allow_unsigned_drafts,
        ) {
            PackageDecision::Allowed => Ok(LoadedRunnerPackage { package }),
            PackageDecision::RejectedUnsignedPublished => Err(RuntimeError::Security(
                "unsigned published runner packages are refused by policy".to_owned(),
            )),
        }
    }

    pub fn verify_registry_runner(
        &self,
        signed: &SignedRunnerManifest,
        key: &SigningKey,
        tenant_id: &str,
    ) -> Result<LoadedRegistryRunner, RuntimeError> {
        signed.verify(key)?;
        if signed.manifest.scope.tenant_id != tenant_id {
            return Err(RuntimeError::Registry(RegistryError::ScopeMismatch));
        }
        self.telemetry
            .record("runner_verify", signed.manifest.package_ref());
        Ok(LoadedRegistryRunner {
            runner_id: signed.manifest.runner_id.clone(),
            version: signed.manifest.version.clone(),
        })
    }

    pub fn pack_runner(
        &self,
        runner_id: &str,
        out: &Path,
    ) -> Result<GreenticPackCommandResult, RuntimeError> {
        self.telemetry.record("runner_pack", runner_id.to_owned());
        let runner_manifest = self.find_runner_manifest(runner_id)?;
        let temp = std::env::temp_dir().join(format!(
            "greentic-pack-{}-{}",
            std::process::id(),
            monotonic_nanos()
        ));
        fs::create_dir_all(&temp)?;
        let answers_path = temp.join("answers.json");
        let answers = serde_json::json!({
            "schema_version": "greentic.pack.answers.v1",
            "source": "greentic-desktop",
            "runner_id": runner_id,
            "runner_manifest_path": runner_manifest,
            "runner_definition_path": runner_manifest,
            "input_schema_path": serde_json::Value::Null,
            "output_schema_path": serde_json::Value::Null,
            "asset_paths": [],
            "evidence_policy": {
                "capture_outputs": true,
                "redact_secrets": true
            },
            "signing": {
                "mode": "greentic-pack-default"
            },
            "output_path": out,
        });
        fs::write(
            &answers_path,
            serde_json::to_vec_pretty(&answers).map_err(|err| {
                RuntimeError::Pack(format!(
                    "failed to render greentic-pack answers.json: {err}"
                ))
            })?,
        )?;
        set_owner_only_permissions(&answers_path)?;
        run_greentic_pack(["--answers".to_owned(), answers_path.display().to_string()]).map(
            |mut result| {
                result.answers_path = Some(answers_path);
                result
            },
        )
    }

    pub fn verify_runner_pack(
        &self,
        path: &Path,
    ) -> Result<GreenticPackCommandResult, RuntimeError> {
        self.telemetry
            .record("runner_pack_verify", path.display().to_string());
        run_greentic_pack(["verify".to_owned(), path.display().to_string()])
    }

    pub fn install_runner_pack(&self, path: &Path) -> Result<PathBuf, RuntimeError> {
        self.verify_runner_pack(path)?;
        let runners_dir = self.config.runner.home.join("runners");
        fs::create_dir_all(&runners_dir)?;
        let file_name = path.file_name().ok_or_else(|| {
            RuntimeError::Pack(format!(
                "runner pack path has no file name: {}",
                path.display()
            ))
        })?;
        let destination = runners_dir.join(file_name);
        fs::copy(path, &destination)?;
        Ok(destination)
    }

    pub fn serve_mcp(&self, bind: &str) -> Result<(), RuntimeError> {
        self.telemetry.record("tool_call", "desktop.mcp.serve");
        let listener = TcpListener::bind(bind)?;

        for stream in listener.incoming() {
            handle_mcp_connection(stream?)?;
        }

        Ok(())
    }

    fn find_runner_manifest(&self, runner_id: &str) -> Result<PathBuf, RuntimeError> {
        let runners_dir = self.config.runner.home.join("runners");
        [
            format!("{runner_id}.runner.json"),
            format!("{runner_id}.draft.yaml"),
            format!("{runner_id}.yaml"),
            format!("{runner_id}.json"),
        ]
        .into_iter()
        .map(|file| runners_dir.join(file))
        .find(|path| path.exists())
        .ok_or_else(|| {
            RuntimeError::Pack(format!(
                "runner manifest for {runner_id} was not found in {}",
                runners_dir.display()
            ))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GreenticPackCommandResult {
    pub stdout: String,
    pub stderr: String,
    pub answers_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeInfo {
    pub version: String,
    pub os: String,
    pub installed_adapters: Vec<String>,
    pub registry_path: std::path::PathBuf,
}

impl RuntimeInfo {
    pub fn render_human(&self) -> String {
        format!(
            "version: {}\nos: {}\nadapters: {}\nregistry: {}\n",
            self.version,
            self.os,
            self.installed_adapters.join(","),
            self.registry_path.display()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedRunnerPackage {
    pub package: RunnerPackageRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedRegistryRunner {
    pub runner_id: String,
    pub version: String,
}

pub fn discover_extensions(home: &Path) -> Result<Vec<String>, RuntimeError> {
    let manager = ExtensionManager::new(home, false);
    Ok(manager
        .list()?
        .into_iter()
        .map(|manifest| manifest.id)
        .collect())
}

pub fn discover_runners(home: &Path) -> Result<Vec<String>, RuntimeError> {
    let runners_dir = home.join("runners");
    if !runners_dir.exists() {
        return Ok(Vec::new());
    }

    let mut runners = Vec::new();
    for entry in fs::read_dir(runners_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("gtpack") {
            runners.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    runners.sort();
    Ok(runners)
}

fn run_greentic_pack(
    args: impl IntoIterator<Item = String>,
) -> Result<GreenticPackCommandResult, RuntimeError> {
    let args = args.into_iter().collect::<Vec<_>>();
    // greentic-pack is an explicit local tool dependency and is invoked directly without a shell.
    // foxguard: ignore[rs/no-command-injection]
    let output = Command::new("greentic-pack")
        .args(&args)
        .stdin(Stdio::null())
        .output()
        .map_err(|err| {
            RuntimeError::Pack(format!(
                "failed to run greentic-pack {}: {err}. Install greentic-pack and ensure it is on PATH.",
                args.join(" ")
            ))
        })?;
    let result = GreenticPackCommandResult {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        answers_path: None,
    };
    if output.status.success() {
        Ok(result)
    } else {
        Err(RuntimeError::Pack(format!(
            "greentic-pack {} failed with status {}.\nstdout:\n{}\nstderr:\n{}",
            args.join(" "),
            output.status,
            result.stdout,
            result.stderr
        )))
    }
}

fn monotonic_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn set_owner_only_permissions(path: &Path) -> Result<(), RuntimeError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

fn handle_mcp_connection(mut stream: TcpStream) -> Result<(), RuntimeError> {
    let (request_line, request_body) = read_http_request(&mut stream)?;
    let body = if request_line.starts_with("GET /health") {
        "{\"status\":\"ok\"}".to_owned()
    } else {
        let mut state = McpServerState::new(Vec::new(), Vec::<String>::new());
        state.handle_jsonrpc(&request_body)
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())?;
    Ok(())
}

fn read_http_request(stream: &mut TcpStream) -> Result<(String, String), RuntimeError> {
    let mut request = Vec::new();
    let mut chunk = [0; 1024];
    let header_end = loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break find_header_end(&request).unwrap_or(request.len());
        }
        request.extend_from_slice(&chunk[..read]);
        if let Some(header_end) = find_header_end(&request) {
            break header_end;
        }
    };
    let header = String::from_utf8_lossy(&request[..header_end]).to_string();
    let request_line = header.lines().next().unwrap_or_default().to_owned();
    let content_length = header
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let body_start = header_end + 4;
    while request.len().saturating_sub(body_start) < content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
    }
    let available = request.len().saturating_sub(body_start);
    let body_len = available.min(content_length);
    let body = String::from_utf8_lossy(&request[body_start..body_start + body_len]).to_string();
    Ok((request_line, body))
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_registry::{
        sign_manifest, RegistryStage, RunnerLifecycle, RunnerManifest, SigningKey, TenantScope,
    };
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{}-{}",
            prefix,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ))
    }

    fn signed_manifest_for(tenant_id: &str) -> (SignedRunnerManifest, SigningKey) {
        let key = SigningKey::new("local-dev", "test-material");
        let manifest = RunnerManifest {
            runner_id: "crm.create_customer".to_owned(),
            version: "1.2.0".to_owned(),
            lifecycle: RunnerLifecycle::Published,
            stage: RegistryStage::Prod,
            scope: TenantScope {
                tenant_id: tenant_id.to_owned(),
                team_id: "sales_ops".to_owned(),
                private: true,
            },
            required_adapters: vec!["greentic.desktop.playwright".to_owned()],
            compatibility: vec!["greentic-desktop>=0.1.0".to_owned()],
            package_checksum:
                "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_owned(),
        };
        let signed = sign_manifest(manifest, &key).expect("manifest should sign");
        (signed, key)
    }

    #[test]
    fn info_reports_version_os_adapter_and_registry() {
        let runtime = DesktopRuntime::new(RuntimeConfig::default());
        let info = runtime.info();

        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.os, std::env::consts::OS);
        assert!(info
            .installed_adapters
            .contains(&"greentic.desktop.core".to_owned()));
        assert_eq!(info.registry_path, runtime.config().runner.home);
    }

    #[test]
    fn init_creates_home_and_evidence_directories() {
        let root = temp_root("greentic-desktop-runtime-test");
        let mut config = RuntimeConfig::default();
        config.runner.home = root.clone();
        config.evidence.store = root.join("evidence");
        let runtime = DesktopRuntime::new(config);

        runtime.init().expect("runtime init should create dirs");
        assert!(root.is_dir());
        assert!(root.join("evidence").is_dir());
        assert!(root.join("extensions").is_dir());

        fs::remove_dir_all(root).expect("test dir should be removable");
    }

    #[test]
    fn default_capabilities_are_sorted_and_declared() {
        let runtime = DesktopRuntime::new(RuntimeConfig::default());
        let capabilities = runtime.default_capabilities();
        let names = capabilities
            .iter()
            .map(|capability| capability.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec!["desktop.info", "desktop.mcp.serve", "desktop.runner.load"]
        );
        assert!(capabilities
            .iter()
            .any(|capability| capability.risk == RiskLevel::Medium));
    }

    #[test]
    fn refuses_unsigned_published_runner_package() {
        let runtime = DesktopRuntime::new(RuntimeConfig::default());
        let err = runtime
            .load_runner_package(RunnerPackageRef::local("runner.gtpack", false, false))
            .expect_err("unsigned published package should fail");

        assert!(err
            .to_string()
            .contains("unsigned published runner packages are refused"));
    }

    #[test]
    fn loads_unsigned_draft_runner_package_when_policy_allows_it() {
        let runtime = DesktopRuntime::new(RuntimeConfig::default());
        let loaded = runtime
            .load_runner_package(RunnerPackageRef::local("draft.gtpack", false, true))
            .expect("unsigned drafts are allowed by default");

        assert_eq!(loaded.package.path, PathBuf::from("draft.gtpack"));
    }

    #[test]
    fn refuses_tampered_registry_runner_package() {
        let runtime = DesktopRuntime::new(RuntimeConfig::default());
        let (mut signed, key) = signed_manifest_for("tenant_a");
        signed.manifest.package_checksum =
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned();

        let err = runtime
            .verify_registry_runner(&signed, &key, "tenant_a")
            .expect_err("tampered manifest should fail");
        assert!(err.to_string().contains("tampered"));
    }

    #[test]
    fn verifies_signed_registry_runner_and_rejects_scope_mismatch() {
        let runtime = DesktopRuntime::new(RuntimeConfig::default());
        let (signed, key) = signed_manifest_for("tenant_a");

        let loaded = runtime
            .verify_registry_runner(&signed, &key, "tenant_a")
            .expect("matching tenant should verify");
        assert_eq!(loaded.runner_id, "crm.create_customer");
        assert_eq!(loaded.version, "1.2.0");

        let err = runtime
            .verify_registry_runner(&signed, &key, "tenant_b")
            .expect_err("wrong tenant should fail");
        assert!(err.to_string().contains("scope"));
    }

    #[test]
    fn records_tool_calls() {
        let runtime = DesktopRuntime::new(RuntimeConfig::default());
        let _ = runtime.info();

        let events = runtime.telemetry().events();
        assert!(events
            .iter()
            .any(|event| event.name == "tool_call" && event.detail == "desktop.info"));
    }

    #[test]
    fn validates_required_capabilities_before_execution() {
        let runtime = DesktopRuntime::new(RuntimeConfig::default());
        let validation = runtime.validate_required_capabilities(["desktop.info", "web.click"]);

        assert!(!validation.is_valid());
        assert_eq!(validation.missing, vec!["web.click"]);
    }

    #[test]
    fn selects_installed_adapter_for_supported_capabilities() {
        let runtime = DesktopRuntime::new(RuntimeConfig::default());
        let selected = runtime
            .select_adapter(["desktop.info", "desktop.mcp.serve"])
            .expect("core adapter supports desktop capabilities");

        assert_eq!(selected.adapter_id, "greentic.desktop.core");
    }

    #[test]
    fn installs_signed_extension_and_lists_capabilities() {
        let root = temp_root("greentic-desktop-extension-runtime-test");
        let mut config = RuntimeConfig::default();
        config.runner.home = root.clone();
        config.evidence.store = root.join("evidence");
        let runtime = DesktopRuntime::new(config);

        let manifest = runtime
            .install_extension("greentic.desktop.playwright")
            .expect("signed built-in extension should install");
        assert_eq!(manifest.id, "greentic.desktop.playwright");

        let installed = runtime.list_extensions().expect("extensions should list");
        assert_eq!(installed.len(), 1);
        assert!(installed[0].capabilities.contains(&"web.click".to_owned()));

        fs::remove_dir_all(root).expect("test dir should be removable");
    }

    #[test]
    fn installs_extension_through_distributor_sources() {
        let root = temp_root("greentic-desktop-distributor-runtime-test");
        let mut config = RuntimeConfig::default();
        config.runner.home = root.clone();
        config.evidence.store = root.join("evidence");
        let runtime = DesktopRuntime::new(config);

        let manifest = runtime
            .install_extension(
                "oci://ghcr.io/greenticai/greentic-desktop/extensions/playwright:1.0.0",
            )
            .expect("oci install should resolve through distributor");

        assert_eq!(manifest.id, "greentic.desktop.playwright");
        let resolved = runtime
            .resolve_extension_source("store://greentic.desktop.playwright")
            .expect("store source should resolve");
        assert_eq!(resolved.extension_id, "greentic.desktop.playwright");
        assert!(resolved.digest.starts_with("sha256:"));

        fs::remove_dir_all(root).expect("test dir should be removable");
    }

    #[test]
    fn starts_sidecar_extension_metadata() {
        let root = temp_root("greentic-desktop-sidecar-runtime-test");
        let mut config = RuntimeConfig::default();
        config.runner.home = root.clone();
        config.evidence.store = root.join("evidence");
        let runtime = DesktopRuntime::new(config);

        runtime
            .install_extension("greentic.desktop.playwright")
            .expect("extension install should pass");
        let sidecar = runtime
            .start_sidecar("greentic.desktop.playwright")
            .expect("sidecar metadata should prepare");

        assert_eq!(sidecar.command, "node");
        assert_eq!(sidecar.args, vec!["./index.js"]);

        fs::remove_dir_all(root).expect("test dir should be removable");
    }

    #[test]
    fn install_unknown_extension_returns_not_found() {
        let runtime = DesktopRuntime::new(RuntimeConfig::default());
        let err = runtime
            .install_extension("greentic.desktop.missing")
            .expect_err("unknown extension should fail");

        assert!(
            err.to_string().contains("not found")
                || err.to_string().contains("invalid extension source")
        );
    }

    #[test]
    fn discovers_extensions_and_sorted_runner_packages() {
        let root = temp_root("greentic-desktop-discovery-test");
        let mut config = RuntimeConfig::default();
        config.runner.home = root.clone();
        config.evidence.store = root.join("evidence");
        let runtime = DesktopRuntime::new(config);
        runtime.init().expect("runtime init should pass");
        runtime
            .install_extension("greentic.desktop.playwright")
            .expect("extension install should pass");

        let runners = root.join("runners");
        fs::create_dir_all(&runners).expect("runner dir should be created");
        fs::write(runners.join("b.gtpack"), "runner b").expect("runner should write");
        fs::write(runners.join("a.gtpack"), "runner a").expect("runner should write");
        fs::write(runners.join("ignored.txt"), "not a runner").expect("file should write");

        assert_eq!(
            discover_extensions(&root).expect("extensions should discover"),
            vec!["greentic.desktop.playwright".to_owned()]
        );
        assert_eq!(
            discover_runners(&root).expect("runners should discover"),
            vec!["a.gtpack".to_owned(), "b.gtpack".to_owned()]
        );

        fs::remove_dir_all(root).expect("test dir should be removable");
    }
}
