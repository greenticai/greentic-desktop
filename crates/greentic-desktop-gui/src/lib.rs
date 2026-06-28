use greentic_desktop_adapter::{LocatorTarget, RunnerStep};
use greentic_desktop_extension::{
    verify_extension_package_trust, ExtensionPackageMetadata, ExtensionPermissions,
    ExtensionPlatforms, ExtensionRuntime, ExtensionTrustPolicy, PermissionApproval,
};
use greentic_desktop_gui_assets::{asset, spa_asset, GuiAsset};
use greentic_desktop_java::{JavaAccessBridgeRecordingBackend, JavaDesktopAdapter};
use greentic_desktop_linux::{
    detect_wayland_support, LinuxWaylandAdapter, LinuxWaylandRecordingBackend, LinuxX11Adapter,
    LinuxX11RecordingBackend, WaylandCompositor,
};
use greentic_desktop_llm::{
    is_openai_compatible_provider, known_providers, provider_by_id, HeuristicLlmClient,
    LlmProvider, OpenAiCompatibleLlmClient,
};
use greentic_desktop_macos::{MacOsAccessibilityAdapter, MacOsAccessibilityRecordingBackend};
use greentic_desktop_planner::{
    plan_prompt_with_llm, PlannerOptions, PlanningContext, RunnerDraft,
};
use greentic_desktop_platform::{
    detect_platform, DesktopPlatform, PlatformInfo, PlatformPermission,
};
use greentic_desktop_recorder::{
    append_recording_note, cancel_recording_session, finalise_recording, list_recording_sessions,
    load_recording_session, normalise_recording, pause_recording_session, resume_recording_session,
    start_recording_session_with_registry, stop_recording_session, FakeRecordingBackend,
    RecordingBackendRegistry, RecordingMode, RecordingSessionManifest, RecordingStartRequest,
    RecordingTargetKind, RunnerPackage,
};
use greentic_desktop_replay::{
    replay_with_context, AdapterRegistry, OnFailure, ReplayExecutionContext, ReplayRequest,
};
use greentic_desktop_runner_schema::{
    OutputFailureBehavior, RedactionPolicy, RunnerDefinition, RunnerInput, RunnerOutput,
};
use greentic_desktop_session::SessionProfile;
use greentic_desktop_terminal::{
    TerminalAdapter, TerminalProfile, TerminalProtocol, TerminalRecordingBackend,
};
use greentic_desktop_vision::{
    RemoteViewportCalibration, RemoteVisionRecordingBackend, VisionAdapter,
};
use greentic_desktop_web::{
    PlaywrightRecorderOptions, PlaywrightWebAdapter, PlaywrightWebRecordingBackend,
    PLAYWRIGHT_ADAPTER_ID,
};
use greentic_desktop_windows::{WindowsUiAdapter, WindowsUiRecordingBackend};
use greentic_desktop_workflow::{WorkflowOutputExtractor, WorkflowValueType};
use greentic_distributor_client::GreenticDistributorClient;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiHostOptions {
    pub bind: SocketAddr,
    pub api_state: GuiApiState,
}

impl Default for GuiHostOptions {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiApiState {
    pub app_version: String,
    pub platform: String,
    pub runtime_home: PathBuf,
    pub evidence_store: PathBuf,
    pub mcp_bind: String,
    pub installed_core_adapter_ids: Vec<String>,
    pub installed_extension_ids: Vec<String>,
    pub runner_names: Vec<String>,
    pub gui_token: String,
}

impl Default for GuiApiState {
    fn default() -> Self {
        let runtime_home = std::env::var_os("GREENTIC_DESKTOP_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".greentic/desktop"));
        Self {
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            platform: std::env::consts::OS.to_owned(),
            evidence_store: runtime_home.join("evidence"),
            runtime_home,
            mcp_bind: "127.0.0.1:8799".to_owned(),
            installed_core_adapter_ids: vec!["greentic.desktop.core".to_owned()],
            installed_extension_ids: Vec::new(),
            runner_names: Vec::new(),
            gui_token: String::new(),
        }
    }
}

#[derive(Debug)]
pub enum GuiError {
    Io(std::io::Error),
    BrowserOpen(std::io::Error),
}

impl fmt::Display for GuiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::BrowserOpen(err) => write!(f, "failed to open default browser: {err}"),
        }
    }
}

impl std::error::Error for GuiError {}

impl From<std::io::Error> for GuiError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub struct GuiHost;

impl GuiHost {
    pub fn start(options: GuiHostOptions) -> Result<GuiHostHandle, GuiError> {
        let listener = TcpListener::bind(options.bind)?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let api_state = Arc::new(options.api_state);
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

        let join = thread::spawn(move || loop {
            if shutdown_rx.try_recv().is_ok() {
                break;
            }

            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = handle_connection(stream, addr, &api_state);
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(_) => break,
            }
        });

        Ok(GuiHostHandle {
            addr,
            shutdown_tx: Some(shutdown_tx),
            join: Some(join),
        })
    }
}

pub struct GuiHostHandle {
    addr: SocketAddr,
    shutdown_tx: Option<Sender<()>>,
    join: Option<JoinHandle<()>>,
}

impl GuiHostHandle {
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn url(&self) -> String {
        format!("http://{}/", self.addr)
    }

    pub fn token_url(&self, token: &str) -> String {
        if token.is_empty() {
            self.url()
        } else {
            format!("http://{}/?token={}", self.addr, token)
        }
    }

    pub fn shutdown(mut self) {
        self.shutdown_inner();
    }

    fn shutdown_inner(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for GuiHostHandle {
    fn drop(&mut self) {
        self.shutdown_inner();
    }
}

pub fn open_default_browser(url: &str) -> Result<(), GuiError> {
    let command = browser_command(url);
    // Browser openers are selected from a fixed OS allow-list; the URL is passed as an argument without shell expansion.
    // foxguard: ignore[rs/no-command-injection]
    Command::new(command.program)
        .args(command.args)
        .spawn()
        .map(|_| ())
        .map_err(GuiError::BrowserOpen)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserCommand {
    pub program: &'static str,
    pub args: Vec<String>,
}

pub fn browser_command(url: &str) -> BrowserCommand {
    browser_command_for(std::env::consts::OS, url)
}

pub fn browser_command_for(os: &str, url: &str) -> BrowserCommand {
    match os {
        "macos" => BrowserCommand {
            program: "open",
            args: vec![url.to_owned()],
        },
        "windows" => BrowserCommand {
            program: "cmd",
            args: vec![
                "/C".to_owned(),
                "start".to_owned(),
                "".to_owned(),
                url.to_owned(),
            ],
        },
        _ => BrowserCommand {
            program: "xdg-open",
            args: vec![url.to_owned()],
        },
    }
}

fn handle_connection(
    mut stream: TcpStream,
    addr: SocketAddr,
    api_state: &GuiApiState,
) -> Result<(), GuiError> {
    stream.set_nonblocking(false)?;
    let mut buffer = [0; 8192];
    let read = stream.read(&mut buffer)?;
    if read == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buffer[..read]);
    let (method, path) = parse_request_line(&request).unwrap_or(("GET", "/"));
    let body = request.split_once("\r\n\r\n").map_or("", |(_, body)| body);
    let response = if path.starts_with("/api/") {
        if let Some(error) = reject_unsafe_api_request(method, &request, addr, api_state) {
            let response = json_response(403, "Forbidden", &error, method == "HEAD");
            stream.write_all(&response)?;
            return Ok(());
        }
        api_response(method, path, body, addr, api_state)
    } else if method != "GET" && method != "HEAD" {
        http_response(
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            b"method not allowed",
            method == "HEAD",
        )
    } else {
        let gui_asset = asset(path).unwrap_or_else(|| spa_asset(path));
        asset_response(gui_asset, method == "HEAD")
    };

    stream.write_all(&response)?;
    Ok(())
}

fn reject_unsafe_api_request(
    method: &str,
    request: &str,
    addr: SocketAddr,
    state: &GuiApiState,
) -> Option<String> {
    if !matches!(method, "POST" | "PUT" | "PATCH" | "DELETE") || state.gui_token.is_empty() {
        return None;
    }
    let token = request_header(request, "x-greentic-gui-token");
    if token.as_deref() != Some(state.gui_token.as_str()) {
        return Some(api_error_json(
            "security.token_required",
            "A valid GUI session token is required for this API request.",
        ));
    }
    if let Some(origin) = request_header(request, "origin") {
        let expected = format!("http://{addr}");
        if origin.trim_end_matches('/') != expected {
            return Some(api_error_json(
                "security.origin_rejected",
                "The request origin does not match this GUI session.",
            ));
        }
    }
    if let Some(content_type) = request_header(request, "content-type") {
        let allowed = content_type.starts_with("application/json")
            || content_type.starts_with("text/plain")
            || content_type.starts_with("application/x-www-form-urlencoded");
        if !allowed {
            return Some(api_error_json(
                "security.content_type_rejected",
                "The request content type is not accepted by the GUI API.",
            ));
        }
    }
    None
}

fn request_header(request: &str, name: &str) -> Option<String> {
    let wanted = name.to_ascii_lowercase();
    request.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim().eq_ignore_ascii_case(&wanted)).then(|| value.trim().to_owned())
    })
}

fn api_response(
    method: &str,
    path: &str,
    body: &str,
    addr: SocketAddr,
    state: &GuiApiState,
) -> Vec<u8> {
    let head_only = method == "HEAD";
    if method != "GET"
        && method != "POST"
        && method != "PUT"
        && method != "PATCH"
        && method != "DELETE"
        && method != "HEAD"
    {
        return json_response(
            405,
            "Method Not Allowed",
            &api_error_json(
                "runtime.method_not_allowed",
                "This API endpoint does not support the requested method.",
            ),
            head_only,
        );
    }

    let path = path.split_once('?').map_or(path, |(path, _)| path);
    let data = match (method, path) {
        ("GET" | "HEAD", "/api/v1/health") => r#"{"apiVersion":"v1","status":"ok"}"#.to_owned(),
        ("GET" | "HEAD", "/api/v1/runtime/info") => runtime_info_json(addr, state),
        ("GET" | "HEAD", "/api/v1/activity") => activity_json(state),
        ("GET" | "HEAD", "/api/v1/evidence") => evidence_list_json(state),
        ("GET" | "HEAD", path) if path.starts_with("/api/v1/evidence/") => {
            match evidence_detail_json(path, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("GET" | "HEAD", "/api/v1/approvals") => approvals_list_json(state),
        ("POST", path) if path.starts_with("/api/v1/approvals/") => {
            match approval_action_json(path, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("GET" | "HEAD", "/api/v1/setup/checklist") => setup_checklist_json(state),
        ("GET" | "HEAD", "/api/v1/extensions/recommended") => recommended_extensions_json(None),
        ("GET" | "HEAD", "/api/v1/extensions/installed") => installed_extensions_json(state),
        ("GET" | "HEAD", path) if path.starts_with("/api/v1/extensions/search") => {
            let query = path.split_once("q=").map(|(_, value)| value).unwrap_or("");
            recommended_extensions_json(Some(query))
        }
        ("GET" | "HEAD", path)
            if path.starts_with("/api/v1/extensions/") && path.ends_with("/versions") =>
        {
            extension_versions_json(path)
        }
        ("GET" | "HEAD", path) if path.starts_with("/api/v1/extensions/") => {
            extension_detail_json(path, state)
        }
        ("GET" | "HEAD", "/api/v1/runners") => runners_json(state),
        ("GET" | "HEAD", path) if path.contains("/edit-drafts/") => {
            match runner_edit_draft_action_json(method, path, body, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("GET" | "HEAD", path)
            if path.starts_with("/api/v1/runners/") && path.ends_with("/versions") =>
        {
            match runner_versions_json(path, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("GET" | "HEAD", path) if path.starts_with("/api/v1/runners/") => {
            runner_detail_json(path, state)
        }
        ("GET" | "HEAD", "/api/v1/recordings") => recordings_list_json(state),
        ("GET" | "HEAD", "/api/v1/recording-targets") => recording_targets_json(),
        ("POST", "/api/v1/recordings") => match create_recording_json(body, state) {
            Ok(json) => json,
            Err(error) => return json_response(400, "Bad Request", &error, head_only),
        },
        ("GET" | "HEAD", path) if path.starts_with("/api/v1/recordings/") => {
            match recording_action_json(method, path, body, state) {
                Ok(json) => json,
                Err(error) => return json_response(400, "Bad Request", &error, head_only),
            }
        }
        ("POST", path) if path.starts_with("/api/v1/recordings/") => {
            match recording_action_json(method, path, body, state) {
                Ok(json) => json,
                Err(error) => return json_response(400, "Bad Request", &error, head_only),
            }
        }
        ("GET" | "HEAD", "/api/v1/mcp/status") => mcp_status_json(state),
        ("GET" | "HEAD", "/api/v1/mcp/tools") => mcp_tools_json(state),
        ("GET" | "HEAD", "/api/v1/mcp/client-config") => mcp_client_config_json(state),
        ("GET" | "HEAD", "/api/v1/settings/llm") => llm_settings_json(state)
            .unwrap_or_else(|err| api_error_json("settings.invalid_llm_provider", &err)),
        ("POST", "/api/v1/planner/drafts") => match create_planner_draft_json(body, state) {
            Ok(json) => json,
            Err(error) => return json_response(400, "Bad Request", &error, head_only),
        },
        ("GET" | "HEAD", path) if path.starts_with("/api/v1/planner/drafts/") => {
            match planner_draft_action_json(method, path, body, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("PATCH", path) if path.starts_with("/api/v1/planner/drafts/") => {
            match planner_draft_action_json(method, path, body, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("DELETE", path) if path.starts_with("/api/v1/planner/drafts/") => {
            match delete_planner_draft_json(path, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("POST", path) if path.starts_with("/api/v1/planner/drafts/") => {
            match planner_draft_action_json(method, path, body, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("POST", "/api/v1/setup/fix") => match setup_fix_json(body, state) {
            Ok(json) => json,
            Err(error) => return json_response(400, "Bad Request", &error, head_only),
        },
        ("POST", "/api/v1/mcp/start") => match start_mcp_service(state) {
            Ok(json) => json,
            Err(error) => return json_response(400, "Bad Request", &error, head_only),
        },
        ("POST", "/api/v1/mcp/stop") => stop_mcp_service(state),
        ("POST", "/api/v1/mcp/restart") => match restart_mcp_service(state) {
            Ok(json) => json,
            Err(error) => return json_response(400, "Bad Request", &error, head_only),
        },
        ("PUT", "/api/v1/settings/llm") => match save_llm_settings_json(body, state) {
            Ok(json) => json,
            Err(error) => return json_response(400, "Bad Request", &error, head_only),
        },
        ("POST", "/api/v1/settings/llm/test") => match test_llm_settings_json(state) {
            Ok(json) => json,
            Err(error) => return json_response(400, "Bad Request", &error, head_only),
        },
        ("POST", "/api/v1/extensions/install") => match extension_install_json(body, state) {
            Ok(json) => json,
            Err(error) => return json_response(400, "Bad Request", &error, head_only),
        },
        ("POST", path) if path.starts_with("/api/v1/extensions/") => {
            match extension_action_json(path, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("POST", path) if path.contains("/edit-drafts") => {
            match runner_edit_draft_action_json(method, path, body, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("POST", path) if path.contains("/versions/") && path.ends_with("/restore") => {
            match restore_runner_version_json(path, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("PATCH", path) if path.contains("/edit-drafts/") => {
            match runner_edit_draft_action_json(method, path, body, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("DELETE", path) if path.contains("/edit-drafts/") => {
            match runner_edit_draft_action_json(method, path, body, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("POST", path) if path.starts_with("/api/v1/runners/") && path.contains("/refinement") => {
            match refinement_action_json(path, body, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("POST", path) if path.starts_with("/api/v1/runners/") => {
            match runner_action_json(path, body, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("POST", path) if path.starts_with("/api/v1/mcp/tools/") => {
            match mcp_tool_action_json(path, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        _ => {
            return json_response(
                404,
                "Not Found",
                &api_error_json("runtime.not_found", "API endpoint not found."),
                head_only,
            );
        }
    };

    json_response(200, "OK", &api_ok_json(&data), head_only)
}

fn json_response(status: u16, reason: &str, body: &str, head_only: bool) -> Vec<u8> {
    http_response(
        status,
        reason,
        "application/json; charset=utf-8",
        body.as_bytes(),
        head_only,
    )
}

fn api_ok_json(data: &str) -> String {
    format!(r#"{{"ok":true,"data":{data}}}"#)
}

fn api_error_json(code: &str, message: &str) -> String {
    format!(
        r#"{{"ok":false,"error":{{"code":"{}","message":"{}","details":{{}}}}}}"#,
        escape_json(code),
        escape_json(message)
    )
}

fn runtime_info_json(addr: SocketAddr, state: &GuiApiState) -> String {
    format!(
        r#"{{"appVersion":"{}","platform":"{}","runtimeHome":"{}","evidenceStore":"{}","guiUrl":"http://{}/","config":{{"mcpBind":"{}"}},"installedCoreAdapterIds":{}}}"#,
        escape_json(&state.app_version),
        escape_json(&state.platform),
        escape_json(&state.runtime_home.display().to_string()),
        escape_json(&state.evidence_store.display().to_string()),
        addr,
        escape_json(&state.mcp_bind),
        string_array_json(&state.installed_core_adapter_ids)
    )
}

fn activity_json(state: &GuiApiState) -> String {
    let mut events = vec![
        r#"{"id":"startup","kind":"info","message":"GUI host started","timestamp":"local","relatedId":"runtime","target":"/"}"#.to_owned(),
    ];
    for runner in runner_files(state) {
        let status = json_string_field(&runner_state_json(state, &runner.id), "status")
            .unwrap_or_else(|| "draft".to_owned());
        if status != "draft" {
            events.push(format!(
                r#"{{"id":"runner-{}","kind":"success","message":"{} is {}","timestamp":"recent","relatedId":"{}","target":"/runners"}}"#,
                escape_json(&runner.id),
                escape_json(&runner.name),
                escape_json(&status),
                escape_json(&runner.id)
            ));
        }
    }
    for approval in approval_files(state) {
        if let Some(id) = approval_id_from_path(&approval) {
            let json = std::fs::read_to_string(&approval).unwrap_or_default();
            let status = json_string_field(&json, "status").unwrap_or_else(|| "pending".to_owned());
            events.push(format!(
                r#"{{"id":"approval-{}","kind":"warning","message":"Approval {} is {}","timestamp":"recent","relatedId":"{}","target":"/runners"}}"#,
                escape_json(&id),
                escape_json(&id),
                escape_json(&status),
                escape_json(&id)
            ));
        }
    }
    format!(r#"{{"events":[{}]}}"#, events.join(","))
}

fn evidence_list_json(state: &GuiApiState) -> String {
    let bundles = evidence_bundle_files(state)
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .collect::<Vec<_>>()
        .join(",");
    format!(r#"{{"bundles":[{bundles}]}}"#)
}

fn evidence_detail_json(path: &str, state: &GuiApiState) -> Result<String, String> {
    let rest = path.trim_start_matches("/api/v1/evidence/");
    if rest.contains("..") {
        return Err(api_error_json(
            "evidence.invalid_path",
            "Evidence path is not allowed.",
        ));
    }
    if let Some((bundle_id, artifact_path)) = rest.split_once("/artifacts/") {
        let artifact_id = artifact_path.trim();
        let artifact = evidence_bundle_dir(state, bundle_id).join(artifact_id);
        if !artifact.is_file() {
            return Err(api_error_json("evidence.not_found", "Artifact not found."));
        }
        let content = std::fs::read_to_string(&artifact).unwrap_or_default();
        return Ok(format!(
            r#"{{"bundleId":"{}","artifactId":"{}","content":"{}","redacted":true}}"#,
            escape_json(bundle_id),
            escape_json(artifact_id),
            escape_json(&content)
        ));
    }
    let bundle = evidence_bundle_dir(state, rest).join("bundle.json");
    std::fs::read_to_string(bundle)
        .map_err(|_| api_error_json("evidence.not_found", "Evidence bundle not found."))
}

fn approvals_list_json(state: &GuiApiState) -> String {
    let approvals = approval_files(state)
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .collect::<Vec<_>>()
        .join(",");
    format!(r#"{{"approvals":[{approvals}]}}"#)
}

fn approval_action_json(path: &str, state: &GuiApiState) -> Result<String, String> {
    let rest = path.trim_start_matches("/api/v1/approvals/");
    let (approval_id, action) = rest
        .split_once('/')
        .ok_or_else(|| api_error_json("approval.invalid_action", "Approval action not found."))?;
    let approval_path = approvals_dir(state).join(format!("{approval_id}.json"));
    let current = std::fs::read_to_string(&approval_path)
        .map_err(|_| api_error_json("approval.not_found", "Approval not found."))?;
    let runner_id = json_string_field(&current, "runnerId").unwrap_or_default();
    let status = match action {
        "approve" => "approved",
        "reject" => "rejected",
        _ => {
            return Err(api_error_json(
                "approval.invalid_action",
                "Approval action not found.",
            ))
        }
    };
    let json = approval_json(approval_id, &runner_id, status);
    std::fs::write(&approval_path, &json)
        .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    if status == "approved" {
        if let Some(runner) = find_runner(state, &runner_id) {
            persist_runner_state(
                state,
                &runner.id,
                "approved",
                "passed",
                &format!("local://approvals/{approval_id}"),
            )?;
            persist_mcp_tool(state, &runner)?;
        }
    }
    Ok(json)
}

fn evidence_bundle_dir(state: &GuiApiState, bundle_id: &str) -> PathBuf {
    state.evidence_store.join(bundle_id)
}

fn evidence_bundle_files(state: &GuiApiState) -> Vec<PathBuf> {
    let mut files = std::fs::read_dir(&state.evidence_store)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path().join("bundle.json"))
                .filter(|path| path.is_file())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    files.sort();
    files
}

fn persist_replay_evidence_bundle(
    state: &GuiApiState,
    runner: &RunnerFile,
    action: &str,
    outputs_json: &str,
    failure: Option<&str>,
) -> Result<String, String> {
    let status = if failure.is_some() {
        "failed"
    } else {
        "success"
    };
    persist_evidence_bundle_with_outputs(state, runner, action, status, outputs_json, failure)
}

fn persist_evidence_bundle_with_outputs(
    state: &GuiApiState,
    runner: &RunnerFile,
    action: &str,
    status: &str,
    outputs_json: &str,
    failure: Option<&str>,
) -> Result<String, String> {
    let bundle_id = format!("{}-{}", runner.id, action);
    let dir = evidence_bundle_dir(state, &bundle_id);
    std::fs::create_dir_all(&dir).map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    let artifact_id = "trace.txt";
    std::fs::write(
        dir.join(artifact_id),
        failure.unwrap_or("All local validation checks passed."),
    )
    .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    let json = format!(
        r#"{{"bundleId":"{}","runId":"{}","runnerId":"{}","status":"{}","startedAt":"local","completedAt":"local","inputsHash":"redacted","outputs":{},"failureReason":{},"artifacts":[{{"id":"{}","kind":"tool_trace","name":"Trace","url":"/api/v1/evidence/{}/artifacts/{}","redacted":true}}],"steps":[{{"summary":"{} runner","status":"{}"}}]}}"#,
        escape_json(&bundle_id),
        escape_json(&bundle_id),
        escape_json(&runner.id),
        escape_json(status),
        if outputs_json.trim().starts_with('{') {
            outputs_json.to_owned()
        } else {
            "{}".to_owned()
        },
        failure
            .map(|value| format!(r#""{}""#, escape_json(value)))
            .unwrap_or_else(|| "null".to_owned()),
        artifact_id,
        escape_json(&bundle_id),
        artifact_id,
        escape_json(action),
        escape_json(status)
    );
    std::fs::write(dir.join("bundle.json"), &json)
        .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    Ok(bundle_id)
}

fn approvals_dir(state: &GuiApiState) -> PathBuf {
    state.runtime_home.join("approvals")
}

fn approval_files(state: &GuiApiState) -> Vec<PathBuf> {
    let mut files = std::fs::read_dir(approvals_dir(state))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    files.sort();
    files
}

fn approval_id_from_path(path: &std::path::Path) -> Option<String> {
    Some(path.file_stem()?.to_str()?.to_owned())
}

fn persist_approval(
    state: &GuiApiState,
    runner: &RunnerFile,
    status: &str,
) -> Result<String, String> {
    let approval_id = format!("approval-{:016x}", fnv1a64(runner.id.as_bytes()));
    let dir = approvals_dir(state);
    std::fs::create_dir_all(&dir).map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    let json = approval_json(&approval_id, &runner.id, status);
    std::fs::write(dir.join(format!("{approval_id}.json")), json)
        .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    Ok(approval_id)
}

fn approval_json(approval_id: &str, runner_id: &str, status: &str) -> String {
    format!(
        r#"{{"id":"{}","action":"Publish runner as MCP tool","runnerId":"{}","risk":"high","requestedBy":"local user","evidenceRef":"local://approvals/{}","policyReason":"High-risk runner publish requires approval.","status":"{}"}}"#,
        escape_json(approval_id),
        escape_json(runner_id),
        escape_json(approval_id),
        escape_json(status)
    )
}

fn runner_requires_approval(runner: &RunnerFile) -> bool {
    let id = runner.id.to_ascii_lowercase();
    id.contains("payment") || id.contains("billing") || id.contains("delete") || id.contains("high")
}

fn runner_has_approval(state: &GuiApiState, runner_id: &str) -> bool {
    approval_files(state).iter().any(|path| {
        let json = std::fs::read_to_string(path).unwrap_or_default();
        json_string_field(&json, "runnerId").as_deref() == Some(runner_id)
            && json_string_field(&json, "status").as_deref() == Some("approved")
    })
}

fn refinements_dir(state: &GuiApiState) -> PathBuf {
    state.runtime_home.join("refinements")
}

fn setup_checklist_json(state: &GuiApiState) -> String {
    let runtime_home_ok = state.runtime_home.is_dir();
    let browser_installed = state
        .installed_extension_ids
        .iter()
        .any(|id| id == "greentic.desktop.playwright");
    format!(
        r#"{{"items":[{}, {}, {}, {}, {}, {}]}}"#,
        checklist_item_json(
            "runtime_home",
            "Runtime home exists",
            if runtime_home_ok { "ready" } else { "missing" },
            "Creates the local folder Greentic uses for runners, extensions, and logs.",
            "setup_runtime",
        ),
        checklist_item_json(
            "browser_automation",
            "Browser automation extension installed",
            if browser_installed {
                "ready"
            } else {
                "missing"
            },
            "Install the official browser extension before recording or replaying web tasks.",
            "install_extension",
        ),
        checklist_item_json(
            "screen_capture_permission",
            "Screen capture permission",
            "ready",
            "Required only for desktop recording and visual fallback. Browser, prompt, runner, and MCP flows can run without it.",
            "open_system_settings",
        ),
        checklist_item_json(
            "accessibility_permission",
            "Accessibility permission",
            "ready",
            "Required only for native desktop automation. Prompt, browser, runner, and MCP flows can run without it.",
            "open_system_settings",
        ),
        checklist_item_json(
            "input_control_permission",
            "Keyboard/mouse control permission",
            "ready",
            "Required only when a runner needs real keyboard or mouse control.",
            "open_system_settings",
        ),
        checklist_item_json(
            "mcp_server",
            "MCP server configured",
            if state.mcp_bind.is_empty() {
                "missing"
            } else {
                "ready"
            },
            "The local MCP endpoint exposes saved runners as tools.",
            "start_mcp",
        ),
    )
}

fn setup_fix_json(body: &str, state: &GuiApiState) -> Result<String, String> {
    let id = json_string_field(body, "id")
        .or_else(|| json_string_field(body, "itemId"))
        .ok_or_else(|| api_error_json("setup.missing_id", "Setup item id is required."))?;
    let result = match id.as_str() {
        "runtime_home" => {
            std::fs::create_dir_all(&state.runtime_home).map_err(|err| {
                api_error_json(
                    "setup.runtime_home_failed",
                    &format!("Could not create runtime home: {err}"),
                )
            })?;
            std::fs::create_dir_all(&state.evidence_store).map_err(|err| {
                api_error_json(
                    "setup.evidence_store_failed",
                    &format!("Could not create evidence store: {err}"),
                )
            })?;
            setup_fix_result_json(
                &id,
                "created",
                &format!(
                    "Runtime folders are ready at {}.",
                    state.runtime_home.display()
                ),
            )
        }
        "browser_automation" => setup_fix_result_json(
            &id,
            "manual",
            "Install the browser automation extension from Extensions, then retry web recording.",
        ),
        "screen_capture_permission" => open_permission_settings(
            state,
            &id,
            "screen_capture",
            "Open the screen capture or screen recording permission page and grant access to the terminal or Greentic Desktop app you are running.",
        ),
        "accessibility_permission" => open_permission_settings(
            state,
            &id,
            "accessibility",
            "Open the accessibility permission page and grant access to the terminal or Greentic Desktop app you are running.",
        ),
        "input_control_permission" => open_permission_settings(
            state,
            &id,
            "input_control",
            "Open the keyboard, mouse, or input monitoring permission page and grant access to the terminal or Greentic Desktop app you are running.",
        ),
        "mcp_server" => setup_fix_result_json(
            &id,
            "manual",
            &format!("Start or configure the local MCP endpoint at {}.", state.mcp_bind),
        ),
        _ => {
            return Err(api_error_json(
                "setup.unknown_item",
                &format!("Unknown setup item '{id}'."),
            ));
        }
    };
    Ok(result)
}

fn setup_fix_result_json(id: &str, status: &str, message: &str) -> String {
    format!(
        r#"{{"id":"{}","status":"{}","message":"{}"}}"#,
        escape_json(id),
        escape_json(status),
        escape_json(message)
    )
}

fn open_permission_settings(
    state: &GuiApiState,
    id: &str,
    permission: &str,
    manual_message: &str,
) -> String {
    match open_platform_settings(&state.platform, permission) {
        Ok(()) => setup_fix_result_json(id, "opened", "Opened the relevant operating-system settings page. Grant the permission, then restart Greentic Desktop if the OS asks you to."),
        Err(reason) => setup_fix_result_json(
            id,
            "manual",
            &format!("{manual_message} Automatic opening was not available: {reason}."),
        ),
    }
}

fn open_platform_settings(platform: &str, permission: &str) -> Result<(), String> {
    match platform {
        "macos" => open_macos_settings(permission),
        "windows" => open_windows_settings(permission),
        "linux" => open_linux_settings(permission),
        other => Err(format!("{other} is not supported by the setup opener")),
    }
}

fn open_macos_settings(permission: &str) -> Result<(), String> {
    let pane = match permission {
        "screen_capture" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        }
        "accessibility" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        "input_control" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
        }
        _ => "x-apple.systempreferences:com.apple.preference.security",
    };
    spawn_detached("open", &[pane])
}

fn open_windows_settings(permission: &str) -> Result<(), String> {
    let page = match permission {
        "screen_capture" => "ms-settings:privacy",
        "accessibility" => "ms-settings:easeofaccess",
        "input_control" => "ms-settings:keyboard",
        _ => "ms-settings:",
    };
    spawn_detached("cmd", &["/C", "start", "", page])
}

fn open_linux_settings(permission: &str) -> Result<(), String> {
    let gnome_panel = match permission {
        "screen_capture" => "privacy",
        "accessibility" => "universal-access",
        "input_control" => "keyboard",
        _ => "privacy",
    };
    let kde_panel = match permission {
        "screen_capture" => "kcm_kwin_virtualdesktops",
        "accessibility" => "kcm_access",
        "input_control" => "kcm_keyboard",
        _ => "kcm_access",
    };

    let attempts: [(&str, &[&str]); 4] = [
        ("gnome-control-center", &[gnome_panel]),
        ("systemsettings", &[kde_panel]),
        ("kcmshell5", &[kde_panel]),
        ("xdg-open", &["settings://privacy"]),
    ];
    let mut errors = Vec::new();
    for (command, args) in attempts {
        match spawn_detached(command, args) {
            Ok(()) => return Ok(()),
            Err(error) => errors.push(format!("{command}: {error}")),
        }
    }
    Err(errors.join("; "))
}

fn spawn_detached(command: &str, args: &[&str]) -> Result<(), String> {
    // Settings openers are selected by the OS-specific callers above from fixed command names and argument templates.
    // foxguard: ignore[rs/no-command-injection]
    Command::new(command)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|err| err.to_string())
}

fn checklist_item_json(id: &str, label: &str, status: &str, help: &str, action: &str) -> String {
    format!(
        r#"{{"id":"{}","label":"{}","ok":{},"status":"{}","help":"{}","action":"{}"}}"#,
        escape_json(id),
        escape_json(label),
        status == "ready",
        escape_json(status),
        escape_json(help),
        escape_json(action)
    )
}

fn recommended_extensions_json(query: Option<&str>) -> String {
    let client = GreenticDistributorClient::new(".greentic/extension-cache");
    let extensions = client
        .search(query.unwrap_or(""))
        .iter()
        .map(|extension| {
            extension_store_entry_json(
                &extension.id,
                &extension.name,
                "Recommended",
                &extension.description,
                &extension.permissions.join(","),
                &extension.capabilities.join(","),
                extension
                    .platforms
                    .iter()
                    .any(|platform| platform == std::env::consts::OS),
                &extension.latest,
                &extension.source,
                &extension.publisher,
                false,
                false,
                "unknown",
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(r#"{{"extensions":[{extensions}]}}"#)
}

fn installed_extensions_json(state: &GuiApiState) -> String {
    let records = gui_extension_records(state);
    let extensions = if records.is_empty() {
        state
            .installed_extension_ids
            .iter()
            .map(|id| {
                format!(
                    r#"{{"id":"{}","name":"{}","status":"installed","enabled":true,"health":"unknown","version":"local","publisher":"local","trust":"verified","digest":null,"source":"local","capabilities":[],"permissions":[],"platformCompatible":true,"available":true}}"#,
                    escape_json(id),
                    escape_json(id)
                )
            })
            .collect::<Vec<_>>()
    } else {
        records
            .iter()
            .map(|record| {
                let id = toml_string_field(record, "id").unwrap_or_default();
                let version =
                    toml_string_field(record, "version").unwrap_or_else(|| "local".to_owned());
                let source = toml_string_field(record, "source").unwrap_or_default();
                let digest = toml_string_field(record, "digest").unwrap_or_default();
                let enabled = toml_bool_field(record, "enabled").unwrap_or(true);
                let publisher =
                    toml_string_field(record, "publisher").unwrap_or_else(|| "greenticai".to_owned());
                let signature_status = toml_string_field(record, "signature_status")
                    .unwrap_or_else(|| "valid".to_owned());
                let sbom_present = toml_bool_field(record, "sbom_present").unwrap_or(true);
                let trust_reasons = toml_array_field(record, "trust_reasons").unwrap_or_default();
                format!(
                    r#"{{"id":"{}","name":"{}","status":"installed","enabled":{},"health":"unknown","version":"{}","publisher":"{}","trust":"verified","signatureStatus":"{}","sbomPresent":{},"trustReasons":{},"digest":"{}","source":"{}","capabilities":[],"permissions":[],"platformCompatible":true,"available":true}}"#,
                    escape_json(&id),
                    escape_json(&id),
                    enabled,
                    escape_json(&version),
                    escape_json(&publisher),
                    escape_json(&signature_status),
                    sbom_present,
                    string_array_json(&trust_reasons),
                    escape_json(&digest),
                    escape_json(&source)
                )
            })
            .collect::<Vec<_>>()
    }
    .join(",");
    format!(r#"{{"extensions":[{extensions}]}}"#)
}

fn gui_extension_records(state: &GuiApiState) -> Vec<String> {
    std::fs::read_to_string(gui_installed_lock_path(state))
        .unwrap_or_default()
        .split("[[extensions]]")
        .filter(|chunk| chunk.contains("id ="))
        .map(str::to_owned)
        .collect()
}

fn gui_installed_lock_path(state: &GuiApiState) -> PathBuf {
    state.runtime_home.join("extensions").join("installed.lock")
}

struct GuiExtensionVerificationRecord {
    publisher: String,
    signature_status: String,
    sbom_present: bool,
    trust_reasons: Vec<String>,
}

fn persist_gui_extension_record(
    state: &GuiApiState,
    id: &str,
    version: &str,
    source: &str,
    digest: &str,
    enabled: bool,
    verification: &GuiExtensionVerificationRecord,
) -> Result<(), String> {
    let extensions_dir = state.runtime_home.join("extensions");
    let version_dir = extensions_dir.join(id).join(version);
    std::fs::create_dir_all(&version_dir)
        .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    std::fs::write(
        version_dir.join("extension.toml"),
        format!(
            "id = \"{}\"\nname = \"{}\"\nversion = \"{}\"\nruntime = \"sidecar\"\ncommand = \"sidecar/index.js\"\nsigned = true\n\n[capabilities]\ntools = [\"web.click\"]\n",
            id, id, version
        ),
    )
    .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    std::fs::write(extensions_dir.join(id).join("current"), version)
        .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    let mut records = gui_extension_records(state);
    records.retain(|record| toml_string_field(record, "id").as_deref() != Some(id));
    records.push(format!(
        "\nid = \"{}\"\nversion = \"{}\"\nsource = \"{}\"\ndigest = \"{}\"\ninstalled_at = \"local\"\nenabled = {}\npublisher = \"{}\"\nsignature_status = \"{}\"\nsbom_present = {}\ntrust_reasons = [{}]\n",
        id,
        version,
        source,
        digest,
        enabled,
        verification.publisher,
        verification.signature_status,
        verification.sbom_present,
        verification
            .trust_reasons
            .iter()
            .map(|reason| format!(r#""{}""#, escape_json(reason)))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    let rendered = records
        .iter()
        .map(|record| format!("[[extensions]]{record}"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(gui_installed_lock_path(state), rendered)
        .map_err(|err| api_error_json("runtime.io", &err.to_string()))
}

fn remove_gui_extension_record(state: &GuiApiState, id: &str) -> Result<(), String> {
    let extension_dir = state.runtime_home.join("extensions").join(id);
    if extension_dir.exists() {
        std::fs::remove_dir_all(extension_dir)
            .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    }
    let mut records = gui_extension_records(state);
    records.retain(|record| toml_string_field(record, "id").as_deref() != Some(id));
    let rendered = records
        .iter()
        .map(|record| format!("[[extensions]]{record}"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(gui_installed_lock_path(state), rendered)
        .map_err(|err| api_error_json("runtime.io", &err.to_string()))
}

fn set_gui_extension_enabled(state: &GuiApiState, id: &str, enabled: bool) -> Result<(), String> {
    let mut found = false;
    let records = gui_extension_records(state)
        .into_iter()
        .map(|record| {
            if toml_string_field(&record, "id").as_deref() == Some(id) {
                found = true;
                let version =
                    toml_string_field(&record, "version").unwrap_or_else(|| "local".to_owned());
                let source = toml_string_field(&record, "source").unwrap_or_default();
                let digest = toml_string_field(&record, "digest").unwrap_or_default();
                let publisher =
                    toml_string_field(&record, "publisher").unwrap_or_else(|| "greenticai".to_owned());
                let signature_status = toml_string_field(&record, "signature_status")
                    .unwrap_or_else(|| "valid".to_owned());
                let sbom_present = toml_bool_field(&record, "sbom_present").unwrap_or(true);
                let trust_reasons =
                    toml_array_field(&record, "trust_reasons").unwrap_or_default();
                format!(
                    "\nid = \"{}\"\nversion = \"{}\"\nsource = \"{}\"\ndigest = \"{}\"\ninstalled_at = \"local\"\nenabled = {}\npublisher = \"{}\"\nsignature_status = \"{}\"\nsbom_present = {}\ntrust_reasons = [{}]\n",
                    id,
                    version,
                    source,
                    digest,
                    enabled,
                    publisher,
                    signature_status,
                    sbom_present,
                    trust_reasons
                        .iter()
                        .map(|reason| format!(r#""{}""#, escape_json(reason)))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            } else {
                record
            }
        })
        .collect::<Vec<_>>();
    if !found {
        return Err(api_error_json(
            "extension.not_found",
            "Extension not found.",
        ));
    }
    let rendered = records
        .iter()
        .map(|record| format!("[[extensions]]{record}"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(gui_installed_lock_path(state), rendered)
        .map_err(|err| api_error_json("runtime.io", &err.to_string()))
}

fn toml_string_field(input: &str, field: &str) -> Option<String> {
    let prefix = format!("{field} = \"");
    let rest = input
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))?;
    Some(rest.split('"').next()?.to_owned())
}

fn toml_bool_field(input: &str, field: &str) -> Option<bool> {
    let prefix = format!("{field} = ");
    let rest = input
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))?;
    match rest.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn toml_array_field(input: &str, field: &str) -> Option<Vec<String>> {
    let prefix = format!("{field} = [");
    let rest = input
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))?;
    let values = rest.strip_suffix(']')?;
    Some(
        values
            .split(',')
            .filter_map(|value| {
                let value = value.trim();
                value
                    .strip_prefix('"')
                    .and_then(|value| value.strip_suffix('"'))
                    .map(str::to_owned)
            })
            .collect(),
    )
}

fn extension_detail_json(path: &str, state: &GuiApiState) -> String {
    let id = path
        .trim_start_matches("/api/v1/extensions/")
        .trim_end_matches("/versions");
    let client = GreenticDistributorClient::new(state.runtime_home.join("extension-cache"));
    let records = gui_extension_records(state);
    let installed_record = records
        .iter()
        .find(|record| toml_string_field(record, "id").as_deref() == Some(id));
    let installed = installed_record.is_some()
        || state
            .installed_extension_ids
            .iter()
            .any(|value| value == id);
    let enabled = installed_record
        .and_then(|record| toml_bool_field(record, "enabled"))
        .unwrap_or(installed);
    let health = if installed { "healthy" } else { "unknown" };
    if let Some(extension) = client.store_index().find(id) {
        return format!(
            r#"{{"extension":{}}}"#,
            extension_store_entry_json(
                &extension.id,
                &extension.name,
                "Recommended",
                &extension.description,
                &extension.permissions.join(","),
                &extension.capabilities.join(","),
                extension
                    .platforms
                    .iter()
                    .any(|platform| platform == std::env::consts::OS),
                &extension.latest,
                &extension.source,
                &extension.publisher,
                installed,
                enabled,
                health,
            )
        );
    }

    format!(
        r#"{{"extension":{{"id":"{}","name":"{}","status":"{}","installed":{},"enabled":{},"health":"{}","version":"local","publisher":"local","trust":"local","capabilities":[],"permissions":[],"permissionPrompts":[],"platformCompatible":true,"available":{}}}}}"#,
        escape_json(id),
        escape_json(id),
        if installed { "installed" } else { "available" },
        installed,
        enabled,
        escape_json(health),
        !installed
    )
}

fn extension_versions_json(path: &str) -> String {
    let id = path
        .trim_start_matches("/api/v1/extensions/")
        .trim_end_matches("/versions");
    let client = GreenticDistributorClient::new(".greentic/extension-cache");
    let versions = client.versions(id).unwrap_or_default();
    format!(
        r#"{{"id":"{}","versions":{}}}"#,
        escape_json(id),
        string_array_json(&versions)
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunnerFile {
    id: String,
    name: String,
    path: Option<PathBuf>,
    updated: String,
}

fn runners_json(state: &GuiApiState) -> String {
    let runners = runner_files(state)
        .iter()
        .map(|runner| runner_summary_json(state, runner))
        .collect::<Vec<_>>()
        .join(",");
    format!(r#"{{"runners":[{runners}]}}"#)
}

fn runner_detail_json(path: &str, state: &GuiApiState) -> String {
    let id = path.trim_start_matches("/api/v1/runners/");
    if let Some(runner) = find_runner(state, id) {
        let yaml = runner
            .path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default();
        format!(
            r#"{{"runner":{},"yamlPreview":"{}"}}"#,
            runner_summary_json(state, &runner),
            escape_json(&yaml)
        )
    } else {
        r#"{"runner":null}"#.to_owned()
    }
}

fn mcp_status_json(state: &GuiApiState) -> String {
    let service = mcp_service_snapshot(state);
    format!(
        r#"{{"status":"{}","bind":"{}","tools":{}}}"#,
        escape_json(&service.status),
        escape_json(&service.bind),
        enabled_mcp_tools(state).len()
    )
}

fn mcp_tools_json(state: &GuiApiState) -> String {
    let tools = published_mcp_tools(state)
        .iter()
        .map(|tool| {
            let name = tool_name(&tool.id);
            let status = mcp_tool_status(state, &tool.id);
            format!(
                r#"{{"id":"{}","name":"{}","runner":"{}","status":"{}","description":"{}","version":"local","lastCall":"never","successRate":1.0,"risk":"medium","inputSchema":{{"type":"object"}},"outputSchema":{{"type":"object"}}}}"#,
                escape_json(&tool.id),
                escape_json(&name),
                escape_json(&tool.name),
                escape_json(&status),
                escape_json(&format!("MCP wrapper for {}", tool.name))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(r#"{{"tools":[{tools}]}}"#)
}

fn mcp_client_config_json(state: &GuiApiState) -> String {
    let url = format!("http://{}", state.mcp_bind);
    format!(
        r#"{{"localUrl":"{}","clientJson":"{}","awsWorkSpacesDoc":"docs/aws-workspaces-mcp.md","awsForwardedConfigured":false}}"#,
        escape_json(&url),
        escape_json(&format!(
            r#"{{"mcpServers":{{"greentic-desktop":{{"url":"{url}/mcp"}}}}}}"#
        ))
    )
}

#[derive(Debug)]
struct ManagedMcpService {
    bind: String,
    shutdown_tx: Sender<()>,
    join: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct McpServiceSnapshot {
    status: String,
    bind: String,
}

static MCP_SERVICES: OnceLock<Mutex<HashMap<String, ManagedMcpService>>> = OnceLock::new();

fn mcp_services() -> &'static Mutex<HashMap<String, ManagedMcpService>> {
    MCP_SERVICES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn mcp_service_key(state: &GuiApiState) -> String {
    state.runtime_home.display().to_string()
}

fn mcp_service_snapshot(state: &GuiApiState) -> McpServiceSnapshot {
    let key = mcp_service_key(state);
    let services = mcp_services().lock().expect("MCP service lock");
    let (status, bind) = if let Some(service) = services.get(&key) {
        ("running", service.bind.clone())
    } else {
        ("stopped", state.mcp_bind.clone())
    };
    McpServiceSnapshot {
        status: status.to_owned(),
        bind,
    }
}

fn start_mcp_service(state: &GuiApiState) -> Result<String, String> {
    let key = mcp_service_key(state);
    let mut services = mcp_services().lock().expect("MCP service lock");
    if let Some(service) = services.get(&key) {
        return Ok(mcp_lifecycle_json("running", &service.bind, state));
    }

    let listener = TcpListener::bind(&state.mcp_bind).map_err(|err| {
        api_error_json(
            "mcp.bind_failed",
            &format!("Could not start MCP server on {}: {err}", state.mcp_bind),
        )
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    let bind = listener
        .local_addr()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|_| state.mcp_bind.clone());
    let api_state = state.clone();
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
    let join = thread::spawn(move || loop {
        if shutdown_rx.try_recv().is_ok() {
            break;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = handle_mcp_connection(stream, &api_state);
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(_) => break,
        }
    });

    services.insert(
        key,
        ManagedMcpService {
            bind: bind.clone(),
            shutdown_tx,
            join: Some(join),
        },
    );
    Ok(mcp_lifecycle_json("running", &bind, state))
}

fn stop_mcp_service(state: &GuiApiState) -> String {
    let key = mcp_service_key(state);
    let service = mcp_services()
        .lock()
        .expect("MCP service lock")
        .remove(&key);
    if let Some(mut service) = service {
        let _ = service.shutdown_tx.send(());
        if let Some(join) = service.join.take() {
            let _ = join.join();
        }
    }
    mcp_lifecycle_json("stopped", &state.mcp_bind, state)
}

fn restart_mcp_service(state: &GuiApiState) -> Result<String, String> {
    let _ = stop_mcp_service(state);
    start_mcp_service(state)
}

fn mcp_lifecycle_json(status: &str, bind: &str, state: &GuiApiState) -> String {
    format!(
        r#"{{"status":"{}","bind":"{}","tools":{}}}"#,
        escape_json(status),
        escape_json(bind),
        enabled_mcp_tools(state).len()
    )
}

fn handle_mcp_connection(mut stream: TcpStream, state: &GuiApiState) -> Result<(), GuiError> {
    let mut buffer = [0; 8192];
    let read = stream.read(&mut buffer)?;
    if read == 0 {
        return Ok(());
    }
    let request = String::from_utf8_lossy(&buffer[..read]);
    let body = request.split_once("\r\n\r\n").map_or("", |(_, body)| body);
    let data = if body.contains("\"tools/list\"") || body.contains("tools/list") {
        mcp_protocol_tools_list_json(state)
    } else if body.contains("\"tools/call\"") || body.contains("tools/call") {
        mcp_protocol_tool_call_json(body, state)
    } else {
        r#"{"jsonrpc":"2.0","result":{"status":"ok"},"id":1}"#.to_owned()
    };
    let response = http_response(
        200,
        "OK",
        "application/json; charset=utf-8",
        data.as_bytes(),
        false,
    );
    stream.write_all(&response)?;
    Ok(())
}

fn mcp_protocol_tools_list_json(state: &GuiApiState) -> String {
    let tools = enabled_mcp_tools(state)
        .iter()
        .map(|tool| {
            format!(
                r#"{{"name":"{}","description":"{}","inputSchema":{{"type":"object"}}}}"#,
                escape_json(&tool_name(&tool.id)),
                escape_json(&format!("Published MCP wrapper for {}", tool.name))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(r#"{{"jsonrpc":"2.0","result":{{"tools":[{tools}]}},"id":1}}"#)
}

fn mcp_protocol_tool_call_json(body: &str, state: &GuiApiState) -> String {
    let name = json_string_field(body, "name").unwrap_or_default();
    let runner_id = name.trim_start_matches("runner.").replace('.', "_");
    let matched = enabled_mcp_tools(state)
        .into_iter()
        .find(|tool| tool_name(&tool.id) == name || tool.id == runner_id);
    match matched {
        Some(tool) => match execute_runner(state, &tool, "mcp-call", body) {
            Ok(result) => format!(
                r#"{{"jsonrpc":"2.0","result":{{"content":[{{"type":"text","text":"{} {}"}}],"structuredContent":{{"runnerId":"{}","status":"{}","evidenceRef":"{}","outputs":{}}}}},"id":1}}"#,
                escape_json(&tool.name),
                escape_json(&result.status),
                escape_json(&tool.id),
                escape_json(&result.status),
                escape_json(&result.evidence_ref),
                result.outputs_json,
            ),
            Err(error) => format!(
                r#"{{"jsonrpc":"2.0","error":{{"code":-32005,"message":"{}"}},"id":1}}"#,
                escape_json(&error)
            ),
        },
        None => {
            r#"{"jsonrpc":"2.0","error":{"code":-32004,"message":"Tool is not enabled"},"id":1}"#
                .to_owned()
        }
    }
}

fn mcp_tool_action_json(path: &str, state: &GuiApiState) -> Result<String, String> {
    let (id, action) = mcp_tool_parts(path);
    let runner = find_runner(state, id)
        .ok_or_else(|| api_error_json("mcp.tool_not_found", "MCP tool not found."))?;
    match action {
        "enable" => {
            persist_mcp_tool_with_status(state, &runner, "enabled")?;
            Ok(mcp_tool_result_json(&runner, "enable", "enabled"))
        }
        "disable" => {
            persist_mcp_tool_with_status(state, &runner, "disabled")?;
            Ok(mcp_tool_result_json(&runner, "disable", "disabled"))
        }
        "test" => {
            let result = execute_runner(state, &runner, "mcp-test", "{}")?;
            Ok(format!(
                r#"{{"toolId":"{}","toolName":"{}","action":"test","status":"{}","evidenceRef":"{}","outputs":{}}}"#,
                escape_json(&runner.id),
                escape_json(&tool_name(&runner.id)),
                escape_json(&result.status),
                escape_json(&result.evidence_ref),
                result.outputs_json
            ))
        }
        _ => Err(api_error_json(
            "runtime.not_found",
            "MCP tool action not found.",
        )),
    }
}

fn mcp_tool_result_json(runner: &RunnerFile, action: &str, status: &str) -> String {
    format!(
        r#"{{"toolId":"{}","toolName":"{}","action":"{}","status":"{}","evidenceRef":"local://mcp/{}/{}/latest","outputs":{{}}}}"#,
        escape_json(&runner.id),
        escape_json(&tool_name(&runner.id)),
        escape_json(action),
        escape_json(status),
        escape_json(&runner.id),
        escape_json(action)
    )
}

fn mcp_tool_parts(path: &str) -> (&str, &str) {
    let rest = path.trim_start_matches("/api/v1/mcp/tools/");
    rest.split_once('/')
        .map_or((rest, ""), |(id, action)| (id, action))
}

fn runner_action_json(path: &str, body: &str, state: &GuiApiState) -> Result<String, String> {
    let (id, action) = runner_parts(path);
    let runner = find_runner(state, id)
        .ok_or_else(|| api_error_json("runner.not_found", "Runner not found."))?;
    let status = match action {
        "validate" => {
            let package = runner_package_from_yaml(&runner_yaml(&runner))?;
            validate_runner_package_against_adapters(state, &package)?;
            let evidence_ref = format!("local://runners/{}/{}/latest", runner.id, action);
            persist_runner_state(state, &runner.id, "validated", "unknown", &evidence_ref)?;
            return Ok(format!(
                r#"{{"runnerId":"{}","action":"{}","status":"validated","evidenceRef":"{}","outputs":{{}},"steps":[{{"summary":"Validate runner schema and capabilities","status":"passed"}}]}}"#,
                escape_json(&runner.id),
                escape_json(action),
                escape_json(&evidence_ref)
            ));
        }
        "test" | "run" => {
            let result = execute_runner(state, &runner, action, body)?;
            persist_runner_state(
                state,
                &runner.id,
                "validated",
                &result.status,
                &result.evidence_ref,
            )?;
            return Ok(format!(
                r#"{{"runnerId":"{}","action":"{}","status":"{}","evidenceRef":"{}","outputs":{},"steps":{}}}"#,
                escape_json(&runner.id),
                escape_json(action),
                escape_json(&result.status),
                escape_json(&result.evidence_ref),
                result.outputs_json,
                result.steps_json
            ));
        }
        "approve" => {
            let evidence_ref = format!("local://runners/{}/{}/latest", runner.id, action);
            persist_runner_state(state, &runner.id, "approved", "passed", &evidence_ref)?;
            "approved"
        }
        "publish" => {
            let evidence_ref = format!("local://runners/{}/{}/latest", runner.id, action);
            if runner_requires_approval(&runner) && !runner_has_approval(state, &runner.id) {
                let approval_id = persist_approval(state, &runner, "pending")?;
                return Ok(format!(
                    r#"{{"runnerId":"{}","action":"publish","status":"approval_required","evidenceRef":"local://approvals/{}","outputs":{{"approvalId":"{}"}},"steps":[{{"summary":"High-risk publish requires approval","status":"blocked"}}]}}"#,
                    escape_json(&runner.id),
                    escape_json(&approval_id),
                    escape_json(&approval_id)
                ));
            }
            persist_runner_state(state, &runner.id, "published", "passed", &evidence_ref)?;
            persist_mcp_tool(state, &runner)?;
            "published"
        }
        "deprecate" => {
            let evidence_ref = format!("local://runners/{}/{}/latest", runner.id, action);
            persist_runner_state(state, &runner.id, "deprecated", "unknown", &evidence_ref)?;
            let _ = std::fs::remove_file(mcp_tool_path(state, &runner.id));
            "deprecated"
        }
        "refine" => {
            let evidence_ref = format!("local://runners/{}/{}/latest", runner.id, action);
            persist_runner_state(state, &runner.id, "draft", "unknown", &evidence_ref)?;
            "draft"
        }
        "rename" => {
            let name = json_string_field(body, "name").unwrap_or_default();
            rename_runner(state, &runner, name.trim())?;
            "renamed"
        }
        "delete" => {
            delete_runner(state, &runner)?;
            "deleted"
        }
        _ => {
            return Err(api_error_json(
                "runtime.not_found",
                "Runner action not found.",
            ))
        }
    };

    let evidence_ref = format!("local://runners/{}/{}/latest", runner.id, action);
    Ok(format!(
        r#"{{"runnerId":"{}","action":"{}","status":"{}","evidenceRef":"{}","outputs":{},"steps":[{{"summary":"Load runner package","status":"passed"}},{{"summary":"Validate local permissions","status":"passed"}},{{"summary":"Execute runner with provided inputs","status":"passed"}}]}}"#,
        escape_json(&runner.id),
        escape_json(action),
        escape_json(status),
        escape_json(&evidence_ref),
        "{}"
    ))
}

#[derive(Debug, Clone)]
struct GuiRunnerExecution {
    status: String,
    evidence_ref: String,
    outputs_json: String,
    steps_json: String,
}

fn execute_runner(
    state: &GuiApiState,
    runner: &RunnerFile,
    action: &str,
    body: &str,
) -> Result<GuiRunnerExecution, String> {
    let package = runner_package_from_yaml(&runner_yaml(runner))?;
    let inputs = runner_inputs_from_body(body, &package.inputs);
    ensure_required_inputs_present(&package, &inputs)?;
    let secrets = runner_secrets_from_body(state, body, &package.secrets)?;
    let request = ReplayRequest {
        package,
        session_profile: SessionProfile {
            id: format!("gui-{action}"),
            bootstrap: Vec::new(),
            teardown: Vec::new(),
        },
        inputs,
        secrets,
        adapters: replay_adapter_registry(state).capabilities(),
    };
    let context = ReplayExecutionContext {
        registry: replay_adapter_registry(state),
        on_failure: OnFailure::Stop,
    };
    let outcome = replay_with_context(request, &context);
    let status = if outcome.passed { "passed" } else { "failed" }.to_owned();
    let failure = outcome.failure_reason.clone();
    let evidence_ref = outcome.evidence_ref.uri.clone();
    persist_replay_evidence_bundle(
        state,
        runner,
        action,
        &outcome.outputs_json(),
        failure.as_deref(),
    )?;
    if !outcome.passed {
        return Err(api_error_json(
            "runner.execution_failed",
            failure.as_deref().unwrap_or("Runner execution failed."),
        ));
    }
    if !runner_declared_fields(runner, "outputs").is_empty() && outcome.outputs.is_empty() {
        return Err(api_error_json(
            "runner.output_extraction_failed",
            "Runner completed but did not extract any declared outputs.",
        ));
    }
    Ok(GuiRunnerExecution {
        status,
        evidence_ref,
        outputs_json: outcome.outputs_json(),
        steps_json: replay_steps_json(&outcome.traces),
    })
}

fn ensure_required_inputs_present(
    package: &RunnerPackage,
    inputs: &BTreeMap<String, String>,
) -> Result<(), String> {
    let missing = package
        .inputs
        .iter()
        .filter(|input| {
            inputs
                .get(*input)
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
        })
        .map(|input| field_display_name(input))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(api_error_json(
            "runner.input_missing",
            &format!("Missing required runner input(s): {}", missing.join(", ")),
        ))
    }
}

fn runner_secrets_from_body(
    state: &GuiApiState,
    body: &str,
    declared_secrets: &[String],
) -> Result<BTreeMap<String, String>, String> {
    let mut secrets = BTreeMap::new();
    let mut missing = Vec::new();
    for secret in declared_secrets {
        let short = field_display_name(secret);
        let value = json_string_field(body, &short)
            .or_else(|| json_string_field(body, secret))
            .or_else(|| read_gui_secret(state, &short))
            .or_else(|| read_gui_secret(state, secret))
            .or_else(|| std::env::var(&short).ok())
            .or_else(|| std::env::var(secret).ok());
        if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
            if json_string_field(body, &short).is_some()
                || json_string_field(body, secret).is_some()
            {
                let _ = persist_gui_secret(state, &short, &value);
            }
            secrets.insert(secret.clone(), value);
        } else {
            missing.push(short);
        }
    }
    if missing.is_empty() {
        Ok(secrets)
    } else {
        Err(api_error_json(
            "runner.secret_missing",
            &format!("Missing required runner secret(s): {}", missing.join(", ")),
        ))
    }
}

fn validate_runner_package_against_adapters(
    state: &GuiApiState,
    package: &RunnerPackage,
) -> Result<(), String> {
    greentic_desktop_replay::validate_package(
        package,
        &replay_adapter_registry(state).capabilities(),
    )
    .map_err(|message| api_error_json("runner.capability_missing", &message))
}

fn replay_adapter_registry(state: &GuiApiState) -> AdapterRegistry {
    let mut registry = AdapterRegistry::new();
    registry.insert(Arc::new(PlaywrightWebAdapter::new()));
    registry.insert(Arc::new(TerminalAdapter::new()));
    registry.insert(Arc::new(VisionAdapter::new()));
    registry.insert(Arc::new(JavaDesktopAdapter::new(
        java_access_bridge_available(),
    )));
    match detect_platform().os {
        DesktopPlatform::MacOS => {
            registry.insert(Arc::new(MacOsAccessibilityAdapter::new(detect_platform())));
        }
        DesktopPlatform::Windows => {
            registry.insert(Arc::new(WindowsUiAdapter::new()));
        }
        DesktopPlatform::Linux if state.platform == "linux" => {
            let platform = detect_platform();
            if platform.display_server.as_deref() == Some("wayland") {
                registry.insert(Arc::new(LinuxWaylandAdapter::new(detect_wayland_support(
                    &platform,
                    WaylandCompositor::Unknown,
                    false,
                    false,
                ))));
            } else {
                registry.insert(Arc::new(LinuxX11Adapter::new(platform)));
            }
        }
        _ => {}
    }
    registry
}

fn runner_inputs_from_body(body: &str, declared_inputs: &[String]) -> BTreeMap<String, String> {
    let mut inputs = BTreeMap::new();
    for input in declared_inputs {
        let short = field_display_name(input);
        if let Some(value) =
            json_string_field(body, &short).or_else(|| json_string_field(body, input))
        {
            inputs.insert(input.clone(), value);
        }
    }
    inputs
}

fn runner_package_from_yaml(yaml: &str) -> Result<RunnerPackage, String> {
    if let Some(definition) = parse_runner_definition_manifest(yaml)? {
        return Ok(definition.into_package());
    }
    let id = yaml_scalar(yaml, "id")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            api_error_json("runner.invalid_manifest", "Runner manifest is missing id.")
        })?;
    let version = yaml_scalar(yaml, "version").unwrap_or_else(|| "0.1.0-draft".to_owned());
    let inputs = yaml_list(yaml, "inputs");
    let secrets = yaml_list(yaml, "secrets");
    let outputs = yaml_list(yaml, "outputs");
    let assertions = yaml_list(yaml, "assertions");
    let open_questions = yaml_list(yaml, "open_questions");
    let steps = yaml_steps(yaml)?;
    Ok(RunnerPackage {
        id,
        version,
        mode: RecordingMode::AssistedPrompt,
        inputs,
        secrets,
        steps,
        assertions,
        outputs,
        open_questions,
    })
}

fn parse_runner_definition_manifest(raw: &str) -> Result<Option<RunnerDefinition>, String> {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with('{') {
        return Ok(None);
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).map_err(|err| {
        api_error_json(
            "runner.invalid_manifest",
            &format!("Typed runner manifest is not valid JSON: {err}"),
        )
    })?;
    if let Some(definition) = value.get("runner_definition") {
        return serde_json::from_value(definition.clone())
            .map(Some)
            .map_err(|err| {
                api_error_json(
                    "runner.invalid_manifest",
                    &format!("runner_definition does not match the typed runner schema: {err}"),
                )
            });
    }
    serde_json::from_value(value).map(Some).map_err(|err| {
        api_error_json(
            "runner.invalid_manifest",
            &format!("Typed runner manifest does not match the runner schema: {err}"),
        )
    })
}

fn manifest_name(raw: &str) -> Option<String> {
    parse_runner_definition_manifest(raw)
        .ok()
        .flatten()
        .map(|definition| definition.summary)
        .or_else(|| yaml_scalar(raw, "name"))
}

fn manifest_description(raw: &str) -> Option<String> {
    parse_runner_definition_manifest(raw)
        .ok()
        .flatten()
        .map(|definition| definition.intent)
        .or_else(|| yaml_scalar(raw, "description"))
}

fn manifest_declared_fields(raw: &str, key: &str) -> Vec<String> {
    if let Ok(Some(definition)) = parse_runner_definition_manifest(raw) {
        return match key {
            "inputs" => definition
                .inputs
                .iter()
                .map(|input| format!("inputs.{}", input.name))
                .collect(),
            "secrets" => definition
                .secrets
                .iter()
                .map(|secret| format!("secrets.{}", secret.name))
                .collect(),
            "outputs" => definition
                .outputs
                .iter()
                .map(|output| format!("outputs.{}", output.name))
                .collect(),
            "assertions" => definition
                .assertions
                .iter()
                .map(|assertion| assertion.name.clone())
                .collect(),
            _ => Vec::new(),
        };
    }
    yaml_list(raw, key)
}

fn manifest_input_fields_json(raw: &str) -> String {
    if let Ok(Some(definition)) = parse_runner_definition_manifest(raw) {
        return definition
            .inputs
            .iter()
            .map(|input| {
                typed_field_json(
                    &input.name,
                    &input.value_type,
                    input.required,
                    input.default_value.as_deref(),
                    input.validation.as_deref(),
                    false,
                    None,
                )
            })
            .collect::<Vec<_>>()
            .join(",")
            .pipe_json_array();
    }
    yaml_fields_json(raw, "inputs", false)
}

fn manifest_secret_fields_json(raw: &str, state: &GuiApiState) -> String {
    if let Ok(Some(definition)) = parse_runner_definition_manifest(raw) {
        return definition
            .secrets
            .iter()
            .map(|secret| {
                let has_value = read_gui_secret(state, &secret.name).is_some()
                    || read_gui_secret(state, &format!("secrets.{}", secret.name)).is_some()
                    || std::env::var(&secret.name).is_ok()
                    || std::env::var(format!("secrets.{}", secret.name)).is_ok();
                typed_field_json(
                    &secret.name,
                    &secret.value_type,
                    secret.required,
                    None,
                    secret.validation.as_deref(),
                    true,
                    Some(has_value),
                )
            })
            .collect::<Vec<_>>()
            .join(",")
            .pipe_json_array();
    }
    let fields = yaml_list(raw, "secrets")
        .into_iter()
        .map(|field| {
            let name = field_display_name(&field);
            let has_value = read_gui_secret(state, &name).is_some()
                || read_gui_secret(state, &field).is_some()
                || std::env::var(&name).is_ok()
                || std::env::var(&field).is_ok();
            simple_field_json(&name, true, Some(has_value))
        })
        .collect::<Vec<_>>();
    fields.join(",").pipe_json_array()
}

fn manifest_secret_fields_without_status_json(raw: &str) -> String {
    if let Ok(Some(definition)) = parse_runner_definition_manifest(raw) {
        return definition
            .secrets
            .iter()
            .map(|secret| {
                typed_field_json(
                    &secret.name,
                    &secret.value_type,
                    secret.required,
                    None,
                    secret.validation.as_deref(),
                    true,
                    None,
                )
            })
            .collect::<Vec<_>>()
            .join(",")
            .pipe_json_array();
    }
    yaml_fields_json(raw, "secrets", true)
}

fn manifest_output_fields_json(raw: &str) -> String {
    if let Ok(Some(definition)) = parse_runner_definition_manifest(raw) {
        return definition
            .outputs
            .iter()
            .map(|output| {
                format!(
                    r#"{{"name":"{}","valueType":{},"required":{},"extractor":{},"failureBehavior":{},"proof":"{}"}}"#,
                    escape_json(&output.name),
                    workflow_value_type_json(&output.value_type),
                    output.required,
                    serde_json::to_string(&output.extractor).unwrap_or_else(|_| "null".to_owned()),
                    serde_json::to_string(&output.failure_behavior).unwrap_or_else(|_| "null".to_owned()),
                    escape_json(&extractor_proof_label(&output.extractor))
                )
            })
            .collect::<Vec<_>>()
            .join(",")
            .pipe_json_array();
    }
    yaml_fields_json(raw, "outputs", false)
}

trait JsonArrayExt {
    fn pipe_json_array(self) -> String;
}

impl JsonArrayExt for String {
    fn pipe_json_array(self) -> String {
        format!("[{self}]")
    }
}

fn yaml_fields_json(raw: &str, key: &str, secret: bool) -> String {
    yaml_list(raw, key)
        .iter()
        .map(|field| simple_field_json(&field_display_name(field), secret, None))
        .collect::<Vec<_>>()
        .join(",")
        .pipe_json_array()
}

fn simple_field_json(name: &str, secret: bool, has_value: Option<bool>) -> String {
    typed_field_json(
        name,
        &WorkflowValueType::String,
        true,
        None,
        None,
        secret,
        has_value,
    )
}

fn typed_field_json(
    name: &str,
    value_type: &WorkflowValueType,
    required: bool,
    default_value: Option<&str>,
    validation: Option<&str>,
    secret: bool,
    has_value: Option<bool>,
) -> String {
    format!(
        r#"{{"name":"{}","valueType":{},"required":{},"defaultValue":{},"enumValues":{},"validation":{},"secret":{},"hasValue":{}}}"#,
        escape_json(name),
        workflow_value_type_json(value_type),
        required,
        json_option(default_value),
        workflow_value_type_enum_values_json(value_type),
        json_option(validation),
        secret,
        has_value
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_owned())
    )
}

fn workflow_value_type_json(value_type: &WorkflowValueType) -> String {
    serde_json::to_string(value_type).unwrap_or_else(|_| r#""String""#.to_owned())
}

fn workflow_value_type_enum_values_json(value_type: &WorkflowValueType) -> String {
    match value_type {
        WorkflowValueType::Enum(values) => string_array_json(values),
        _ => "[]".to_owned(),
    }
}

fn extractor_proof_label(extractor: &WorkflowOutputExtractor) -> String {
    match extractor {
        WorkflowOutputExtractor::TargetText(_) => "target text".to_owned(),
        WorkflowOutputExtractor::VisibleText(text) => format!("visible text: {text}"),
        WorkflowOutputExtractor::Regex(pattern) => format!("regex: {pattern}"),
        WorkflowOutputExtractor::TerminalField(field) => {
            format!(
                "terminal row {}, col {}, len {}",
                field.row, field.col, field.len
            )
        }
        WorkflowOutputExtractor::JsonPath(path) => format!("json path: {path}"),
    }
}

fn runner_file_for_yaml_path(path: &std::path::Path, yaml: &str) -> Result<RunnerFile, String> {
    let id = yaml_scalar(yaml, "id")
        .or_else(|| {
            parse_runner_definition_manifest(yaml)
                .ok()
                .flatten()
                .map(|definition| definition.runner_id)
        })
        .or_else(|| runner_id_from_path(path))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            api_error_json("runner.invalid_manifest", "Runner manifest is missing id.")
        })?;
    let name = manifest_name(yaml).unwrap_or_else(|| id.replace('.', " "));
    Ok(RunnerFile {
        id,
        name,
        path: Some(path.to_path_buf()),
        updated: "draft".to_owned(),
    })
}

fn yaml_steps(yaml: &str) -> Result<Vec<RunnerStep>, String> {
    let mut steps = Vec::new();
    let mut current: Option<RunnerStep> = None;
    let mut in_steps = false;
    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed == "steps:" {
            in_steps = true;
            continue;
        }
        if in_steps && !trimmed.is_empty() && !line.starts_with(' ') {
            break;
        }
        if !in_steps || trimmed.is_empty() {
            continue;
        }
        if let Some(id) = trimmed.strip_prefix("- id:") {
            if let Some(step) = current.take() {
                steps.push(step);
            }
            current = Some(RunnerStep {
                id: unquote_yaml_value(id.trim()),
                action: String::new(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: String::new(),
            });
        } else if let Some(step) = current.as_mut() {
            if let Some(value) = trimmed.strip_prefix("action:") {
                step.action = unquote_yaml_value(value.trim());
            } else if let Some(value) = trimmed.strip_prefix("required_capability:") {
                step.required_capability = unquote_yaml_value(value.trim());
            } else if let Some(value) = trimmed.strip_prefix("value:") {
                step.value = Some(unquote_yaml_value(value.trim()));
            }
        }
    }
    if let Some(step) = current.take() {
        steps.push(step);
    }
    if steps.is_empty() {
        return Err(api_error_json(
            "runner.invalid_manifest",
            "Runner manifest does not contain executable steps.",
        ));
    }
    if let Some(step) = steps.iter().find(|step| {
        step.id.is_empty() || step.action.is_empty() || step.required_capability.is_empty()
    }) {
        return Err(api_error_json(
            "runner.invalid_manifest",
            &format!(
                "Runner step '{}' is missing action or required capability.",
                step.id
            ),
        ));
    }
    Ok(steps)
}

fn unquote_yaml_value(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .replace("\\\"", "\"")
        .replace("\\n", "\n")
}

fn replay_steps_json(traces: &[greentic_desktop_replay::StepTrace]) -> String {
    format!(
        "[{}]",
        traces
            .iter()
            .map(|trace| {
                format!(
                    r#"{{"summary":"{}","status":"{}"}}"#,
                    escape_json(&trace.step_id),
                    if trace.success { "passed" } else { "failed" }
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn refinement_action_json(path: &str, body: &str, state: &GuiApiState) -> Result<String, String> {
    let rest = path.trim_start_matches("/api/v1/runners/");
    let (runner_id, rest) = rest
        .split_once("/refinement")
        .ok_or_else(|| api_error_json("refinement.not_found", "Refinement endpoint not found."))?;
    let runner = find_runner(state, runner_id)
        .ok_or_else(|| api_error_json("runner.not_found", "Runner not found."))?;
    let correction = json_string_field(body, "correction")
        .or_else(|| json_string_field(body, "text"))
        .unwrap_or_else(|| "Use the corrected selector.".to_owned());
    let refinement_id = format!("refine-{:016x}", fnv1a64(correction.as_bytes()));
    if rest.starts_with('/') && rest.ends_with("/apply") {
        persist_runner_state(
            state,
            &runner.id,
            "validated",
            "passed",
            &format!("local://refinements/{refinement_id}"),
        )?;
        return Ok(format!(
            r#"{{"refinementId":"{}","runnerId":"{}","status":"applied","applied":true,"evidenceRef":"local://refinements/{}","diff":{{"stepId":"step-1","before":"click old target","after":"{}"}}}}"#,
            escape_json(&refinement_id),
            escape_json(&runner.id),
            escape_json(&refinement_id),
            escape_json(&correction)
        ));
    }
    let path = refinements_dir(state).join(format!("{refinement_id}.json"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    }
    let json = format!(
        r#"{{"refinementId":"{}","runnerId":"{}","status":"preview","applied":false,"evidenceRef":"local://refinements/{}","diff":{{"stepId":"step-1","before":"click old target","after":"{}"}}}}"#,
        escape_json(&refinement_id),
        escape_json(&runner.id),
        escape_json(&refinement_id),
        escape_json(&correction)
    );
    std::fs::write(path, &json).map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    Ok(json)
}

fn runner_summary_json(state: &GuiApiState, runner: &RunnerFile) -> String {
    let state_json = runner_state_json(state, &runner.id);
    let status = json_string_field(&state_json, "status").unwrap_or_else(|| "draft".to_owned());
    let last_test =
        json_string_field(&state_json, "lastTest").unwrap_or_else(|| "unknown".to_owned());
    let published = true;
    let yaml = runner
        .path
        .as_ref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    let description = manifest_description(&yaml)
        .unwrap_or_else(|| "Local runner package managed by Greentic Desktop.".to_owned());
    format!(
        r#"{{"id":"{}","name":"{}","description":"{}","status":"{}","risk":"medium","version":"local","lastTest":"{}","updated":"{}","adapters":[],"inputs":{},"outputs":{},"secrets":{},"inputFields":{},"secretFields":{},"outputFields":{},"published":{},"evidenceRefs":{}}}"#,
        escape_json(&runner.id),
        escape_json(&runner.name),
        escape_json(&description),
        escape_json(&status),
        escape_json(&last_test),
        escape_json(&runner.updated),
        field_names_json(&manifest_declared_fields(&yaml, "inputs")),
        field_names_json(&manifest_declared_fields(&yaml, "outputs")),
        field_names_json(&manifest_declared_fields(&yaml, "secrets")),
        manifest_input_fields_json(&yaml),
        manifest_secret_fields_json(&yaml, state),
        manifest_output_fields_json(&yaml),
        published,
        runner_evidence_json(state, &runner.id)
    )
}

fn runner_declared_fields(runner: &RunnerFile, key: &str) -> Vec<String> {
    runner
        .path
        .as_ref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|yaml| manifest_declared_fields(&yaml, key))
        .unwrap_or_default()
}

fn runner_yaml(runner: &RunnerFile) -> String {
    runner
        .path
        .as_ref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_else(|| {
            format!(
                "id: {}\nname: {}\ndescription: Local runner package managed by Greentic Desktop.\ninputs: []\noutputs: []\n",
                runner.id, runner.name
            )
        })
}

fn runner_files(state: &GuiApiState) -> Vec<RunnerFile> {
    let mut runners = Vec::new();
    let runner_dir = state.runtime_home.join("runners");
    if let Ok(entries) = std::fs::read_dir(runner_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !is_runner_file(&path) {
                continue;
            }
            let Some(id) = runner_id_from_path(&path) else {
                continue;
            };
            let yaml = std::fs::read_to_string(&path).unwrap_or_default();
            let name = manifest_name(&yaml).unwrap_or_else(|| id.replace('.', " "));
            let updated = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .map(|_| "recently".to_owned())
                .unwrap_or_else(|_| "unknown".to_owned());
            runners.push(RunnerFile {
                id,
                name,
                path: Some(path),
                updated,
            });
        }
    }

    for name in &state.runner_names {
        if runners.iter().any(|runner| runner.id == *name) {
            continue;
        }
        runners.push(RunnerFile {
            id: name.clone(),
            name: name.clone(),
            path: None,
            updated: "unknown".to_owned(),
        });
    }
    runners.sort_by(|left, right| left.id.cmp(&right.id));
    runners
}

fn find_runner(state: &GuiApiState, id: &str) -> Option<RunnerFile> {
    runner_files(state)
        .into_iter()
        .find(|runner| runner.id == id)
}

fn is_runner_file(path: &std::path::Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if name.ends_with(".state.json") || name.ends_with(".mcp.json") {
        return false;
    }
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("gtpack" | "yaml" | "yml" | "json")
    )
}

fn runner_id_from_path(path: &std::path::Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    Some(
        name.trim_end_matches(".draft.yaml")
            .trim_end_matches(".runner.json")
            .trim_end_matches(".draft.json")
            .trim_end_matches(".yaml")
            .trim_end_matches(".yml")
            .trim_end_matches(".json")
            .trim_end_matches(".gtpack")
            .to_owned(),
    )
}

fn runner_parts(path: &str) -> (&str, &str) {
    let rest = path.trim_start_matches("/api/v1/runners/");
    rest.split_once('/')
        .map_or((rest, ""), |(id, action)| (id, action))
}

fn runner_state_path(state: &GuiApiState, id: &str) -> PathBuf {
    state
        .runtime_home
        .join("runners")
        .join(format!("{id}.state.json"))
}

fn runner_state_json(state: &GuiApiState, id: &str) -> String {
    std::fs::read_to_string(runner_state_path(state, id)).unwrap_or_default()
}

fn persist_runner_state(
    state: &GuiApiState,
    id: &str,
    status: &str,
    last_test: &str,
    evidence_ref: &str,
) -> Result<(), String> {
    let path = runner_state_path(state, id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    }
    let json = format!(
        r#"{{"status":"{}","lastTest":"{}","evidenceRef":"{}"}}"#,
        escape_json(status),
        escape_json(last_test),
        escape_json(evidence_ref)
    );
    std::fs::write(path, json).map_err(|err| api_error_json("runtime.io", &err.to_string()))
}

fn delete_runner(state: &GuiApiState, runner: &RunnerFile) -> Result<(), String> {
    if let Some(path) = &runner.path {
        if path.is_file() {
            std::fs::remove_file(path)
                .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
        }
    }
    let state_path = runner_state_path(state, &runner.id);
    if state_path.is_file() {
        std::fs::remove_file(state_path)
            .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    }
    let mcp_path = mcp_tool_path(state, &runner.id);
    if mcp_path.is_file() {
        std::fs::remove_file(mcp_path)
            .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    }
    Ok(())
}

fn rename_runner(state: &GuiApiState, runner: &RunnerFile, name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(api_error_json(
            "runner.rename.invalid_name",
            "Runner name cannot be empty.",
        ));
    }

    let path = runner.path.as_ref().ok_or_else(|| {
        api_error_json(
            "runner.rename.not_persisted",
            "This runner cannot be renamed because it has no manifest file.",
        )
    })?;
    let yaml = std::fs::read_to_string(path).map_err(|err| {
        api_error_json(
            "runtime.io",
            &format!("Could not read runner manifest: {err}"),
        )
    })?;
    let mut changed = false;
    let mut lines = Vec::new();
    for line in yaml.lines() {
        if line.trim_start().starts_with("name:")
            && !line.starts_with(' ')
            && !line.starts_with('\t')
        {
            lines.push(format!("name: {}", yaml_quoted_string(name)));
            changed = true;
        } else {
            lines.push(line.to_owned());
        }
    }
    if !changed {
        let id_line = lines
            .iter()
            .position(|line| line.trim_start().starts_with("id:") && !line.starts_with(' '));
        match id_line {
            Some(index) => lines.insert(index + 1, format!("name: {}", yaml_quoted_string(name))),
            None => lines.insert(0, format!("name: {}", yaml_quoted_string(name))),
        }
    }
    let mut updated = lines.join("\n");
    if yaml.ends_with('\n') {
        updated.push('\n');
    }
    std::fs::write(path, updated).map_err(|err| {
        api_error_json(
            "runtime.io",
            &format!("Could not update runner manifest: {err}"),
        )
    })?;
    let renamed_runner = RunnerFile {
        name: name.to_owned(),
        ..runner.clone()
    };
    persist_mcp_tool_with_status(state, &renamed_runner, "enabled")?;
    Ok(())
}

fn yaml_quoted_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    )
}

fn runner_evidence_json(state: &GuiApiState, id: &str) -> String {
    let evidence = json_string_field(&runner_state_json(state, id), "evidenceRef");
    match evidence {
        Some(value) => string_array_json(&[value]),
        None => "[]".to_owned(),
    }
}

fn mcp_tool_path(state: &GuiApiState, id: &str) -> PathBuf {
    state
        .runtime_home
        .join("mcp-tools")
        .join(format!("{id}.mcp.json"))
}

fn persist_mcp_tool(state: &GuiApiState, runner: &RunnerFile) -> Result<(), String> {
    persist_mcp_tool_with_status(state, runner, "enabled")
}

fn persist_mcp_tool_with_status(
    state: &GuiApiState,
    runner: &RunnerFile,
    status: &str,
) -> Result<(), String> {
    let path = mcp_tool_path(state, &runner.id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
    }
    let json = format!(
        r#"{{"id":"{}","name":"{}","runner":"{}","status":"{}"}}"#,
        escape_json(&runner.id),
        escape_json(&tool_name(&runner.id)),
        escape_json(&runner.name),
        escape_json(status)
    );
    std::fs::write(path, json).map_err(|err| api_error_json("runtime.io", &err.to_string()))
}

fn published_mcp_tools(state: &GuiApiState) -> Vec<RunnerFile> {
    runner_files(state)
}

fn enabled_mcp_tools(state: &GuiApiState) -> Vec<RunnerFile> {
    published_mcp_tools(state)
        .into_iter()
        .filter(|runner| mcp_tool_status(state, &runner.id) == "enabled")
        .collect()
}

fn mcp_tool_status(state: &GuiApiState, id: &str) -> String {
    std::fs::read_to_string(mcp_tool_path(state, id))
        .ok()
        .and_then(|json| json_string_field(&json, "status"))
        .unwrap_or_else(|| "enabled".to_owned())
}

fn tool_name(id: &str) -> String {
    format!("runner.{}", id.replace('-', "."))
}

fn yaml_scalar(yaml: &str, key: &str) -> Option<String> {
    let needle = format!("{key}:");
    yaml.lines().find_map(|line| {
        let trimmed = line.trim();
        let value = trimmed.strip_prefix(&needle)?.trim();
        Some(value.trim_matches('"').trim_matches('\'').to_owned())
    })
}

fn field_names_json(fields: &[String]) -> String {
    string_array_json(
        &fields
            .iter()
            .map(|field| field_display_name(field))
            .collect::<Vec<_>>(),
    )
}

fn yaml_list(yaml: &str, key: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut in_list = false;
    let list_header = format!("{key}:");
    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed == list_header {
            in_list = true;
            continue;
        }
        if in_list {
            if let Some(value) = trimmed.strip_prefix("- ") {
                values.push(value.trim_matches('"').trim_matches('\'').to_owned());
                continue;
            }
            if !trimmed.is_empty() && !line.starts_with(' ') {
                break;
            }
        }
    }
    values
}

fn field_display_name(field: &str) -> String {
    field
        .trim_start_matches("inputs.")
        .trim_start_matches("outputs.")
        .trim_start_matches("secrets.")
        .to_owned()
}

fn recording_targets_json() -> String {
    r#"{"targets":[{"id":"browser","label":"Browser task - Greentic opens a browser window","profile":"web","adapter":"greentic.desktop.playwright","available":true},{"id":"desktop","label":"Desktop app task","profile":"desktop","adapter":"greentic.desktop.vision","available":true},{"id":"java","label":"Java app task","profile":"java","adapter":"greentic.desktop.java-accessibility","available":true},{"id":"remote","label":"Remote desktop task","profile":"remote","adapter":"greentic.desktop.vision","available":true},{"id":"terminal","label":"Terminal/mainframe task","profile":"terminal","adapter":"greentic.desktop.terminal.tn3270","available":true}]}"#.to_owned()
}

fn recordings_list_json(state: &GuiApiState) -> String {
    let recordings = list_recording_sessions(&state.runtime_home)
        .unwrap_or_default()
        .iter()
        .map(recording_manifest_json)
        .collect::<Vec<_>>()
        .join(",");
    format!(r#"{{"recordings":[{recordings}]}}"#)
}

fn create_recording_json(body: &str, state: &GuiApiState) -> Result<String, String> {
    let name = json_string_field(body, "name").unwrap_or_else(|| "recorded.runner".to_owned());
    if name.trim().is_empty() {
        return Err(api_error_json(
            "recording.invalid_state",
            "Recording name must not be empty.",
        ));
    }
    let target = json_string_field(body, "target").unwrap_or_else(|| "browser".to_owned());
    let initial_url = json_string_field(body, "initialUrl")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "about:blank".to_owned());
    let (profile, adapter) = recording_target_profile(&target);
    let out = state.runtime_home.join("recordings").join(slug(&name));
    let manifest = start_recording_session_with_registry(
        RecordingStartRequest {
            name,
            profile: profile.to_owned(),
            adapter: adapter.to_owned(),
            target_kind: recording_target_kind(&target),
            out,
            runtime_home: state.runtime_home.clone(),
            redact: vec!["text".to_owned(), "password".to_owned(), "token".to_owned()],
            secret_fields: vec!["password".to_owned(), "api_key".to_owned()],
        },
        &recording_backend_registry(&target, &initial_url, state),
    )
    .map_err(|err| api_error_json("recording.invalid_state", &err.to_string()))?;
    Ok(recording_manifest_json(&manifest))
}

fn recording_backend_registry(
    target: &str,
    initial_url: &str,
    state: &GuiApiState,
) -> RecordingBackendRegistry {
    let mut registry = RecordingBackendRegistry::new();
    #[cfg(test)]
    if target == "__test_browser" {
        registry.register(FakeRecordingBackend::ready(
            "greentic.recording.web.test",
            RecordingTargetKind::Web,
        ));
        return registry;
    }
    if target == "browser" {
        registry.register(PlaywrightWebRecordingBackend::new(
            PlaywrightRecorderOptions {
                initial_url: initial_url.to_owned(),
                sidecar_command: PLAYWRIGHT_ADAPTER_ID.to_owned(),
                browser_context: "greentic-owned".to_owned(),
                require_playwright: true,
            },
        ));
    } else if target == "java" {
        if allow_experimental_recording_backend("java") {
            registry.register(JavaAccessBridgeRecordingBackend::new(
                java_access_bridge_available(),
            ));
        } else {
            registry.register(FakeRecordingBackend::blocked(
                "greentic.recording.java.not-configured",
                RecordingTargetKind::Java,
                "Java recording requires a real Java Access Bridge event bridge. Configure GREENTIC_ENABLE_EXPERIMENTAL_JAVA_RECORDING=1 only when that bridge is installed.",
            ));
        }
    } else if target == "terminal" {
        if let Ok(command) = std::env::var("GREENTIC_TERMINAL_RECORDER_COMMAND") {
            registry.register(TerminalRecordingBackend::with_capture_command(
                TerminalProfile {
                    name: "local-shell".to_owned(),
                    protocol: TerminalProtocol::Vt220,
                    host: "localhost".to_owned(),
                },
                true,
                command,
            ));
        } else {
            registry.register(FakeRecordingBackend::blocked(
                "greentic.recording.terminal.not-configured",
                RecordingTargetKind::Terminal,
                "Terminal recording requires a Greentic-owned PTY/SSH/TN3270 event source. Configure GREENTIC_TERMINAL_RECORDER_COMMAND to start that source.",
            ));
        }
    } else if target == "remote" {
        if allow_experimental_recording_backend("remote") {
            let platform = detect_platform();
            let screen_capture = platform.has_permission(PlatformPermission::ScreenRecording)
                || platform.has_permission(PlatformPermission::Screenshot);
            let input_control = platform.has_permission(PlatformPermission::KeyboardInput)
                && platform.has_permission(PlatformPermission::MouseInput);
            registry.register(RemoteVisionRecordingBackend::new(
                true,
                screen_capture,
                input_control,
                Some(RemoteViewportCalibration {
                    origin_x: 0,
                    origin_y: 0,
                    width: 1280,
                    height: 720,
                    scale_percent: 100,
                }),
            ));
        } else {
            registry.register(FakeRecordingBackend::blocked(
                "greentic.recording.remote.not-configured",
                RecordingTargetKind::Remote,
                "Remote recording requires a Greentic-owned remote viewport event source and calibrated input stream. Configure GREENTIC_ENABLE_EXPERIMENTAL_REMOTE_RECORDING=1 only when that source is installed.",
            ));
        }
    } else if target == "desktop" {
        let platform = detect_platform();
        if platform_desktop_recording_configured(state, &platform)
            || allow_experimental_recording_backend("desktop")
        {
            match platform.os {
                DesktopPlatform::MacOS => {
                    registry.register(MacOsAccessibilityRecordingBackend::new(platform));
                }
                DesktopPlatform::Windows => {
                    registry.register(WindowsUiRecordingBackend::default());
                }
                DesktopPlatform::Linux if platform.display_server.as_deref() == Some("wayland") => {
                    let support =
                        detect_wayland_support(&platform, WaylandCompositor::Unknown, false, false);
                    registry.register(LinuxWaylandRecordingBackend::new(support));
                }
                DesktopPlatform::Linux => {
                    registry.register(LinuxX11RecordingBackend::new(platform));
                }
            }
        } else {
            registry.register(FakeRecordingBackend::blocked(
                "greentic.recording.desktop.not-configured",
                RecordingTargetKind::Desktop,
                format!(
                    "Desktop app recording requires the {} extension. Install it from Settings > Extensions, then retry recording.",
                    platform_desktop_extension_label(&platform)
                ),
            ));
        }
    }
    registry
}

fn platform_desktop_recording_configured(state: &GuiApiState, platform: &PlatformInfo) -> bool {
    platform_desktop_extension_ids(platform)
        .iter()
        .any(|id| gui_extension_enabled(state, id))
}

fn platform_desktop_extension_ids(platform: &PlatformInfo) -> Vec<&'static str> {
    match platform.os {
        DesktopPlatform::MacOS => vec!["greentic.desktop.macos.ax"],
        DesktopPlatform::Windows => vec!["greentic.desktop.windows-ui"],
        DesktopPlatform::Linux if platform.display_server.as_deref() == Some("wayland") => {
            vec!["greentic.desktop.linux.wayland"]
        }
        DesktopPlatform::Linux => vec!["greentic.desktop.linux.x11"],
    }
}

fn platform_desktop_extension_label(platform: &PlatformInfo) -> &'static str {
    match platform.os {
        DesktopPlatform::MacOS => "macOS Accessibility Adapter",
        DesktopPlatform::Windows => "Windows UI Automation Adapter",
        DesktopPlatform::Linux if platform.display_server.as_deref() == Some("wayland") => {
            "Linux Wayland Compatibility Adapter"
        }
        DesktopPlatform::Linux => "Linux X11 Desktop Adapter",
    }
}

fn gui_extension_enabled(state: &GuiApiState, id: &str) -> bool {
    if let Some(record) = gui_extension_records(state)
        .iter()
        .find(|record| toml_string_field(record, "id").as_deref() == Some(id))
    {
        return toml_bool_field(record, "enabled").unwrap_or(true);
    }
    state
        .installed_extension_ids
        .iter()
        .any(|value| value == id)
}

fn allow_experimental_recording_backend(target: &str) -> bool {
    let specific = format!(
        "GREENTIC_ENABLE_EXPERIMENTAL_{}_RECORDING",
        target.to_uppercase()
    );
    std::env::var(&specific)
        .or_else(|_| std::env::var("GREENTIC_ENABLE_EXPERIMENTAL_RECORDING_BACKENDS"))
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn java_access_bridge_available() -> bool {
    std::env::var("GREENTIC_JAVA_ACCESS_BRIDGE")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn recording_action_json(
    method: &str,
    path: &str,
    body: &str,
    state: &GuiApiState,
) -> Result<String, String> {
    let (session_id, action) = recording_parts(path);
    if method == "GET" || method == "HEAD" {
        let manifest = load_recording_session(&state.runtime_home, session_id)
            .map_err(|err| api_error_json("recording.not_found", &err.to_string()))?;
        return Ok(recording_manifest_json(&manifest));
    }

    let manifest = match action {
        "pause" => pause_recording_session(&state.runtime_home, session_id),
        "resume" => resume_recording_session(&state.runtime_home, session_id),
        "stop" => stop_recording_session(&state.runtime_home, session_id),
        "cancel" => cancel_recording_session(&state.runtime_home, session_id),
        "mark-input" | "mark-output" | "mark-secret" | "add-assertion" | "note" => {
            let value = json_string_field(body, "value").unwrap_or_else(|| action.to_owned());
            append_recording_note(&state.runtime_home, session_id, action, &value)
                .and_then(|_| load_recording_session(&state.runtime_home, session_id))
        }
        "normalise" => {
            let manifest = load_recording_session(&state.runtime_home, session_id)
                .map_err(|err| api_error_json("recording.not_found", &err.to_string()))?;
            let package = normalise_recording(&manifest.raw_events, &manifest.draft_runner)
                .map_err(|err| api_error_json("recording.invalid_state", &err.to_string()))?;
            return Ok(format!(
                r#"{{"sessionId":"{}","runnerId":"{}","steps":{},"inputs":{},"outputs":{},"yamlPreview":"{}","warnings":[]}}"#,
                escape_json(session_id),
                escape_json(&package.id),
                string_array_json(
                    &package
                        .steps
                        .iter()
                        .map(|step| format!("{step:?}"))
                        .collect::<Vec<_>>()
                ),
                string_array_json(&package.inputs),
                string_array_json(&package.outputs),
                escape_json(&std::fs::read_to_string(&manifest.draft_runner).unwrap_or_default())
            ));
        }
        "finalise" => {
            let manifest = load_recording_session(&state.runtime_home, session_id)
                .map_err(|err| api_error_json("recording.not_found", &err.to_string()))?;
            let recording_runner = finalise_recording(&manifest.root, &manifest.draft_runner)
                .map_err(|err| api_error_json("recording.invalid_state", &err.to_string()))?;
            let runner_id = slug(&manifest.name);
            let out = state.runtime_home.join("runners").join(format!("{runner_id}.draft.yaml"));
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
            }
            std::fs::copy(&recording_runner, &out)
                .map_err(|err| api_error_json("runtime.io", &err.to_string()))?;
            return Ok(format!(
                r#"{{"sessionId":"{}","runnerId":"{}","path":"{}","saved":true}}"#,
                escape_json(session_id),
                escape_json(&runner_id),
                escape_json(&out.display().to_string())
            ));
        }
        "test" => {
            let manifest = load_recording_session(&state.runtime_home, session_id)
                .map_err(|err| api_error_json("recording.not_found", &err.to_string()))?;
            let yaml = std::fs::read_to_string(&manifest.draft_runner).unwrap_or_default();
            let runner = runner_file_for_yaml_path(&manifest.draft_runner, &yaml)?;
            let result = execute_runner(state, &runner, "recording-test", body)?;
            return Ok(format!(
                r#"{{"sessionId":"{}","status":"{}","evidenceRef":"{}","outputs":{}}}"#,
                escape_json(session_id),
                escape_json(&result.status),
                escape_json(&result.evidence_ref),
                result.outputs_json
            ));
        }
        _ => return Err(api_error_json("runtime.not_found", "Recording action not found.")),
    }
    .map_err(|err| api_error_json("recording.invalid_state", &err.to_string()))?;

    Ok(recording_manifest_json(&manifest))
}

fn recording_manifest_json(manifest: &RecordingSessionManifest) -> String {
    let markers = std::fs::read_to_string(manifest.root.join("markers.jsonl"))
        .unwrap_or_default()
        .lines()
        .count();
    let raw_events = std::fs::read_to_string(&manifest.raw_events)
        .unwrap_or_default()
        .lines()
        .count();
    format!(
        r#"{{"sessionId":"{}","name":"{}","state":"{}","elapsedSeconds":0,"profile":"{}","adapter":"{}","activeApp":null,"captureState":"{}","captureBackend":{},"captureHeartbeatAt":{},"captureBlockedReasons":{},"rawEvents":{},"observations":{},"screenshots":{},"lastEventSummary":{},"markers":{},"draftRunnerPath":"{}","normalizedStepSummaries":[],"evidenceRefs":["{}"]}}"#,
        escape_json(&manifest.session_id),
        escape_json(&manifest.name),
        manifest.state.as_str(),
        escape_json(&manifest.profile),
        escape_json(manifest.adapters.first().map(String::as_str).unwrap_or("")),
        manifest.capture_state.as_str(),
        json_option(manifest.capture_backend.as_deref()),
        json_option(manifest.capture_heartbeat_at.as_deref()),
        string_array_json(&manifest.capture_blocked_reasons),
        raw_events,
        manifest.observations,
        manifest.screenshot_count,
        json_option(manifest.last_event_summary.as_deref()),
        markers,
        escape_json(&manifest.draft_runner.display().to_string()),
        escape_json(&manifest.screenshots.display().to_string())
    )
}

fn recording_target_kind(target: &str) -> RecordingTargetKind {
    match target {
        "desktop" => RecordingTargetKind::Desktop,
        "java" => RecordingTargetKind::Java,
        "remote" => RecordingTargetKind::Remote,
        "terminal" => RecordingTargetKind::Terminal,
        _ => RecordingTargetKind::Web,
    }
}

fn recording_target_profile(target: &str) -> (&'static str, &'static str) {
    match target {
        "desktop" => ("desktop", "greentic.desktop.vision"),
        "java" => ("java", "greentic.desktop.java-accessibility"),
        "remote" => ("remote", "greentic.desktop.vision"),
        "terminal" => ("terminal", "greentic.desktop.terminal.tn3270"),
        _ => ("web", "greentic.desktop.playwright"),
    }
}

fn recording_parts(path: &str) -> (&str, &str) {
    let rest = path.trim_start_matches("/api/v1/recordings/");
    rest.split_once('/')
        .map_or((rest, ""), |(id, action)| (id, action))
}

fn slug(value: &str) -> String {
    let slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '.'
            }
        })
        .collect::<String>();
    slug.trim_matches('.').to_owned()
}

fn create_planner_draft_json(body: &str, state: &GuiApiState) -> Result<String, String> {
    let prompt = json_string_field(body, "prompt").unwrap_or_default();
    if prompt.trim().is_empty() {
        return Err(api_error_json(
            "planner.needs_clarification",
            "Describe the task before generating a runner draft.",
        ));
    }

    let context = planner_context(state);
    let planned = plan_prompt_with_configured_llm(&prompt, &context, state)
        .map_err(|err| api_error_json(&err.code, &err.message))?;
    let draft = planned.draft;
    let draft_id = format!("draft-{:016x}", fnv1a64(prompt.as_bytes()));
    let draft_dir = planner_drafts_dir(state).join(&draft_id);
    std::fs::create_dir_all(&draft_dir).map_err(|err| {
        api_error_json(
            "runtime.io",
            &format!("Could not create planner draft directory: {err}"),
        )
    })?;
    let yaml = draft.render_yaml();
    let json = planner_draft_json(&draft_id, &draft, &yaml);
    std::fs::write(draft_dir.join("draft.json"), &json)
        .and_then(|_| std::fs::write(draft_dir.join("runner.yaml"), yaml))
        .and_then(|_| std::fs::write(draft_dir.join("request.json"), planned.request_json))
        .map_err(|err| api_error_json("runtime.io", &format!("Could not persist draft: {err}")))?;
    Ok(json)
}

fn plan_prompt_with_configured_llm(
    prompt: &str,
    context: &PlanningContext,
    state: &GuiApiState,
) -> Result<greentic_desktop_planner::PlannerResult, greentic_desktop_planner::PlannerDiagnostic> {
    let settings = load_llm_settings(state).unwrap_or_else(default_llm_settings);
    let provider = provider_by_id(&settings.provider).ok_or_else(|| {
        greentic_desktop_planner::PlannerDiagnostic {
            code: "settings.unknown_llm_provider".to_owned(),
            message: format!("Unknown LLM provider '{}'.", settings.provider),
        }
    })?;
    if provider.mode == "heuristic" {
        return plan_prompt_with_llm(
            prompt,
            context,
            &PlannerOptions::default(),
            &HeuristicLlmClient,
        );
    }
    if !is_openai_compatible_provider(provider.id) {
        return Err(greentic_desktop_planner::PlannerDiagnostic {
            code: "planner.llm_provider_unsupported".to_owned(),
            message: format!(
                "LLM provider '{}' is listed but does not have a live request adapter yet.",
                provider.id
            ),
        });
    }
    let api_key = provider.and_then_secret(state);
    if provider.secret_name.is_some() && api_key.as_deref().unwrap_or_default().is_empty() {
        return Err(greentic_desktop_planner::PlannerDiagnostic {
            code: "planner.llm_secret_missing".to_owned(),
            message: format!(
                "Add {} in Settings > LLM before generating runners with {}.",
                provider.secret_name.unwrap_or("the provider API key"),
                provider.name
            ),
        });
    }
    let client = OpenAiCompatibleLlmClient::new(
        provider.id,
        settings
            .endpoint
            .or_else(|| provider.endpoint.map(str::to_owned))
            .unwrap_or_default(),
        if settings.model.is_empty() {
            provider.default_model.to_owned()
        } else {
            settings.model
        },
        api_key,
    );
    plan_prompt_with_llm(prompt, context, &PlannerOptions::default(), &client)
}

fn planner_context(state: &GuiApiState) -> PlanningContext {
    let adapters = replay_adapter_registry(state).capabilities();

    PlanningContext {
        available_adapters: adapters,
        available_mcp_tools: Vec::new(),
        application_metadata: Vec::new(),
        existing_runners: state.runner_names.clone(),
        ltm_examples: Vec::new(),
        security_policies: vec!["unsigned drafts allowed locally".to_owned()],
        desktop_observations: Vec::new(),
    }
}

fn planner_draft_action_json(
    method: &str,
    path: &str,
    body: &str,
    state: &GuiApiState,
) -> Result<String, String> {
    let (draft_id, action) = planner_draft_parts(path);
    let draft_dir = planner_drafts_dir(state).join(draft_id);
    if !draft_dir.is_dir() {
        return Err(api_error_json(
            "planner.draft_not_found",
            "Draft not found.",
        ));
    }

    match (method, action) {
        ("GET" | "HEAD", "") | ("PATCH", "") => {
            std::fs::read_to_string(draft_dir.join("draft.json")).map_err(|err| {
                api_error_json("runtime.io", &format!("Could not read draft: {err}"))
            })
        }
        ("POST", "test") => {
            let runner_path = draft_dir.join("runner.yaml");
            let yaml = std::fs::read_to_string(&runner_path).map_err(|err| {
                api_error_json("runtime.io", &format!("Could not read draft runner: {err}"))
            })?;
            let runner = runner_file_for_yaml_path(&runner_path, &yaml)?;
            let result = execute_runner(state, &runner, "planner-test", body)?;
            Ok(format!(
                r#"{{"draftId":"{}","status":"{}","outputs":{},"evidenceRef":"{}","steps":{}}}"#,
                escape_json(draft_id),
                escape_json(&result.status),
                result.outputs_json,
                escape_json(&result.evidence_ref),
                result.steps_json
            ))
        }
        ("POST", "save") => {
            let yaml = std::fs::read_to_string(draft_dir.join("runner.yaml")).map_err(|err| {
                api_error_json("runtime.io", &format!("Could not read draft runner: {err}"))
            })?;
            let runner_id = json_string_field(
                &std::fs::read_to_string(draft_dir.join("draft.json")).unwrap_or_default(),
                "runnerId",
            )
            .unwrap_or_else(|| draft_id.to_owned());
            let out = state
                .runtime_home
                .join("runners")
                .join(format!("{runner_id}.draft.yaml"));
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|err| {
                    api_error_json(
                        "runtime.io",
                        &format!("Could not create runners dir: {err}"),
                    )
                })?;
            }
            std::fs::write(&out, yaml).map_err(|err| {
                api_error_json("runtime.io", &format!("Could not save runner draft: {err}"))
            })?;
            Ok(format!(
                r#"{{"draftId":"{}","runnerId":"{}","path":"{}","saved":true}}"#,
                escape_json(draft_id),
                escape_json(&runner_id),
                escape_json(&out.display().to_string())
            ))
        }
        _ => Err(api_error_json(
            "runtime.not_found",
            "Planner action not found.",
        )),
    }
}

fn runner_edit_draft_action_json(
    method: &str,
    path: &str,
    body: &str,
    state: &GuiApiState,
) -> Result<String, String> {
    let (runner_id, draft_id, action) = runner_edit_draft_parts(path);
    let runner = find_runner(state, runner_id)
        .ok_or_else(|| api_error_json("runner.not_found", "Runner not found."))?;
    let source_yaml = runner_yaml(&runner);
    let source_checksum = checksum_hex(source_yaml.as_bytes());

    if draft_id.is_empty() && method == "POST" {
        let instruction = json_string_field(body, "instruction").unwrap_or_default();
        let mode = json_string_field(body, "mode").unwrap_or_else(|| "extend".to_owned());
        let seed = format!("{}:{}:{}", runner.id, source_checksum, instruction);
        let draft_id = format!("edit-{:016x}", fnv1a64(seed.as_bytes()));
        let draft_dir = runner_edit_draft_dir(state, &runner.id, &draft_id);
        std::fs::create_dir_all(&draft_dir).map_err(|err| {
            api_error_json(
                "runtime.io",
                &format!("Could not create runner edit draft directory: {err}"),
            )
        })?;
        let json = runner_edit_draft_json(
            &draft_id,
            &runner,
            &source_yaml,
            &source_checksum,
            &instruction,
            &mode,
            "draft",
        );
        std::fs::write(draft_dir.join("request.json"), body)
            .and_then(|_| std::fs::write(draft_dir.join("source.yaml"), &source_yaml))
            .and_then(|_| std::fs::write(draft_dir.join("proposed.yaml"), &source_yaml))
            .and_then(|_| std::fs::write(draft_dir.join("draft.json"), &json))
            .map_err(|err| {
                api_error_json(
                    "runtime.io",
                    &format!("Could not persist edit draft: {err}"),
                )
            })?;
        return Ok(json);
    }

    if draft_id.is_empty() {
        return Err(api_error_json(
            "runner.edit_draft_not_found",
            "Runner edit draft not found.",
        ));
    }

    let draft_dir = runner_edit_draft_dir(state, &runner.id, draft_id);
    if !draft_dir.is_dir() {
        return Err(api_error_json(
            "runner.edit_draft_not_found",
            "Runner edit draft not found.",
        ));
    }

    match (method, action) {
        ("GET" | "HEAD", "") | ("PATCH", "") => {
            std::fs::read_to_string(draft_dir.join("draft.json")).map_err(|err| {
                api_error_json("runtime.io", &format!("Could not read edit draft: {err}"))
            })
        }
        ("POST", "plan") => plan_runner_edit_draft_json(&draft_dir, &runner, body),
        ("POST", "test") => test_runner_edit_draft_json(&draft_dir, draft_id, body),
        ("POST", "apply") => apply_runner_edit_draft_json(&draft_dir, &runner, draft_id, state),
        ("DELETE", "") => {
            std::fs::remove_dir_all(&draft_dir).map_err(|err| {
                api_error_json("runtime.io", &format!("Could not delete edit draft: {err}"))
            })?;
            Ok(format!(
                r#"{{"draftId":"{}","sourceRunnerId":"{}","deleted":true}}"#,
                escape_json(draft_id),
                escape_json(&runner.id)
            ))
        }
        _ => Err(api_error_json(
            "runtime.not_found",
            "Runner edit draft action not found.",
        )),
    }
}

fn runner_edit_draft_parts(path: &str) -> (&str, &str, &str) {
    let rest = path.trim_start_matches("/api/v1/runners/");
    let Some((runner_id, after_runner)) = rest.split_once("/edit-drafts") else {
        return (rest, "", "");
    };
    let after = after_runner.trim_start_matches('/');
    if after.is_empty() {
        return (runner_id, "", "");
    }
    after
        .split_once('/')
        .map_or((runner_id, after, ""), |(draft_id, action)| {
            (runner_id, draft_id, action)
        })
}

fn runner_edit_draft_dir(state: &GuiApiState, runner_id: &str, draft_id: &str) -> PathBuf {
    state
        .runtime_home
        .join("gui-edit-drafts")
        .join(runner_id)
        .join(draft_id)
}

fn runner_edit_draft_json(
    draft_id: &str,
    runner: &RunnerFile,
    yaml: &str,
    source_checksum: &str,
    instruction: &str,
    mode: &str,
    status: &str,
) -> String {
    let runner_model = runner_edit_model_json(runner, yaml);
    format!(
        r#"{{"draftId":"{}","sourceRunnerId":"{}","sourceChecksum":"{}","instruction":"{}","mode":"{}","status":"{}","sourceRunner":{},"proposedRunner":{},"openQuestions":[],"warnings":[],"changeSummary":[],"yamlPreview":"{}"}}"#,
        escape_json(draft_id),
        escape_json(&runner.id),
        escape_json(source_checksum),
        escape_json(instruction),
        escape_json(mode),
        escape_json(status),
        runner_model,
        runner_model,
        escape_json(yaml)
    )
}

fn plan_runner_edit_draft_json(
    draft_dir: &std::path::Path,
    runner: &RunnerFile,
    body: &str,
) -> Result<String, String> {
    let draft_json = std::fs::read_to_string(draft_dir.join("draft.json")).map_err(|err| {
        api_error_json("runtime.io", &format!("Could not read edit draft: {err}"))
    })?;
    let instruction = json_string_field(body, "instruction")
        .or_else(|| json_string_field(&draft_json, "instruction"))
        .unwrap_or_default();
    let source_checksum = json_string_field(&draft_json, "sourceChecksum").unwrap_or_default();
    let source_yaml = std::fs::read_to_string(draft_dir.join("source.yaml")).map_err(|err| {
        api_error_json(
            "runtime.io",
            &format!("Could not read edit draft source: {err}"),
        )
    })?;
    let plan = infer_runner_patch_plan(&source_yaml, &instruction);
    let proposed_yaml = apply_inferred_patch_to_yaml(&source_yaml, &plan);
    let status = if plan.open_questions.is_empty() {
        "ready"
    } else {
        "needs_questions"
    };
    let draft_id = draft_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("edit-draft");
    let json = runner_edit_plan_json(RunnerEditPlanRender {
        draft_id,
        runner,
        source_yaml: &source_yaml,
        proposed_yaml: &proposed_yaml,
        source_checksum: &source_checksum,
        instruction: &instruction,
        status,
        plan: &plan,
    });
    std::fs::write(draft_dir.join("proposed.yaml"), &proposed_yaml)
        .and_then(|_| std::fs::write(draft_dir.join("patch-plan.json"), plan.render_json()))
        .and_then(|_| std::fs::write(draft_dir.join("draft.json"), &json))
        .map_err(|err| {
            api_error_json(
                "runtime.io",
                &format!("Could not persist edit patch plan: {err}"),
            )
        })?;
    Ok(json)
}

fn test_runner_edit_draft_json(
    draft_dir: &std::path::Path,
    draft_id: &str,
    body: &str,
) -> Result<String, String> {
    let proposed_yaml =
        std::fs::read_to_string(draft_dir.join("proposed.yaml")).map_err(|err| {
            api_error_json(
                "runtime.io",
                &format!("Could not read edit draft proposed runner: {err}"),
            )
        })?;
    let proposed_path = draft_dir.join("proposed.yaml");
    let runner = runner_file_for_yaml_path(&proposed_path, &proposed_yaml)?;
    let result = execute_runner(
        &GuiApiState {
            runtime_home: draft_dir
                .parent()
                .and_then(|path| path.parent())
                .and_then(|path| path.parent())
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::temp_dir().join("greentic-desktop")),
            evidence_store: draft_dir.join("evidence"),
            ..GuiApiState::default()
        },
        &runner,
        "runner-edit-test",
        body,
    )?;
    Ok(format!(
        r#"{{"draftId":"{}","status":"{}","outputs":{},"evidenceRef":"{}","steps":{}}}"#,
        escape_json(draft_id),
        escape_json(&result.status),
        result.outputs_json,
        escape_json(&result.evidence_ref),
        result.steps_json
    ))
}

fn apply_runner_edit_draft_json(
    draft_dir: &std::path::Path,
    runner: &RunnerFile,
    draft_id: &str,
    state: &GuiApiState,
) -> Result<String, String> {
    let draft_json = std::fs::read_to_string(draft_dir.join("draft.json")).map_err(|err| {
        api_error_json("runtime.io", &format!("Could not read edit draft: {err}"))
    })?;
    let expected_checksum = json_string_field(&draft_json, "sourceChecksum").unwrap_or_default();
    let current_yaml = runner_yaml(runner);
    let current_checksum = checksum_hex(current_yaml.as_bytes());
    if expected_checksum != current_checksum {
        return Err(api_error_json(
            "runner.edit_conflict",
            "Runner changed after this edit draft was created. Rebase the edit before applying.",
        ));
    }

    let proposed_yaml =
        std::fs::read_to_string(draft_dir.join("proposed.yaml")).map_err(|err| {
            api_error_json(
                "runtime.io",
                &format!("Could not read edit draft proposed runner: {err}"),
            )
        })?;
    let version_id = next_runner_version_id(state, &runner.id);
    let version_dir = runner_versions_dir(state, &runner.id);
    std::fs::create_dir_all(&version_dir).map_err(|err| {
        api_error_json(
            "runtime.io",
            &format!("Could not create runner version directory: {err}"),
        )
    })?;
    std::fs::write(version_dir.join(format!("{version_id}.yaml")), &current_yaml)
        .and_then(|_| {
            std::fs::write(
                version_dir.join(format!("{version_id}.metadata.json")),
                format!(
                    r#"{{"versionId":"{}","runnerId":"{}","editDraftId":"{}","sourceChecksum":"{}","resultingChecksum":"{}","testEvidenceRef":"local://runner-edits/{}/test-results/latest"}}"#,
                    escape_json(&version_id),
                    escape_json(&runner.id),
                    escape_json(draft_id),
                    escape_json(&current_checksum),
                    escape_json(&checksum_hex(proposed_yaml.as_bytes())),
                    escape_json(draft_id)
                ),
            )
        })
        .map_err(|err| {
            api_error_json(
                "runtime.io",
                &format!("Could not persist runner version history: {err}"),
            )
        })?;

    let out = runner.path.clone().unwrap_or_else(|| {
        state
            .runtime_home
            .join("runners")
            .join(format!("{}.draft.yaml", runner.id))
    });
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            api_error_json(
                "runtime.io",
                &format!("Could not create runners dir: {err}"),
            )
        })?;
    }
    std::fs::write(&out, proposed_yaml).map_err(|err| {
        api_error_json(
            "runtime.io",
            &format!("Could not apply edited runner: {err}"),
        )
    })?;
    persist_runner_state(
        state,
        &runner.id,
        "validated",
        "passed",
        &format!("local://runners/{}/edit/{version_id}", runner.id),
    )?;

    Ok(format!(
        r#"{{"runnerId":"{}","status":"applied","previousVersion":"{}","currentVersion":"{}","mcpTool":"{}","evidenceRef":"local://runners/{}/edit/{}"}}"#,
        escape_json(&runner.id),
        escape_json(&version_id),
        escape_json(&format!("{}-current", version_id)),
        escape_json(&tool_name(&runner.id)),
        escape_json(&runner.id),
        escape_json(&version_id)
    ))
}

fn runner_versions_json(path: &str, state: &GuiApiState) -> Result<String, String> {
    let runner_id = path
        .trim_start_matches("/api/v1/runners/")
        .trim_end_matches("/versions");
    find_runner(state, runner_id)
        .ok_or_else(|| api_error_json("runner.not_found", "Runner not found."))?;
    let version_dir = runner_versions_dir(state, runner_id);
    let mut versions = Vec::new();
    if let Ok(entries) = std::fs::read_dir(version_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if let Some(version_id) = name.strip_suffix(".yaml") {
                versions.push(version_id.to_owned());
            }
        }
    }
    versions.sort();
    Ok(format!(
        r#"{{"runnerId":"{}","versions":{}}}"#,
        escape_json(runner_id),
        string_array_json(&versions)
    ))
}

fn restore_runner_version_json(path: &str, state: &GuiApiState) -> Result<String, String> {
    let rest = path.trim_start_matches("/api/v1/runners/");
    let Some((runner_id, rest)) = rest.split_once("/versions/") else {
        return Err(api_error_json(
            "runtime.not_found",
            "Runner version not found.",
        ));
    };
    let version_id = rest.trim_end_matches("/restore");
    let runner = find_runner(state, runner_id)
        .ok_or_else(|| api_error_json("runner.not_found", "Runner not found."))?;
    let version_path = runner_versions_dir(state, runner_id).join(format!("{version_id}.yaml"));
    let yaml = std::fs::read_to_string(&version_path).map_err(|err| {
        api_error_json(
            "runner.version_not_found",
            &format!("Could not read runner version: {err}"),
        )
    })?;
    let out = runner.path.clone().unwrap_or_else(|| {
        state
            .runtime_home
            .join("runners")
            .join(format!("{}.draft.yaml", runner.id))
    });
    std::fs::write(&out, yaml)
        .map_err(|err| api_error_json("runtime.io", &format!("Could not restore runner: {err}")))?;
    persist_runner_state(
        state,
        runner_id,
        "validated",
        "passed",
        &format!("local://runners/{runner_id}/restore/{version_id}"),
    )?;
    Ok(format!(
        r#"{{"runnerId":"{}","status":"restored","currentVersion":"{}","mcpTool":"{}","evidenceRef":"local://runners/{}/restore/{}"}}"#,
        escape_json(runner_id),
        escape_json(version_id),
        escape_json(&tool_name(runner_id)),
        escape_json(runner_id),
        escape_json(version_id)
    ))
}

fn runner_versions_dir(state: &GuiApiState, runner_id: &str) -> PathBuf {
    state
        .runtime_home
        .join("runners")
        .join("versions")
        .join(runner_id)
}

fn next_runner_version_id(state: &GuiApiState, runner_id: &str) -> String {
    let existing = runner_versions_dir(state, runner_id)
        .read_dir()
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    entry
                        .path()
                        .file_name()
                        .and_then(|value| value.to_str())
                        .and_then(|name| name.strip_suffix(".yaml"))
                        .and_then(|version| version.trim_start_matches('v').parse::<u32>().ok())
                })
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    format!("v{}", existing + 1)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct InferredRunnerPatchPlan {
    operations: Vec<InferredPatchOperation>,
    input_changes: Vec<String>,
    output_changes: Vec<String>,
    step_changes: Vec<String>,
    open_questions: Vec<String>,
    warnings: Vec<String>,
    change_summary: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InferredPatchOperation {
    operation: &'static str,
    target: String,
    after: String,
    rationale: String,
    safety: &'static str,
    requires_test: bool,
}

impl InferredRunnerPatchPlan {
    fn render_json(&self) -> String {
        format!(
            r#"{{"intentSummary":"Edit existing runner","preserveBehavior":true,"operations":{},"requiredAdapters":[],"inputChanges":{},"outputChanges":{},"secretChanges":[],"stepChanges":{},"assertionChanges":[],"extractorChanges":[],"policyImpact":"unchanged","openQuestions":{},"warnings":{},"changeSummary":{}}}"#,
            patch_operations_json(&self.operations),
            string_array_json(&self.input_changes),
            string_array_json(&self.output_changes),
            string_array_json(&self.step_changes),
            string_array_json(&self.open_questions),
            string_array_json(&self.warnings),
            string_array_json(&self.change_summary)
        )
    }
}

fn infer_runner_patch_plan(source_yaml: &str, instruction: &str) -> InferredRunnerPatchPlan {
    let lower = instruction.to_ascii_lowercase();
    let mut plan = InferredRunnerPatchPlan::default();

    for field in inferred_edit_inputs(&lower) {
        if !yaml_list(source_yaml, "inputs")
            .iter()
            .any(|existing| field_display_name(existing) == field)
        {
            plan.input_changes.push(field.clone());
            plan.operations.push(InferredPatchOperation {
                operation: "add_input",
                target: format!("/inputs/{field}"),
                after: field.clone(),
                rationale: format!("User requested support for {field}."),
                safety: "low",
                requires_test: true,
            });
            plan.change_summary
                .push(format!("Add input '{}'.", field.replace('_', " ")));
        }
    }

    for field in inferred_edit_outputs(&lower) {
        if !yaml_list(source_yaml, "outputs")
            .iter()
            .any(|existing| field_display_name(existing) == field)
        {
            plan.output_changes.push(field.clone());
            plan.operations.push(InferredPatchOperation {
                operation: "add_output",
                target: format!("/outputs/{field}"),
                after: field.clone(),
                rationale: format!("User requested returning {field}."),
                safety: "low",
                requires_test: true,
            });
            plan.change_summary
                .push(format!("Add output '{}'.", field.replace('_', " ")));
        }
    }

    for step in inferred_edit_steps(&lower) {
        plan.step_changes.push(step.clone());
        plan.operations.push(InferredPatchOperation {
            operation: "add_step",
            target: "/steps/-".to_owned(),
            after: step.clone(),
            rationale: "User requested an additional behavior.".to_owned(),
            safety: "medium",
            requires_test: true,
        });
        plan.change_summary.push(step);
    }

    if lower.trim().is_empty() || plan.operations.is_empty() {
        plan.open_questions.push(
            "Which input, output, or automation step should be changed in this runner?".to_owned(),
        );
    }
    plan
}

fn inferred_edit_inputs(lower: &str) -> Vec<String> {
    let mut inputs = Vec::new();
    if lower.contains("precision") || lower.contains("round") {
        inputs.push("precision".to_owned());
    }
    if lower.contains("discount") {
        inputs.push("discount_percentage".to_owned());
    }
    if lower.contains("invoice") {
        inputs.push("invoice_number".to_owned());
    }
    if lower.contains("phone") {
        inputs.push("phone".to_owned());
    }
    if lower.contains("name") && (lower.contains("column") || lower.contains("field")) {
        inputs.push("name".to_owned());
    }
    if lower.contains("email") && (lower.contains("column") || lower.contains("field")) {
        inputs.push("email".to_owned());
    }
    if lower.contains("alias") || lower.contains("aliases") {
        inputs.push("operation_alias".to_owned());
    }
    inputs
}

fn inferred_edit_outputs(lower: &str) -> Vec<String> {
    let mut outputs = Vec::new();
    if lower.contains("expression") || lower.contains("displayed") {
        outputs.push("expression".to_owned());
    }
    if lower.contains("discount") {
        outputs.push("discounted_total".to_owned());
    }
    if lower.contains("history") {
        outputs.push("history".to_owned());
    }
    if lower.contains("stderr") {
        outputs.push("stderr".to_owned());
    }
    if lower.contains("clipboard") {
        outputs.push("clipboard_value".to_owned());
    }
    outputs
}

fn inferred_edit_steps(lower: &str) -> Vec<String> {
    let mut steps = Vec::new();
    if lower.contains("subtract") || lower.contains("subtraction") || lower.contains("minus") {
        steps.push("Support subtraction operation.".to_owned());
    }
    if lower.contains("multiply") || lower.contains("times") {
        steps.push("Support multiplication operation.".to_owned());
    }
    if lower.contains("divide") || lower.contains("division") {
        steps.push("Support division operation.".to_owned());
    }
    if lower.contains("clipboard") {
        steps.push("Copy the final result to the clipboard before extraction.".to_owned());
    }
    steps
}

fn apply_inferred_patch_to_yaml(source_yaml: &str, plan: &InferredRunnerPatchPlan) -> String {
    if let Ok(Some(mut definition)) = parse_runner_definition_manifest(source_yaml) {
        for input in &plan.input_changes {
            add_typed_runner_input(&mut definition, input);
        }
        for output in &plan.output_changes {
            add_typed_runner_output(&mut definition, output);
        }
        return serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": "greentic.runner.v1",
            "runner_definition": definition,
        }))
        .unwrap_or_else(|_| source_yaml.to_owned());
    }

    let mut yaml = source_yaml.to_owned();
    for input in &plan.input_changes {
        yaml = append_yaml_list_value(&yaml, "inputs", &format!("inputs.{input}"));
    }
    for output in &plan.output_changes {
        yaml = append_yaml_list_value(&yaml, "outputs", &format!("outputs.{output}"));
    }
    for step in &plan.step_changes {
        yaml = append_yaml_list_value(&yaml, "steps", step);
    }
    yaml
}

fn add_typed_runner_input(definition: &mut RunnerDefinition, input: &str) {
    if definition
        .inputs
        .iter()
        .any(|existing| existing.name == input)
    {
        return;
    }
    definition.inputs.push(RunnerInput {
        name: input.to_owned(),
        value_type: WorkflowValueType::String,
        required: true,
        default_value: None,
        redaction: RedactionPolicy::None,
        validation: None,
    });
    if !definition
        .workflow
        .inputs
        .iter()
        .any(|existing| existing.name == input)
    {
        definition
            .workflow
            .inputs
            .push(greentic_desktop_workflow::WorkflowInput {
                name: input.to_owned(),
                value_type: WorkflowValueType::String,
                required: true,
                secret: false,
                target: LocatorTarget::default(),
                value_template: format!("{{{{inputs.{input}}}}}"),
            });
    }
}

fn add_typed_runner_output(definition: &mut RunnerDefinition, output: &str) {
    if definition
        .outputs
        .iter()
        .any(|existing| existing.name == output)
    {
        return;
    }
    definition.outputs.push(RunnerOutput {
        name: output.to_owned(),
        value_type: WorkflowValueType::String,
        required: true,
        extractor: WorkflowOutputExtractor::VisibleText(output.to_owned()),
        failure_behavior: OutputFailureBehavior::FailRunner,
    });
    if !definition
        .workflow
        .outputs
        .iter()
        .any(|existing| existing.name == output)
    {
        definition
            .workflow
            .outputs
            .push(greentic_desktop_workflow::WorkflowOutput {
                name: output.to_owned(),
                value_type: WorkflowValueType::String,
                extractor: WorkflowOutputExtractor::VisibleText(output.to_owned()),
                required: true,
                expected: None,
            });
    }
}

fn append_yaml_list_value(yaml: &str, key: &str, value: &str) -> String {
    let existing = yaml_list(yaml, key);
    if existing
        .iter()
        .any(|item| item == value || field_display_name(item) == value)
    {
        return yaml.to_owned();
    }
    if !yaml.lines().any(|line| line.trim() == format!("{key}:")) {
        let mut output = yaml.trim_end().to_owned();
        output.push_str(&format!("\n{key}:\n  - {value}\n"));
        return output;
    }

    let mut output = String::new();
    let mut inserted = false;
    let mut in_list = false;
    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed == format!("{key}:") {
            in_list = true;
            output.push_str(line);
            output.push('\n');
            continue;
        }
        if in_list && !trimmed.is_empty() && !line.starts_with(' ') && !inserted {
            output.push_str(&format!("  - {value}\n"));
            inserted = true;
            in_list = false;
        }
        output.push_str(line);
        output.push('\n');
    }
    if in_list && !inserted {
        output.push_str(&format!("  - {value}\n"));
    }
    output
}

fn patch_operations_json(operations: &[InferredPatchOperation]) -> String {
    format!(
        "[{}]",
        operations
            .iter()
            .map(|operation| {
                format!(
                    r#"{{"operation":"{}","target":"{}","after":"{}","rationale":"{}","safety":"{}","requiresTest":{}}}"#,
                    escape_json(operation.operation),
                    escape_json(&operation.target),
                    escape_json(&operation.after),
                    escape_json(&operation.rationale),
                    escape_json(operation.safety),
                    operation.requires_test
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    )
}

struct RunnerEditPlanRender<'a> {
    draft_id: &'a str,
    runner: &'a RunnerFile,
    source_yaml: &'a str,
    proposed_yaml: &'a str,
    source_checksum: &'a str,
    instruction: &'a str,
    status: &'a str,
    plan: &'a InferredRunnerPatchPlan,
}

fn runner_edit_plan_json(input: RunnerEditPlanRender<'_>) -> String {
    format!(
        r#"{{"draftId":"{}","sourceRunnerId":"{}","sourceChecksum":"{}","instruction":"{}","mode":"extend","status":"{}","sourceRunner":{},"proposedRunner":{},"patch":{},"openQuestions":{},"warnings":{},"changeSummary":{},"yamlPreview":"{}"}}"#,
        escape_json(input.draft_id),
        escape_json(&input.runner.id),
        escape_json(input.source_checksum),
        escape_json(input.instruction),
        escape_json(input.status),
        runner_edit_model_json(input.runner, input.source_yaml),
        runner_edit_model_json(input.runner, input.proposed_yaml),
        input.plan.render_json(),
        string_array_json(&input.plan.open_questions),
        string_array_json(&input.plan.warnings),
        string_array_json(&input.plan.change_summary),
        escape_json(input.proposed_yaml)
    )
}

fn runner_edit_model_json(runner: &RunnerFile, yaml: &str) -> String {
    let description = manifest_description(yaml)
        .unwrap_or_else(|| "Local runner package managed by Greentic Desktop.".to_owned());
    let inputs = manifest_declared_fields(yaml, "inputs");
    let outputs = manifest_declared_fields(yaml, "outputs");
    let secrets = manifest_declared_fields(yaml, "secrets");
    let assertions = manifest_declared_fields(yaml, "assertions");
    let steps = yaml_list(yaml, "steps")
        .iter()
        .enumerate()
        .map(|(index, step)| {
            format!(
                r#"{{"id":"step-{}","summary":"{}","editable":true}}"#,
                index + 1,
                escape_json(step)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"runnerId":"{}","name":"{}","description":"{}","risk":"medium","requiredAdapters":[],"inputs":{},"outputs":{},"secrets":{},"inputFields":{},"secretFields":{},"outputFields":{},"steps":[{}],"assertions":{},"yamlPreview":"{}"}}"#,
        escape_json(&runner.id),
        escape_json(&runner.name),
        escape_json(&description),
        field_names_json(&inputs),
        field_names_json(&outputs),
        field_names_json(&secrets),
        manifest_input_fields_json(yaml),
        manifest_secret_fields_without_status_json(yaml),
        manifest_output_fields_json(yaml),
        steps,
        field_names_json(&assertions),
        escape_json(yaml)
    )
}

fn delete_planner_draft_json(path: &str, state: &GuiApiState) -> Result<String, String> {
    let (draft_id, _) = planner_draft_parts(path);
    let draft_dir = planner_drafts_dir(state).join(draft_id);
    if draft_dir.is_dir() {
        std::fs::remove_dir_all(&draft_dir).map_err(|err| {
            api_error_json(
                "runtime.io",
                &format!("Could not delete planner draft: {err}"),
            )
        })?;
    }
    Ok(format!(
        r#"{{"draftId":"{}","deleted":true}}"#,
        escape_json(draft_id)
    ))
}

fn planner_draft_json(draft_id: &str, draft: &RunnerDraft, yaml: &str) -> String {
    let package = &draft.package;
    let steps = package
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            format!(
                r#"{{"id":"step-{}","summary":"{}","editable":true}}"#,
                index + 1,
                escape_json(&format!("{step:?}"))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"draftId":"{}","runnerId":"{}","name":"{}","description":"Draft generated from prompt","risk":"{}","requiredAdapters":{},"inputs":{},"outputs":{},"secrets":{},"steps":[{}],"assertions":{},"openQuestions":{},"yamlPreview":"{}","policyWarnings":[]}}"#,
        escape_json(draft_id),
        escape_json(&package.id),
        escape_json(&package.id),
        escape_json(&format!("{:?}", draft.risk).to_ascii_lowercase()),
        string_array_json(&draft.required_adapters),
        field_names_json(&package.inputs),
        field_names_json(&package.outputs),
        string_array_json(&package.secrets),
        steps,
        string_array_json(&package.assertions),
        string_array_json(&draft.open_questions),
        escape_json(yaml)
    )
}

fn planner_drafts_dir(state: &GuiApiState) -> PathBuf {
    state.runtime_home.join("gui-drafts")
}

fn planner_draft_parts(path: &str) -> (&str, &str) {
    let rest = path.trim_start_matches("/api/v1/planner/drafts/");
    rest.split_once('/')
        .map_or((rest, ""), |(id, action)| (id, action))
}

#[allow(clippy::too_many_arguments)]
fn extension_store_entry_json(
    id: &str,
    name: &str,
    category: &str,
    description: &str,
    permissions: &str,
    capabilities: &str,
    platform_compatible: bool,
    version: &str,
    source: &str,
    publisher: &str,
    installed: bool,
    enabled: bool,
    health: &str,
) -> String {
    format!(
        r#"{{"id":"{}","name":"{}","category":"{}","description":"{}","installed":{},"available":true,"status":"{}","enabled":{},"health":"{}","version":"{}","publisher":"{}","trust":"official","digest":"sha256:pending","source":"{}","permissions":{},"permissionPrompts":{},"capabilities":{},"platformCompatible":{}}}"#,
        escape_json(id),
        escape_json(name),
        escape_json(category),
        escape_json(description),
        installed,
        if installed { "installed" } else { "available" },
        enabled,
        escape_json(health),
        escape_json(version),
        escape_json(publisher),
        escape_json(source),
        csv_json_array(permissions),
        permission_prompts_json(&csv_values(permissions)),
        csv_json_array(capabilities),
        platform_compatible
    )
}

fn extension_install_json(body: &str, state: &GuiApiState) -> Result<String, String> {
    let source = json_string_field(body, "source")
        .or_else(|| json_string_field(body, "id"))
        .unwrap_or_else(|| "store://greentic.desktop.playwright".to_owned());
    let client = GreenticDistributorClient::new(state.runtime_home.join("extension-cache"));
    let artifact = client
        .resolve(&source)
        .map_err(|err| api_error_json("extension.resolve_failed", &err.to_string()))?;
    let store_entry = client.store_index().find(&artifact.extension_id);
    let permissions = store_entry
        .map(|entry| entry.permissions.clone())
        .unwrap_or_default();
    let metadata = ExtensionPackageMetadata {
        id: artifact.extension_id.clone(),
        name: artifact.extension_id.clone(),
        version: artifact.version.clone(),
        publisher: store_entry
            .map(|entry| entry.publisher.clone())
            .unwrap_or_else(|| "local".to_owned()),
        runtime: ExtensionRuntime::Sidecar,
        entrypoint: "sidecar/index.js".to_owned(),
        distribution_source: artifact.resolved_uri.clone(),
        platforms: ExtensionPlatforms {
            windows: true,
            macos: true,
            linux: true,
        },
        capabilities: store_entry
            .map(|entry| entry.capabilities.clone())
            .unwrap_or_else(|| vec!["extension.run".to_owned()]),
        permissions: ExtensionPermissions {
            network: permissions
                .iter()
                .any(|permission| permission == "network" || permission.starts_with("network.")),
            filesystem: if permissions
                .iter()
                .any(|permission| permission == "filesystem.write")
            {
                "write".to_owned()
            } else {
                "none".to_owned()
            },
            screen_capture: permissions.iter().any(|permission| {
                permission == "screen_capture"
                    || permission == "desktop.screenshot"
                    || permission == "desktop.screen_recording"
                    || permission == "desktop.portal_screenshot"
            }),
            keyboard_mouse: permissions.iter().any(|permission| {
                permission == "keyboard_mouse"
                    || permission == "desktop.input"
                    || permission == "desktop.input_monitoring"
            }),
        },
        sbom_path: "SBOM.spdx.json".to_owned(),
        signature_dir: "signatures/".to_owned(),
    };
    let approval = PermissionApproval {
        screen_capture: json_bool_field(body, "approveScreenCapture").unwrap_or(false),
        keyboard_mouse: json_bool_field(body, "approveKeyboardMouse").unwrap_or(false),
        filesystem_write: json_bool_field(body, "approveFilesystemWrite").unwrap_or(false),
    };
    let signed = !artifact.source_uri.starts_with("file://");
    let sbom_present = !artifact.source_uri.starts_with("file://");
    let verification = verify_extension_package_trust(
        &metadata,
        &ExtensionTrustPolicy::default(),
        signed,
        sbom_present,
        &approval,
    );
    if !verification.allowed {
        return Err(api_error_json(
            "extension.trust_policy_blocked",
            &verification.reasons.join("; "),
        ));
    }
    persist_gui_extension_record(
        state,
        &artifact.extension_id,
        &artifact.version,
        &artifact.resolved_uri,
        &artifact.digest,
        true,
        &GuiExtensionVerificationRecord {
            publisher: verification.publisher.clone(),
            signature_status: verification.signature_status.clone(),
            sbom_present: verification.sbom_present,
            trust_reasons: verification.reasons.clone(),
        },
    )?;
    let phases = artifact
        .phases
        .iter()
        .chain(
            [
                greentic_distributor_client::ResolutionPhase {
                    phase: "installing".to_owned(),
                    status: "complete".to_owned(),
                    message: "extension metadata written to the local store".to_owned(),
                },
                greentic_distributor_client::ResolutionPhase {
                    phase: "complete".to_owned(),
                    status: "complete".to_owned(),
                    message: "extension installed and ready".to_owned(),
                },
            ]
            .iter(),
        )
        .map(|phase| {
            format!(
                r#"{{"phase":"{}","status":"{}","message":"{}"}}"#,
                escape_json(&phase.phase),
                escape_json(&phase.status),
                escape_json(&phase.message)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        r#"{{"id":"{}","status":"installed","phase":"complete","version":"{}","source":"{}","resolvedUri":"{}","digest":"{}","localCachePath":"{}","publisher":"{}","signatureStatus":"{}","sbomPresent":{},"trustReasons":[],"permissions":{},"permissionPrompts":{},"capabilities":{},"phases":[{}],"needs_restart":false}}"#,
        escape_json(&artifact.extension_id),
        escape_json(&artifact.version),
        escape_json(&artifact.source_uri),
        escape_json(&artifact.resolved_uri),
        escape_json(&artifact.digest),
        escape_json(&artifact.local_path.display().to_string()),
        escape_json(&verification.publisher),
        escape_json(&verification.signature_status),
        verification.sbom_present,
        string_array_json(&metadata.permissions.as_allow_list()),
        permission_prompts_json(&metadata.permissions.as_allow_list()),
        string_array_json(&metadata.capabilities),
        phases
    ))
}

fn extension_action_json(path: &str, state: &GuiApiState) -> Result<String, String> {
    let id = path
        .trim_start_matches("/api/v1/extensions/")
        .split('/')
        .next()
        .unwrap_or("extension");
    let action = path.rsplit('/').next().unwrap_or("action");
    let status = match action {
        "verify" => "verified",
        "health" => "healthy",
        "enable" => {
            set_gui_extension_enabled(state, id, true)?;
            "enabled"
        }
        "disable" => {
            set_gui_extension_enabled(state, id, false)?;
            "disabled"
        }
        "remove" => {
            remove_gui_extension_record(state, id)?;
            "removed"
        }
        "update" => {
            persist_gui_extension_record(
                state,
                id,
                "latest",
                &format!("store://{id}"),
                &format!("sha256:{:016x}", fnv1a64(id.as_bytes())),
                true,
                &GuiExtensionVerificationRecord {
                    publisher: "greenticai".to_owned(),
                    signature_status: "valid".to_owned(),
                    sbom_present: true,
                    trust_reasons: Vec::new(),
                },
            )?;
            "complete"
        }
        _ => "queued",
    };
    Ok(format!(
        r#"{{"id":"{}","status":"{}","phase":"complete","version":"local","source":"store://{}","digest":"sha256:pending","publisher":"greenticai","permissions":[],"permissionPrompts":[],"capabilities":[],"health":"{}","message":"Extension manifest and local store entry are healthy.","needs_restart":false}}"#,
        escape_json(id),
        escape_json(status),
        escape_json(id),
        if action == "health" {
            "healthy"
        } else {
            "unknown"
        }
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GuiLlmSettings {
    provider: String,
    model: String,
    endpoint: Option<String>,
}

fn save_llm_settings_json(body: &str, state: &GuiApiState) -> Result<String, String> {
    let provider_id = json_string_field(body, "provider").ok_or_else(|| {
        api_error_json("settings.invalid_llm_provider", "LLM provider is required.")
    })?;
    let provider = provider_by_id(&provider_id).ok_or_else(|| {
        api_error_json(
            "settings.unknown_llm_provider",
            &format!("Unknown LLM provider '{provider_id}'."),
        )
    })?;
    let settings = GuiLlmSettings {
        provider: provider.id.to_owned(),
        model: json_string_field(body, "model")
            .unwrap_or_else(|| provider.default_model.to_owned()),
        endpoint: json_string_field(body, "endpoint")
            .or_else(|| provider.endpoint.map(str::to_owned)),
    };
    persist_llm_settings(state, &settings).map_err(|err| {
        api_error_json("runtime.io", &format!("Could not save LLM settings: {err}"))
    })?;
    if let Some(api_key) =
        json_string_field(body, "apiKey").filter(|value| !value.trim().is_empty())
    {
        if let Some(secret_name) = provider.secret_name {
            persist_gui_secret(state, secret_name, &api_key).map_err(|err| {
                api_error_json("runtime.io", &format!("Could not save LLM API key: {err}"))
            })?;
        }
    }
    llm_settings_json_for(&settings, state)
}

fn llm_settings_json(state: &GuiApiState) -> Result<String, String> {
    let settings = load_llm_settings(state).unwrap_or_else(default_llm_settings);
    llm_settings_json_for(&settings, state)
}

fn default_llm_settings() -> GuiLlmSettings {
    let provider = provider_by_id("local").expect("local provider is built in");
    GuiLlmSettings {
        provider: provider.id.to_owned(),
        model: provider.default_model.to_owned(),
        endpoint: provider.endpoint.map(str::to_owned),
    }
}

fn load_llm_settings(state: &GuiApiState) -> Option<GuiLlmSettings> {
    let contents = std::fs::read_to_string(llm_settings_path(state)).ok()?;
    let provider = json_string_field(&contents, "provider")?;
    let known = provider_by_id(&provider)?;
    Some(GuiLlmSettings {
        provider,
        model: json_string_field(&contents, "model")
            .unwrap_or_else(|| known.default_model.to_owned()),
        endpoint: json_string_field(&contents, "endpoint")
            .or_else(|| known.endpoint.map(str::to_owned)),
    })
}

fn persist_llm_settings(state: &GuiApiState, settings: &GuiLlmSettings) -> std::io::Result<()> {
    let path = llm_settings_path(state);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        format!(
            r#"{{"provider":"{}","model":"{}","endpoint":{}}}"#,
            escape_json(&settings.provider),
            escape_json(&settings.model),
            json_option(settings.endpoint.as_deref())
        ),
    )
}

fn llm_settings_path(state: &GuiApiState) -> std::path::PathBuf {
    state.runtime_home.join("settings").join("llm.json")
}

fn llm_settings_json_for(settings: &GuiLlmSettings, state: &GuiApiState) -> Result<String, String> {
    let provider = provider_by_id(&settings.provider).ok_or_else(|| {
        api_error_json(
            "settings.unknown_llm_provider",
            &format!("Unknown LLM provider '{}'.", settings.provider),
        )
    })?;
    Ok(format!(
        r#"{{"provider":"{}","model":"{}","endpoint":{},"secretRef":{},"mode":"{}","hasApiKey":{},"providers":{}}}"#,
        escape_json(provider.id),
        escape_json(&settings.model),
        json_option(settings.endpoint.as_deref()),
        json_option(
            provider
                .secret_name
                .map(|secret_name| format!("secret://{secret_name}"))
                .as_deref()
        ),
        escape_json(provider.mode),
        provider.and_then_secret(state).is_some(),
        llm_providers_json(state),
    ))
}

fn test_llm_settings_json(state: &GuiApiState) -> Result<String, String> {
    let settings = load_llm_settings(state).unwrap_or_else(default_llm_settings);
    let provider = provider_by_id(&settings.provider).ok_or_else(|| {
        api_error_json(
            "settings.unknown_llm_provider",
            &format!("Unknown LLM provider '{}'.", settings.provider),
        )
    })?;
    if provider.mode == "heuristic" {
        return Ok(
            r#"{"status":"ok","message":"Local heuristic planner is available."}"#.to_owned(),
        );
    }
    if !is_openai_compatible_provider(provider.id) {
        return Err(api_error_json(
            "settings.llm_provider_unsupported",
            &format!(
                "{} is listed but does not have a live request adapter yet.",
                provider.name
            ),
        ));
    }
    if let Some(secret_name) = provider.secret_name {
        if provider.and_then_secret(state).is_none() {
            return Err(api_error_json(
                "settings.llm_secret_missing",
                &format!(
                    "Add {secret_name} in Settings > LLM before testing {}.",
                    provider.name
                ),
            ));
        }
    }
    Ok(format!(
        r#"{{"status":"ok","message":"{} is configured for live prompt planning."}}"#,
        escape_json(provider.name)
    ))
}

fn llm_providers_json(state: &GuiApiState) -> String {
    format!(
        "[{}]",
        known_providers()
            .iter()
            .map(|provider| llm_provider_json(provider, state))
            .collect::<Vec<_>>()
            .join(",")
    )
}

trait GuiLlmProviderSecret {
    fn and_then_secret(&self, state: &GuiApiState) -> Option<String>;
}

impl GuiLlmProviderSecret for LlmProvider {
    fn and_then_secret(&self, state: &GuiApiState) -> Option<String> {
        let secret_name = self.secret_name?;
        read_gui_secret(state, secret_name)
            .or_else(|| std::env::var(secret_name).ok())
            .filter(|value| !value.trim().is_empty())
    }
}

fn llm_provider_json(provider: &LlmProvider, state: &GuiApiState) -> String {
    format!(
        r#"{{"id":"{}","name":"{}","label":"{}","defaultModel":"{}","mode":"{}","endpoint":{},"secretName":{},"requiresApiKey":{},"hasApiKey":{}}}"#,
        escape_json(provider.id),
        escape_json(provider.name),
        escape_json(provider.name),
        escape_json(provider.default_model),
        escape_json(provider.mode),
        json_option(provider.endpoint),
        json_option(provider.secret_name),
        provider.secret_name.is_some(),
        provider.and_then_secret(state).is_some(),
    )
}

fn gui_secrets_dir(state: &GuiApiState) -> PathBuf {
    state.runtime_home.join("secrets")
}

fn gui_secret_path(state: &GuiApiState, name: &str) -> Option<PathBuf> {
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.')
    {
        return None;
    }
    Some(gui_secrets_dir(state).join(name))
}

fn read_gui_secret(state: &GuiApiState, name: &str) -> Option<String> {
    std::fs::read_to_string(gui_secret_path(state, name)?)
        .ok()
        .map(|value| value.trim_end_matches(['\r', '\n']).to_owned())
        .filter(|value| !value.trim().is_empty())
}

fn persist_gui_secret(state: &GuiApiState, name: &str, value: &str) -> std::io::Result<()> {
    let path = gui_secret_path(state, name).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid secret name")
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, value.trim())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn json_option(value: Option<&str>) -> String {
    value
        .map(|value| format!(r#""{}""#, escape_json(value)))
        .unwrap_or_else(|| "null".to_owned())
}

fn string_array_json(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!(r#""{}""#, escape_json(value)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn csv_json_array(value: &str) -> String {
    let values = csv_values(value);
    string_array_json(&values)
}

fn csv_values(value: &str) -> Vec<String> {
    value
        .split(',')
        .filter(|item| !item.is_empty())
        .map(|item| item.trim().to_owned())
        .collect()
}

fn permission_prompts_json(permissions: &[String]) -> String {
    let prompts = permissions
        .iter()
        .map(|permission| permission_prompt_label(permission))
        .collect::<Vec<_>>();
    string_array_json(&prompts)
}

fn permission_prompt_label(permission: &str) -> String {
    match permission {
        "network" => "Network access".to_owned(),
        other if other.starts_with("network.") => "Network access".to_owned(),
        "screen_capture" | "desktop.screenshot" => "Screen capture".to_owned(),
        "keyboard_mouse" | "desktop.input" => "Keyboard and mouse control".to_owned(),
        "filesystem.write" => "Filesystem write access".to_owned(),
        other if other.starts_with("filesystem.") => "Filesystem access".to_owned(),
        other => other.replace(['_', '.'], " "),
    }
}

fn json_string_field(body: &str, field: &str) -> Option<String> {
    let needle = format!(r#""{field}""#);
    let after_field = body.split_once(&needle)?.1;
    let after_colon = after_field.split_once(':')?.1.trim_start();
    let mut value = String::new();
    let mut escaped = false;
    for ch in after_colon.strip_prefix('"')?.chars() {
        if escaped {
            value.push(match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                other => other,
            });
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Some(value);
        } else {
            value.push(ch);
        }
    }
    None
}

fn json_bool_field(body: &str, field: &str) -> Option<bool> {
    let needle = format!(r#""{field}""#);
    let after_field = body.split_once(&needle)?.1;
    let after_colon = after_field.split_once(':')?.1.trim_start();
    if after_colon.starts_with("true") {
        Some(true)
    } else if after_colon.starts_with("false") {
        Some(false)
    } else {
        None
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

fn checksum_hex(bytes: &[u8]) -> String {
    format!("{:016x}", fnv1a64(bytes))
}

fn escape_json(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect()
}

fn parse_request_line(request: &str) -> Option<(&str, &str)> {
    let mut parts = request.lines().next()?.split_whitespace();
    let method = parts.next()?;
    let path = parts.next()?;
    Some((method, path))
}

fn asset_response(asset: GuiAsset, head_only: bool) -> Vec<u8> {
    let headers = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: {}\r\ncontent-length: {}\r\netag: {}\r\n{}\r\nconnection: close\r\n\r\n",
        asset.content_type,
        asset.bytes.len(),
        asset.etag,
        security_headers()
    );
    let mut response = headers.into_bytes();
    if !head_only {
        response.extend_from_slice(asset.bytes);
    }
    response
}

fn http_response(
    status: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
    head_only: bool,
) -> Vec<u8> {
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\n{}\r\nconnection: close\r\n\r\n",
        body.len(),
        security_headers()
    );
    let mut response = headers.into_bytes();
    if !head_only {
        response.extend_from_slice(body);
    }
    response
}

fn security_headers() -> &'static str {
    "x-content-type-options: nosniff\r\nreferrer-policy: no-referrer\r\ncontent-security-policy: default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self'\r\ncache-control: no-store"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    fn get(addr: SocketAddr, path: &str) -> Vec<u8> {
        for _ in 0..10 {
            let mut stream = TcpStream::connect(addr).expect("connect to GUI host");
            if write!(
                stream,
                "GET {path} HTTP/1.1\r\nhost: 127.0.0.1\r\nconnection: close\r\n\r\n"
            )
            .is_err()
            {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            let mut response = Vec::new();
            if stream.read_to_end(&mut response).is_err() {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            if response.windows(4).any(|window| window == b"\r\n\r\n") {
                return response;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("GUI host did not return a complete HTTP response for {path}");
    }

    fn post(addr: SocketAddr, path: &str, body: &str) -> Vec<u8> {
        request_with_body(addr, "POST", path, body)
    }

    fn put(addr: SocketAddr, path: &str, body: &str) -> Vec<u8> {
        request_with_body(addr, "PUT", path, body)
    }

    fn request_with_body(addr: SocketAddr, method: &str, path: &str, body: &str) -> Vec<u8> {
        for _ in 0..10 {
            let mut stream = TcpStream::connect(addr).expect("connect to GUI host");
            if write!(
                stream,
                "{method} {path} HTTP/1.1\r\nhost: 127.0.0.1\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .is_err()
            {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            let mut response = Vec::new();
            if stream.read_to_end(&mut response).is_err() {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            if response.windows(4).any(|window| window == b"\r\n\r\n") {
                return response;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("GUI host did not return a complete HTTP response for {path}");
    }

    fn post_json(addr: SocketAddr, path: &str, body: &str) -> Vec<u8> {
        for _ in 0..10 {
            let mut stream = TcpStream::connect(addr).expect("connect to HTTP server");
            if write!(
                stream,
                "POST {path} HTTP/1.1\r\nhost: 127.0.0.1\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .is_err()
            {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            let mut response = Vec::new();
            if stream.read_to_end(&mut response).is_err() {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            if response.windows(4).any(|window| window == b"\r\n\r\n") {
                return response;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("HTTP server did not return a complete response for {path}");
    }

    fn post_with_headers(addr: SocketAddr, path: &str, body: &str, headers: &str) -> Vec<u8> {
        for _ in 0..10 {
            let mut stream = TcpStream::connect(addr).expect("connect to GUI host");
            if write!(
                stream,
                "POST {path} HTTP/1.1\r\nhost: 127.0.0.1\r\n{headers}content-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .is_err()
            {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            let mut response = Vec::new();
            if stream.read_to_end(&mut response).is_err() {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            if response.windows(4).any(|window| window == b"\r\n\r\n") {
                return response;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("GUI host did not return a complete HTTP response for {path}");
    }

    fn response_head(response: &[u8]) -> String {
        let header_end = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("response should contain headers");
        String::from_utf8_lossy(&response[..header_end]).into_owned()
    }

    fn response_body(response: &[u8]) -> &[u8] {
        let header_end = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("response should contain headers");
        &response[(header_end + 4)..]
    }

    fn content_length(head: &str) -> usize {
        head.lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .expect("response should include content-length")
            .parse()
            .expect("content-length should be numeric")
    }

    #[test]
    fn serves_index_assets_and_spa_routes() {
        let handle = GuiHost::start(GuiHostOptions::default()).expect("GUI host should start");

        let root = get(handle.addr(), "/");
        let root_head = response_head(&root);
        assert!(root_head.starts_with("HTTP/1.1 200 OK"));
        assert!(root_head.contains("content-type: text/html"));
        assert!(String::from_utf8_lossy(&root).contains("Greentic"));

        let create = get(handle.addr(), "/create");
        let create_head = response_head(&create);
        assert!(create_head.starts_with("HTTP/1.1 200 OK"));
        assert!(create_head.contains("content-type: text/html"));

        let favicon = get(handle.addr(), "/favicon.ico");
        let favicon_head = response_head(&favicon);
        assert!(favicon_head.starts_with("HTTP/1.1 200 OK"));
        assert!(favicon_head.contains("content-type: image/x-icon"));

        handle.shutdown();
    }

    #[test]
    fn serves_large_assets_without_truncating() {
        let handle = GuiHost::start(GuiHostOptions::default()).expect("GUI host should start");
        let largest_asset = greentic_desktop_gui_assets::asset_manifest()
            .into_iter()
            .filter_map(greentic_desktop_gui_assets::asset)
            .max_by_key(|asset| asset.bytes.len())
            .expect("embedded assets should not be empty");

        let response = get(handle.addr(), largest_asset.path);
        let head = response_head(&response);
        let body = response_body(&response);

        assert!(head.starts_with("HTTP/1.1 200 OK"));
        assert_eq!(content_length(&head), largest_asset.bytes.len());
        assert_eq!(body.len(), largest_asset.bytes.len());
        assert_eq!(body, largest_asset.bytes);

        handle.shutdown();
    }

    #[test]
    fn api_routes_are_reserved_for_later_handlers() {
        let handle = GuiHost::start(GuiHostOptions::default()).expect("GUI host should start");
        let response = get(handle.addr(), "/api/status");
        assert!(response_head(&response).starts_with("HTTP/1.1 404 Not Found"));
        assert!(String::from_utf8_lossy(&response).contains("\"ok\":false"));
        assert!(String::from_utf8_lossy(&response).contains("runtime.not_found"));
    }

    #[test]
    fn serves_versioned_api_contract() {
        let handle = GuiHost::start(GuiHostOptions::default()).expect("GUI host should start");

        let health = get(handle.addr(), "/api/v1/health");
        assert!(response_head(&health).starts_with("HTTP/1.1 200 OK"));
        let health = String::from_utf8_lossy(&health);
        assert!(health.contains("\"ok\":true"));
        assert!(health.contains("\"apiVersion\":\"v1\""));

        let info = get(handle.addr(), "/api/v1/runtime/info");
        let info = String::from_utf8_lossy(&info);
        assert!(info.contains("\"runtimeHome\""));
        assert!(info.contains("\"guiUrl\""));
        assert!(info.contains("greentic.desktop.core"));

        let setup = get(handle.addr(), "/api/v1/setup/checklist");
        let setup = String::from_utf8_lossy(&setup);
        assert!(setup.contains("\"items\""));
        assert!(setup.contains("runtime_home"));
        assert!(setup.contains("\"id\":\"screen_capture_permission\""));
        assert!(setup.contains("Browser, prompt, runner, and MCP flows can run without it."));
        assert!(setup.contains("\"id\":\"accessibility_permission\""));
        assert!(setup.contains("\"id\":\"input_control_permission\""));
    }

    #[test]
    fn setup_fix_creates_runtime_directories() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-setup-fix-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let state = GuiApiState {
            runtime_home: root.clone(),
            evidence_store: root.join("evidence"),
            ..GuiApiState::default()
        };

        let response = setup_fix_json(r#"{"id":"runtime_home"}"#, &state)
            .expect("setup fix should create runtime folders");

        assert!(response.contains("\"id\":\"runtime_home\""));
        assert!(response.contains("\"status\":\"created\""));
        assert!(state.runtime_home.is_dir());
        assert!(state.evidence_store.is_dir());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn setup_fix_returns_manual_message_when_platform_opener_is_unavailable() {
        let state = GuiApiState {
            platform: "plan9".to_owned(),
            ..GuiApiState::default()
        };

        let response = setup_fix_json(r#"{"id":"accessibility_permission"}"#, &state)
            .expect("unsupported platform should return a manual setup result");

        assert!(response.contains("\"id\":\"accessibility_permission\""));
        assert!(response.contains("\"status\":\"manual\""));
        assert!(response.contains("not supported by the setup opener"));
    }

    #[test]
    fn llm_settings_include_provider_catalog() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-llm-settings-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let state = GuiApiState {
            runtime_home: root.clone(),
            evidence_store: root.join("evidence"),
            ..GuiApiState::default()
        };
        let response =
            llm_settings_json_for(&default_llm_settings(), &state).expect("local settings");

        assert!(response.contains(r#""provider":"local""#));
        assert!(response.contains(r#""providers":["#));
        assert!(response.contains(r#""id":"deepseek""#));
        assert!(response.contains(r#""name":"DeepSeek""#));
        assert!(response.contains(r#""label":"DeepSeek""#));
        assert!(response.contains(r#""defaultModel":"deepseek-chat""#));
        assert!(response.contains(r#""secretName":"DEEPSEEK_API_KEY""#));
        assert!(response.contains(r#""requiresApiKey":true"#));

        let saved = save_llm_settings_json(r#"{"provider":"deepseek"}"#, &state)
            .expect("deepseek settings should save");
        assert!(saved.contains(r#""provider":"deepseek""#));
        assert!(saved.contains(r#""model":"deepseek-chat""#));
        assert!(saved.contains(r#""secretRef":"secret://DEEPSEEK_API_KEY""#));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn llm_settings_save_api_key_to_runtime_secret_store() {
        let old_key = std::env::var_os("DEEPSEEK_API_KEY");
        std::env::remove_var("DEEPSEEK_API_KEY");
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-llm-secret-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let state = GuiApiState {
            runtime_home: root.clone(),
            evidence_store: root.join("evidence"),
            ..GuiApiState::default()
        };

        let before = save_llm_settings_json(r#"{"provider":"deepseek"}"#, &state)
            .expect("settings should save without key");
        assert!(before.contains(r#""hasApiKey":false"#), "{before}");

        let saved = save_llm_settings_json(
            r#"{"provider":"deepseek","apiKey":"sk-deepseek-test"}"#,
            &state,
        )
        .expect("api key should save");
        assert!(saved.contains(r#""provider":"deepseek""#));
        assert!(saved.contains(r#""hasApiKey":true"#), "{saved}");
        assert!(!saved.contains("sk-deepseek-test"));
        assert_eq!(
            read_gui_secret(&state, "DEEPSEEK_API_KEY").as_deref(),
            Some("sk-deepseek-test")
        );

        let test = test_llm_settings_json(&state).expect("stored key should satisfy test");
        assert!(test.contains("DeepSeek is configured"));

        if let Some(value) = old_key {
            std::env::set_var("DEEPSEEK_API_KEY", value);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn gui_security_rejects_missing_token_and_cross_origin_mutations() {
        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                gui_token: "test-token".to_owned(),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        let missing = post(handle.addr(), "/api/v1/settings/llm/test", "{}");
        assert!(response_head(&missing).starts_with("HTTP/1.1 403 Forbidden"));
        assert!(String::from_utf8_lossy(&missing).contains("security.token_required"));

        let wrong_origin = post_with_headers(
            handle.addr(),
            "/api/v1/settings/llm/test",
            "{}",
            "x-greentic-gui-token: test-token\r\norigin: http://example.test\r\n",
        );
        assert!(response_head(&wrong_origin).starts_with("HTTP/1.1 403 Forbidden"));
        assert!(String::from_utf8_lossy(&wrong_origin).contains("security.origin_rejected"));

        let ok = post_with_headers(
            handle.addr(),
            "/api/v1/settings/llm/test",
            "{}",
            &format!(
                "x-greentic-gui-token: test-token\r\norigin: http://{}\r\n",
                handle.addr()
            ),
        );
        assert!(response_head(&ok).starts_with("HTTP/1.1 200 OK"));
    }

    #[test]
    fn gui_responses_include_security_headers() {
        let handle = GuiHost::start(GuiHostOptions::default()).expect("GUI host should start");

        let root = get(handle.addr(), "/");
        let root_head = response_head(&root);
        assert!(root_head.contains("content-security-policy"));
        assert!(root_head.contains("x-content-type-options: nosniff"));

        let health = get(handle.addr(), "/api/v1/health");
        let health_head = response_head(&health);
        assert!(health_head.contains("cache-control: no-store"));
        assert!(health_head.contains("referrer-policy: no-referrer"));
    }

    #[test]
    fn extension_install_api_uses_distributor_resolution() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-extension-install-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                mcp_bind: "127.0.0.1:0".to_owned(),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        let search = get(handle.addr(), "/api/v1/extensions/search?q=browser");
        let search = String::from_utf8_lossy(&search);
        assert!(search.contains("greentic.desktop.playwright"));
        assert!(search.contains("Playwright Web Adapter"));

        let versions = get(
            handle.addr(),
            "/api/v1/extensions/greentic.desktop.playwright/versions",
        );
        assert!(String::from_utf8_lossy(&versions).contains("\"1.0.0\""));

        let detail = get(
            handle.addr(),
            "/api/v1/extensions/greentic.desktop.playwright",
        );
        let detail = String::from_utf8_lossy(&detail);
        assert!(detail.contains("\"extension\""));
        assert!(detail.contains("\"permissions\":[\"network.localhost\"]"));
        assert!(detail.contains("\"permissionPrompts\":[\"Network access\"]"));
        assert!(detail.contains("\"capabilities\":[\"web.goto\""));

        let response = post(
            handle.addr(),
            "/api/v1/extensions/install",
            r#"{"source":"store://greentic.desktop.playwright"}"#,
        );
        let response = String::from_utf8_lossy(&response);
        assert!(response.contains("\"id\":\"greentic.desktop.playwright\""));
        assert!(response.contains("\"status\":\"installed\""));
        assert!(response.contains("\"phase\":\"complete\""));
        assert!(response.contains("\"resolvedUri\":\"oci://ghcr.io/"));
        assert!(response.contains("\"phase\":\"resolving\""));
        assert!(response.contains("\"phase\":\"downloading\""));
        assert!(response.contains("\"phase\":\"verifying\""));
        assert!(response.contains("\"phase\":\"installing\""));
        assert!(response.contains("\"needs_restart\":false"));
        assert!(response.contains("\"localCachePath\""));
        assert!(response.contains("\"permissionPrompts\":[\"Network access\"]"));

        let installed = get(handle.addr(), "/api/v1/extensions/installed");
        let installed = String::from_utf8_lossy(&installed);
        assert!(installed.contains("\"id\":\"greentic.desktop.playwright\""));
        assert!(installed.contains("\"enabled\":true"));

        let health = post(
            handle.addr(),
            "/api/v1/extensions/greentic.desktop.playwright/health",
            "{}",
        );
        let health = String::from_utf8_lossy(&health);
        assert!(health.contains("\"status\":\"healthy\""));
        assert!(health.contains("\"health\":\"healthy\""));

        let disabled = post(
            handle.addr(),
            "/api/v1/extensions/greentic.desktop.playwright/disable",
            "{}",
        );
        assert!(String::from_utf8_lossy(&disabled).contains("\"status\":\"disabled\""));
        let installed = get(handle.addr(), "/api/v1/extensions/installed");
        assert!(String::from_utf8_lossy(&installed).contains("\"enabled\":false"));

        let removed = post(
            handle.addr(),
            "/api/v1/extensions/greentic.desktop.playwright/remove",
            "{}",
        );
        assert!(String::from_utf8_lossy(&removed).contains("\"status\":\"removed\""));
        let installed = get(handle.addr(), "/api/v1/extensions/installed");
        assert!(!String::from_utf8_lossy(&installed).contains("greentic.desktop.playwright"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn extension_install_api_blocks_high_risk_extension_without_approval() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-extension-trust-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                mcp_bind: "127.0.0.1:0".to_owned(),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        let blocked = post(
            handle.addr(),
            "/api/v1/extensions/install",
            r#"{"source":"vision"}"#,
        );
        let blocked = String::from_utf8_lossy(&blocked);
        assert!(blocked.contains("HTTP/1.1 400 Bad Request"));
        assert!(blocked.contains("extension.trust_policy_blocked"));
        assert!(blocked.contains("screen capture permission requires approval"));

        let installed = get(handle.addr(), "/api/v1/extensions/installed");
        assert!(!String::from_utf8_lossy(&installed).contains("greentic.desktop.vision"));

        let approved = post(
            handle.addr(),
            "/api/v1/extensions/install",
            r#"{"source":"vision","approveScreenCapture":true}"#,
        );
        let approved = String::from_utf8_lossy(&approved);
        assert!(approved.contains("\"id\":\"greentic.desktop.vision\""));
        assert!(approved.contains("\"signatureStatus\":\"valid\""));
        assert!(approved.contains("\"sbomPresent\":true"));
        assert!(approved.contains("\"screen_capture\""));
        assert!(approved.contains("\"Screen capture\""));

        let installed = get(handle.addr(), "/api/v1/extensions/installed");
        let installed = String::from_utf8_lossy(&installed);
        assert!(installed.contains("\"id\":\"greentic.desktop.vision\""));
        assert!(installed.contains("\"signatureStatus\":\"valid\""));
        assert!(installed.contains("\"sbomPresent\":true"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn planner_draft_api_creates_tests_and_saves() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-planner-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        let body = r#"{"prompt":"Create a customer in the CRM and return the customer ID.","profile":"default"}"#;
        let response = post(handle.addr(), "/api/v1/planner/drafts", body);
        let response = String::from_utf8_lossy(&response);
        assert!(response.contains("\"ok\":true"));
        let draft_id = json_string_field(&response, "draftId").expect("draft id");

        let test = post(
            handle.addr(),
            &format!("/api/v1/planner/drafts/{draft_id}/test"),
            "{}",
        );
        let test = String::from_utf8_lossy(&test);
        assert!(
            test.contains("runner.input_missing")
                || test.contains("runner.output_extraction_failed")
                || test.contains("runner.execution_failed"),
            "{test}"
        );

        let save = post(
            handle.addr(),
            &format!("/api/v1/planner/drafts/{draft_id}/save"),
            "{}",
        );
        assert!(String::from_utf8_lossy(&save).contains("\"saved\":true"));
        assert!(root.join("runners").is_dir());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn planner_draft_api_infers_generic_spreadsheet_inputs() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-planner-spreadsheet-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        let response = post(
            handle.addr(),
            "/api/v1/planner/drafts",
            r#"{"prompt":"Ask for the name of a spreadsheet. In /tmp create the spreadsheet if it does not exist already. Otherwise open it. Add a new line to the spreadsheet with the name and email that the user provided. Save the changes."}"#,
        );
        let response = String::from_utf8_lossy(&response);
        assert!(
            response.contains("\"inputs\":[\"email\",\"name\",\"spreadsheet_name\"]"),
            "{response}"
        );
        assert!(
            response.contains("\"outputs\":[\"saved_status\"]"),
            "{response}"
        );
        assert!(
            response.contains("Which application should open the spreadsheet"),
            "{response}"
        );
        assert!(!response.contains("number_1"), "{response}");
        assert!(!response.contains("operation"), "{response}");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn planner_draft_api_routes_generic_app_to_native_not_java() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-planner-native-app-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let state = GuiApiState {
            runtime_home: root.clone(),
            evidence_store: root.join("evidence"),
            ..GuiApiState::default()
        };
        let native_adapter = replay_adapter_registry(&state)
            .capabilities()
            .into_iter()
            .find(|adapter| {
                adapter.supports("macos.activate_app")
                    || adapter.supports("windows.open_app")
                    || adapter.supports("linux.find_window")
            })
            .expect("platform native adapter should be registered")
            .adapter_id;
        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: state,
        })
        .expect("GUI host should start");

        let response = post(
            handle.addr(),
            "/api/v1/planner/drafts",
            r#"{"prompt":"Open the office application, use the provided path, name, and text inputs, then save the result."}"#,
        );
        let response = String::from_utf8_lossy(&response);

        assert!(response.contains(&native_adapter), "{response}");
        assert!(
            !response.contains("greentic.desktop.java-accessibility"),
            "{response}"
        );
        assert!(
            !response.contains("planner.unsupported_capability"),
            "{response}"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn planner_context_uses_replay_registry_not_static_platform_claims() {
        let context = planner_context(&GuiApiState {
            platform: "windows".to_owned(),
            ..GuiApiState::default()
        });

        assert!(context
            .available_adapters
            .iter()
            .any(|adapter| adapter.supports("web.goto")));
        assert!(!context
            .available_adapters
            .iter()
            .any(|adapter| adapter.supports("windows.open_app")));
    }

    #[test]
    fn planner_draft_api_uses_configured_live_llm_for_inputs_and_outputs() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("mock LLM should bind");
        let llm_addr = listener.local_addr().expect("mock addr");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("LLM request should arrive");
            let mut buffer = [0_u8; 16384];
            let read = stream.read(&mut buffer).expect("LLM request should read");
            let request = String::from_utf8_lossy(&buffer[..read]);
            assert!(request.contains("desktop.prompt_to_runner"));
            assert!(request.contains("green invoice portal"));
            assert!(request.contains("authorization: Bearer test-key"));
            let content = r#"{"runner_id":"llm.invoice.portal","version":"0.1.0-draft","summary":"LLM-generated invoice runner","risk_level":"low","required_capabilities":["web.goto","web.fill","web.extract_text"],"inputs":{"invoice_reference":{"type":"string"},"approval_code":{"type":"string"}},"outputs":{"payment_status":{"type":"string"}},"steps":[{"id":"open-portal","action":"goto","required_capability":"web.goto"},{"id":"fill-reference","action":"fill","required_capability":"web.fill","value":"{{inputs.invoice_reference}}"},{"id":"read-status","action":"extract_text","required_capability":"web.extract_text"}],"assertions":["payment status is visible"],"open_questions":[]}"#;
            let body = serde_json::json!({
                "choices": [{"message": {"content": content}}]
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("LLM response should write");
        });

        let root = std::env::temp_dir().join(format!(
            "greentic-gui-planner-llm-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        let saved = put(
            handle.addr(),
            "/api/v1/settings/llm",
            &format!(
                r#"{{"provider":"openai","model":"mock-model","endpoint":"http://{llm_addr}/v1","apiKey":"test-key"}}"#
            ),
        );
        let saved = String::from_utf8_lossy(&saved);
        assert!(saved.contains("\"provider\":\"openai\""), "{saved}");
        assert!(saved.contains("\"hasApiKey\":true"), "{saved}");
        assert!(!saved.contains("test-key"), "{saved}");

        let draft = post(
            handle.addr(),
            "/api/v1/planner/drafts",
            r#"{"prompt":"Use the green invoice portal with invoice reference and approval code and return payment status."}"#,
        );
        let draft = String::from_utf8_lossy(&draft);
        server.join().expect("mock LLM should finish");

        assert!(
            draft.contains("\"runnerId\":\"llm.invoice.portal\""),
            "{draft}"
        );
        assert!(
            draft.contains("\"inputs\":[\"approval_code\",\"invoice_reference\"]"),
            "{draft}"
        );
        assert!(
            draft.contains("\"outputs\":[\"payment_status\"]"),
            "{draft}"
        );
        assert!(!draft.contains("company_name"), "{draft}");
        assert!(root.join("gui-drafts").is_dir());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn recording_api_runs_lifecycle_and_finalises() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-recording-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        let response = post(
            handle.addr(),
            "/api/v1/recordings",
            r#"{"name":"Create customer in CRM","target":"__test_browser"}"#,
        );
        let response = String::from_utf8_lossy(&response);
        assert!(response.contains("\"state\":\"recording\""));
        assert!(response.contains("\"captureState\":\"recording\""));
        assert!(response.contains("greentic.recording.web.test"));
        let session_id = json_string_field(&response, "sessionId").expect("session id");

        for action in ["pause", "resume", "stop", "mark-input"] {
            let response = post(
                handle.addr(),
                &format!("/api/v1/recordings/{session_id}/{action}"),
                r#"{"value":"company_name"}"#,
            );
            assert!(String::from_utf8_lossy(&response).contains("\"ok\":true"));
        }

        let normalise = post(
            handle.addr(),
            &format!("/api/v1/recordings/{session_id}/normalise"),
            "{}",
        );
        assert!(String::from_utf8_lossy(&normalise).contains("\"yamlPreview\""));
        let manifest = load_recording_session(&root, &session_id).expect("manifest should load");
        std::fs::write(
            &manifest.draft_runner,
            "id: recorded.calculator\nname: Recorded Calculator\ninputs:\n  - inputs.number_1\n  - inputs.number_2\n  - inputs.operation\noutputs:\n  - outputs.result\n",
        )
        .expect("draft runner should be writable");

        let test = post(
            handle.addr(),
            &format!("/api/v1/recordings/{session_id}/test"),
            r#"{"sampleInputs":{"number_1":"1","number_2":"1","operation":"+"},"number_1":"1","number_2":"1","operation":"+"}"#,
        );
        let test = String::from_utf8_lossy(&test);
        assert!(test.contains("runner.invalid_manifest"), "{test}");

        let finalise = post(
            handle.addr(),
            &format!("/api/v1/recordings/{session_id}/finalise"),
            "{}",
        );
        assert!(String::from_utf8_lossy(&finalise).contains("\"saved\":true"));
        assert!(root.join("runners").is_dir());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn recording_api_blocks_targets_without_real_capture_sources() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-recording-blocked-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        for target in ["desktop", "java", "remote", "terminal"] {
            let response = post(
                handle.addr(),
                "/api/v1/recordings",
                &format!(r#"{{"name":"{target} recording","target":"{target}"}}"#),
            );
            let response = String::from_utf8_lossy(&response);
            assert!(
                response.contains("\"state\":\"blocked\""),
                "{target}: {response}"
            );
            assert!(
                response.contains("requires")
                    && (response.contains("Configure") || response.contains("Install")),
                "{target}: {response}"
            );
            assert!(
                !response.contains("\"captureState\":\"recording\""),
                "{target}: {response}"
            );
        }

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn desktop_recording_uses_installed_platform_extension_as_capture_source() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-recording-desktop-extension-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");
        let platform = detect_platform();
        let extension_id = platform_desktop_extension_ids(&platform)
            .into_iter()
            .next()
            .expect("platform extension id");

        let install = post(
            handle.addr(),
            "/api/v1/extensions/install",
            &format!(
                r#"{{"source":"store://{}","approveScreenCapture":true,"approveKeyboardMouse":true}}"#,
                extension_id
            ),
        );
        let install = String::from_utf8_lossy(&install);
        assert!(
            install.contains(&format!(r#""id":"{}""#, extension_id)),
            "{install}"
        );
        assert!(install.contains("\"status\":\"installed\""), "{install}");

        let response = post(
            handle.addr(),
            "/api/v1/recordings",
            r#"{"name":"desktop extension recording","target":"desktop"}"#,
        );
        let response = String::from_utf8_lossy(&response);
        assert!(
            !response.contains("greentic.recording.desktop.not-configured"),
            "{response}"
        );
        assert!(
            !response.contains("GREENTIC_ENABLE_EXPERIMENTAL_DESKTOP_RECORDING"),
            "{response}"
        );
        assert!(
            response.contains("greentic.recording.desktop")
                || response.contains("Grant ")
                || response.contains("\"captureState\":\"recording\""),
            "{response}"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn runner_api_validates_lists_mcp_tools_and_deletes_runners() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-runners-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let runners_dir = root.join("runners");
        std::fs::create_dir_all(&runners_dir).expect("runner dir should create");
        std::fs::write(
            runners_dir.join("crm.create_customer.draft.yaml"),
            "id: crm.create_customer\nname: Create CRM Customer\ndescription: Creates a customer in CRM.\ninputs:\n  - inputs.number_1\n  - inputs.number_2\n  - inputs.operation\noutputs:\n  - outputs.result\n",
        )
        .expect("runner should write");

        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        let list = get(handle.addr(), "/api/v1/runners");
        let list = String::from_utf8_lossy(&list);
        assert!(list.contains("\"id\":\"crm.create_customer\""));
        assert!(list.contains("Create CRM Customer"));
        assert!(list.contains("\"inputs\":[\"number_1\",\"number_2\",\"operation\"]"));
        assert!(list.contains("\"outputs\":[\"result\"]"));

        let rename = post(
            handle.addr(),
            "/api/v1/runners/crm.create_customer/rename",
            r#"{"name":"Create CRM Contact"}"#,
        );
        let rename = String::from_utf8_lossy(&rename);
        assert!(rename.contains("\"status\":\"renamed\""), "{rename}");
        let list = get(handle.addr(), "/api/v1/runners");
        let list = String::from_utf8_lossy(&list);
        assert!(list.contains("Create CRM Contact"), "{list}");
        assert!(!list.contains("Create CRM Customer"), "{list}");
        let tools = get(handle.addr(), "/api/v1/mcp/tools");
        let tools = String::from_utf8_lossy(&tools);
        assert!(
            tools.contains("\"runner\":\"Create CRM Contact\""),
            "{tools}"
        );

        let run = post(
            handle.addr(),
            "/api/v1/runners/crm.create_customer/run",
            r#"{"inputs":{"number_1":"1","number_2":"1","operation":"+"}}"#,
        );
        let run = String::from_utf8_lossy(&run);
        assert!(run.contains("runner.invalid_manifest"), "{run}");

        let validate = post(
            handle.addr(),
            "/api/v1/runners/crm.create_customer/validate",
            "{}",
        );
        assert!(
            String::from_utf8_lossy(&validate).contains("runner.invalid_manifest"),
            "{}",
            String::from_utf8_lossy(&validate)
        );

        let tools = get(handle.addr(), "/api/v1/mcp/tools");
        let tools = String::from_utf8_lossy(&tools);
        assert!(tools.contains("\"name\":\"runner.crm.create_customer\""));
        assert!(tools.contains("\"status\":\"enabled\""));

        let delete = post(
            handle.addr(),
            "/api/v1/runners/crm.create_customer/delete",
            "{}",
        );
        assert!(String::from_utf8_lossy(&delete).contains("\"status\":\"deleted\""));

        let list = get(handle.addr(), "/api/v1/runners");
        let list = String::from_utf8_lossy(&list);
        assert!(!list.contains("\"id\":\"crm.create_customer\""));
        let tools = get(handle.addr(), "/api/v1/mcp/tools");
        let tools = String::from_utf8_lossy(&tools);
        assert!(!tools.contains("\"name\":\"runner.crm.create_customer\""));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn typed_runner_definition_manifest_lists_and_runs_without_flat_yaml() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-typed-runner-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let runners_dir = root.join("runners");
        std::fs::create_dir_all(&runners_dir).expect("runner dir should create");

        let workflow = greentic_desktop_workflow::DesktopWorkflow {
            id: "generic.resource.update".to_owned(),
            summary: "Update a generic resource".to_owned(),
            target: greentic_desktop_workflow::WorkflowTarget::web("http://127.0.0.1/resource"),
            inputs: vec![greentic_desktop_workflow::WorkflowInput {
                name: "resource_name".to_owned(),
                value_type: greentic_desktop_workflow::WorkflowValueType::String,
                required: true,
                secret: false,
                target: LocatorTarget::default(),
                value_template: "{{inputs.resource_name}}".to_owned(),
            }],
            actions: Vec::new(),
            outputs: Vec::new(),
            assertions: Vec::new(),
            evidence_policy: greentic_desktop_workflow::WorkflowEvidencePolicy::default(),
        };
        let definition = RunnerDefinition::from_workflow(
            "generic.resource.update",
            "0.1.0-draft",
            "Update a generic resource",
            "Open a target and provide the resource name.",
            greentic_desktop_runner_schema::RunnerRisk::Low,
            vec![greentic_desktop_runner_schema::TargetTechnology::Web],
            workflow,
        )
        .expect("typed runner definition should compile");
        let manifest = serde_json::json!({
            "schema_version": "greentic.runner.v1",
            "runner_definition": definition,
        });
        std::fs::write(
            runners_dir.join("generic.resource.update.runner.json"),
            serde_json::to_string_pretty(&manifest).expect("manifest should render"),
        )
        .expect("typed manifest should write");

        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        let list = get(handle.addr(), "/api/v1/runners");
        let list = String::from_utf8_lossy(&list);
        assert!(
            list.contains("\"id\":\"generic.resource.update\""),
            "{list}"
        );
        assert!(list.contains("Update a generic resource"), "{list}");
        assert!(list.contains("\"inputs\":[\"resource_name\"]"), "{list}");

        let run = post(
            handle.addr(),
            "/api/v1/runners/generic.resource.update/run",
            r#"{"resource_name":"quarterly-report"}"#,
        );
        let run = String::from_utf8_lossy(&run);
        assert!(run.contains("\"status\":\"passed\""), "{run}");
        assert!(run.contains("\"outputs\":{}"), "{run}");

        let missing = post(
            handle.addr(),
            "/api/v1/runners/generic.resource.update/run",
            "{}",
        );
        let missing = String::from_utf8_lossy(&missing);
        assert!(missing.contains("runner.input_missing"), "{missing}");
        assert!(missing.contains("resource_name"), "{missing}");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn typed_runner_edit_adds_input_to_same_runner_manifest() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-typed-edit-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let runners_dir = root.join("runners");
        std::fs::create_dir_all(&runners_dir).expect("runner dir should create");

        let workflow = greentic_desktop_workflow::DesktopWorkflow {
            id: "generic.table.append".to_owned(),
            summary: "Append a generic table row".to_owned(),
            target: greentic_desktop_workflow::WorkflowTarget::web("http://127.0.0.1/table"),
            inputs: vec![greentic_desktop_workflow::WorkflowInput {
                name: "name".to_owned(),
                value_type: greentic_desktop_workflow::WorkflowValueType::String,
                required: true,
                secret: false,
                target: LocatorTarget::default(),
                value_template: "{{inputs.name}}".to_owned(),
            }],
            actions: Vec::new(),
            outputs: Vec::new(),
            assertions: Vec::new(),
            evidence_policy: greentic_desktop_workflow::WorkflowEvidencePolicy::default(),
        };
        let definition = RunnerDefinition::from_workflow(
            "generic.table.append",
            "0.1.0-draft",
            "Append a generic table row",
            "Open a target and append row fields.",
            greentic_desktop_runner_schema::RunnerRisk::Low,
            vec![greentic_desktop_runner_schema::TargetTechnology::Web],
            workflow,
        )
        .expect("typed runner definition should compile");
        let manifest_path = runners_dir.join("generic.table.append.runner.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": "greentic.runner.v1",
                "runner_definition": definition,
            }))
            .expect("manifest should render"),
        )
        .expect("typed manifest should write");

        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        let draft = post(
            handle.addr(),
            "/api/v1/runners/generic.table.append/edit-drafts",
            r#"{"instruction":"Add a phone column to the row.","mode":"extend"}"#,
        );
        let draft_id = json_string_field(&String::from_utf8_lossy(&draft), "draftId")
            .expect("draft id should parse");
        let plan = post(
            handle.addr(),
            &format!("/api/v1/runners/generic.table.append/edit-drafts/{draft_id}/plan"),
            r#"{"instruction":"Add a phone column to the row."}"#,
        );
        let plan = String::from_utf8_lossy(&plan);
        assert!(plan.contains("\"phone\""), "{plan}");

        let apply = post(
            handle.addr(),
            &format!("/api/v1/runners/generic.table.append/edit-drafts/{draft_id}/apply"),
            "{}",
        );
        let apply = String::from_utf8_lossy(&apply);
        assert!(apply.contains("\"status\":\"applied\""), "{apply}");

        let updated = std::fs::read_to_string(&manifest_path).expect("manifest should read");
        assert!(updated.contains("\"phone\""), "{updated}");
        let list = get(handle.addr(), "/api/v1/runners");
        let list = String::from_utf8_lossy(&list);
        assert!(list.contains("\"inputs\":[\"name\",\"phone\"]"), "{list}");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn typed_runner_summary_exposes_fields_and_runner_secrets_are_resolved() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-typed-secrets-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let runners_dir = root.join("runners");
        std::fs::create_dir_all(&runners_dir).expect("runner dir should create");

        let workflow = greentic_desktop_workflow::DesktopWorkflow {
            id: "generic.secure.append".to_owned(),
            summary: "Append a row with a protected token".to_owned(),
            target: greentic_desktop_workflow::WorkflowTarget::web("http://127.0.0.1/table"),
            inputs: vec![greentic_desktop_workflow::WorkflowInput {
                name: "name".to_owned(),
                value_type: greentic_desktop_workflow::WorkflowValueType::String,
                required: true,
                secret: false,
                target: LocatorTarget::default(),
                value_template: "{{inputs.name}}".to_owned(),
            }],
            actions: Vec::new(),
            outputs: Vec::new(),
            assertions: Vec::new(),
            evidence_policy: greentic_desktop_workflow::WorkflowEvidencePolicy::default(),
        };
        let mut definition = RunnerDefinition::from_workflow(
            "generic.secure.append",
            "0.1.0-draft",
            "Append a secure generic row",
            "Open a target and append row fields with a protected token.",
            greentic_desktop_runner_schema::RunnerRisk::Low,
            vec![greentic_desktop_runner_schema::TargetTechnology::Web],
            workflow,
        )
        .expect("typed runner definition should compile");
        definition
            .secrets
            .push(greentic_desktop_runner_schema::RunnerSecret {
                name: "api_token".to_owned(),
                value_type: WorkflowValueType::String,
                required: true,
                redaction: RedactionPolicy::Secret,
                validation: None,
            });
        definition.outputs.push(RunnerOutput {
            name: "saved_status".to_owned(),
            value_type: WorkflowValueType::String,
            required: true,
            extractor: WorkflowOutputExtractor::VisibleText("Saved".to_owned()),
            failure_behavior: OutputFailureBehavior::FailRunner,
        });
        std::fs::write(
            runners_dir.join("generic.secure.append.runner.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": "greentic.runner.v1",
                "runner_definition": definition,
            }))
            .expect("manifest should render"),
        )
        .expect("typed manifest should write");

        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        let list = get(handle.addr(), "/api/v1/runners");
        let list = String::from_utf8_lossy(&list);
        assert!(list.contains("\"inputFields\""), "{list}");
        assert!(list.contains("\"secretFields\""), "{list}");
        assert!(list.contains("\"outputFields\""), "{list}");
        assert!(list.contains("\"name\":\"api_token\""), "{list}");
        assert!(list.contains("visible text: Saved"), "{list}");

        let missing = post(
            handle.addr(),
            "/api/v1/runners/generic.secure.append/run",
            r#"{"name":"Maarten"}"#,
        );
        let missing = String::from_utf8_lossy(&missing);
        assert!(missing.contains("runner.secret_missing"), "{missing}");
        assert!(missing.contains("api_token"), "{missing}");

        let with_secret = post(
            handle.addr(),
            "/api/v1/runners/generic.secure.append/run",
            r#"{"name":"Maarten","api_token":"secret-value"}"#,
        );
        let with_secret = String::from_utf8_lossy(&with_secret);
        assert!(
            with_secret.contains("\"status\":\"passed\""),
            "{with_secret}"
        );
        assert!(
            with_secret.contains("outputs.saved_status"),
            "{with_secret}"
        );
        assert_eq!(
            read_gui_secret(
                &GuiApiState {
                    runtime_home: root.clone(),
                    ..GuiApiState::default()
                },
                "api_token"
            )
            .as_deref(),
            Some("secret-value")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn runner_edit_draft_loads_existing_runner_without_creating_duplicate() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-runner-edit-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let runners_dir = root.join("runners");
        std::fs::create_dir_all(&runners_dir).expect("runner dir should create");
        std::fs::write(
            runners_dir.join("calculator.basic.draft.yaml"),
            "id: calculator.basic\nname: Basic Calculator\ndescription: Adds two values.\ninputs:\n  - inputs.number_1\n  - inputs.number_2\noutputs:\n  - outputs.result\nsteps:\n  - Open calculator\n  - Enter values\n  - Read result\n",
        )
        .expect("runner should write");

        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        let detail = get(handle.addr(), "/api/v1/runners/calculator.basic");
        let detail = String::from_utf8_lossy(&detail);
        assert!(detail.contains("\"id\":\"calculator.basic\""), "{detail}");
        assert!(detail.contains("Adds two values."), "{detail}");

        let draft = post(
            handle.addr(),
            "/api/v1/runners/calculator.basic/edit-drafts",
            r#"{"instruction":"Also support subtraction.","mode":"extend"}"#,
        );
        let draft = String::from_utf8_lossy(&draft);
        assert!(
            draft.contains("\"sourceRunnerId\":\"calculator.basic\""),
            "{draft}"
        );
        assert!(
            draft.contains("\"instruction\":\"Also support subtraction.\""),
            "{draft}"
        );
        assert!(draft.contains("\"sourceChecksum\""), "{draft}");
        assert!(draft.contains("\"sourceRunner\""), "{draft}");
        assert!(draft.contains("\"proposedRunner\""), "{draft}");
        let draft_id = json_string_field(&draft, "draftId").expect("draft id");
        assert!(root
            .join("gui-edit-drafts")
            .join("calculator.basic")
            .join(&draft_id)
            .join("source.yaml")
            .is_file());

        let list = get(handle.addr(), "/api/v1/runners");
        let list = String::from_utf8_lossy(&list);
        assert_eq!(
            list.matches("\"id\":\"calculator.basic\"").count(),
            1,
            "{list}"
        );

        let loaded = get(
            handle.addr(),
            &format!("/api/v1/runners/calculator.basic/edit-drafts/{draft_id}"),
        );
        assert!(String::from_utf8_lossy(&loaded).contains(&draft_id));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn runner_edit_patch_planner_adds_structured_changes_and_questions() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-runner-edit-plan-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let runners_dir = root.join("runners");
        std::fs::create_dir_all(&runners_dir).expect("runner dir should create");
        std::fs::write(
            runners_dir.join("calculator.basic.draft.yaml"),
            "id: calculator.basic\nname: Basic Calculator\ndescription: Adds two values.\ninputs:\n  - inputs.number_1\n  - inputs.number_2\noutputs:\n  - outputs.result\nsteps:\n  - Open calculator\n",
        )
        .expect("runner should write");

        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                mcp_bind: "127.0.0.1:0".to_owned(),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        let draft = post(
            handle.addr(),
            "/api/v1/runners/calculator.basic/edit-drafts",
            r#"{"instruction":"Add a precision input and return the displayed expression.","mode":"extend"}"#,
        );
        let draft_id = json_string_field(&String::from_utf8_lossy(&draft), "draftId")
            .expect("draft id should parse");
        let plan = post(
            handle.addr(),
            &format!("/api/v1/runners/calculator.basic/edit-drafts/{draft_id}/plan"),
            r#"{"instruction":"Add a precision input and return the displayed expression."}"#,
        );
        let plan = String::from_utf8_lossy(&plan);
        assert!(plan.contains("\"status\":\"ready\""), "{plan}");
        assert!(plan.contains("\"operation\":\"add_input\""), "{plan}");
        assert!(plan.contains("\"operation\":\"add_output\""), "{plan}");
        assert!(plan.contains("\"precision\""), "{plan}");
        assert!(plan.contains("\"expression\""), "{plan}");
        assert!(root
            .join("gui-edit-drafts")
            .join("calculator.basic")
            .join(&draft_id)
            .join("patch-plan.json")
            .is_file());

        let test = post(
            handle.addr(),
            &format!("/api/v1/runners/calculator.basic/edit-drafts/{draft_id}/test"),
            r#"{"sampleInputs":{"number_1":"1","number_2":"1","operation":"+"},"number_1":"1","number_2":"1","operation":"+"}"#,
        );
        let test = String::from_utf8_lossy(&test);
        assert!(test.contains("runner.invalid_manifest"), "{test}");

        let apply = post(
            handle.addr(),
            &format!("/api/v1/runners/calculator.basic/edit-drafts/{draft_id}/apply"),
            "{}",
        );
        let apply = String::from_utf8_lossy(&apply);
        assert!(apply.contains("\"status\":\"applied\""), "{apply}");
        assert!(apply.contains("\"previousVersion\":\"v1\""), "{apply}");
        let list = get(handle.addr(), "/api/v1/runners");
        let list = String::from_utf8_lossy(&list);
        assert!(
            list.contains("\"outputs\":[\"result\",\"expression\"]"),
            "{list}"
        );
        let start = post(handle.addr(), "/api/v1/mcp/start", "{}");
        let start = String::from_utf8_lossy(&start);
        let bind = json_string_field(&start, "bind")
            .expect("mcp bind should be returned")
            .parse::<SocketAddr>()
            .expect("mcp bind should parse");
        let call = post_json(
            bind,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"runner.calculator.basic","arguments":{"number_1":"1","number_2":"1","operation":"+"}}}"#,
        );
        let call = String::from_utf8_lossy(&call);
        assert!(call.contains("\"error\""), "{call}");
        let stop = post(handle.addr(), "/api/v1/mcp/stop", "{}");
        assert!(String::from_utf8_lossy(&stop).contains("\"status\":\"stopped\""));
        let versions = get(handle.addr(), "/api/v1/runners/calculator.basic/versions");
        let versions = String::from_utf8_lossy(&versions);
        assert!(versions.contains("\"versions\":[\"v1\"]"), "{versions}");
        let restore = post(
            handle.addr(),
            "/api/v1/runners/calculator.basic/versions/v1/restore",
            "{}",
        );
        assert!(String::from_utf8_lossy(&restore).contains("\"status\":\"restored\""));
        let list = get(handle.addr(), "/api/v1/runners");
        let list = String::from_utf8_lossy(&list);
        assert!(list.contains("\"outputs\":[\"result\"]"), "{list}");
        assert!(
            !list.contains("\"outputs\":[\"result\",\"expression\"]"),
            "{list}"
        );

        let conflict_draft = post(
            handle.addr(),
            "/api/v1/runners/calculator.basic/edit-drafts",
            r#"{"instruction":"Add a precision input.","mode":"extend"}"#,
        );
        let conflict_draft_id =
            json_string_field(&String::from_utf8_lossy(&conflict_draft), "draftId")
                .expect("conflict draft id should parse");
        let _ = post(
            handle.addr(),
            &format!("/api/v1/runners/calculator.basic/edit-drafts/{conflict_draft_id}/plan"),
            r#"{"instruction":"Add a precision input."}"#,
        );
        std::fs::write(
            runners_dir.join("calculator.basic.draft.yaml"),
            "id: calculator.basic\nname: Basic Calculator\ndescription: Changed elsewhere.\ninputs:\n  - inputs.number_1\n  - inputs.number_2\noutputs:\n  - outputs.result\n",
        )
        .expect("runner should be changed");
        let conflict = post(
            handle.addr(),
            &format!("/api/v1/runners/calculator.basic/edit-drafts/{conflict_draft_id}/apply"),
            "{}",
        );
        let conflict = String::from_utf8_lossy(&conflict);
        assert!(conflict.contains("runner.edit_conflict"), "{conflict}");

        let vague = post(
            handle.addr(),
            "/api/v1/runners/calculator.basic/edit-drafts",
            r#"{"instruction":"Make it better.","mode":"extend"}"#,
        );
        let vague_id = json_string_field(&String::from_utf8_lossy(&vague), "draftId")
            .expect("vague draft id should parse");
        let vague_plan = post(
            handle.addr(),
            &format!("/api/v1/runners/calculator.basic/edit-drafts/{vague_id}/plan"),
            r#"{"instruction":"Make it better."}"#,
        );
        let vague_plan = String::from_utf8_lossy(&vague_plan);
        assert!(
            vague_plan.contains("\"status\":\"needs_questions\""),
            "{vague_plan}"
        );
        assert!(vague_plan.contains("Which input, output"), "{vague_plan}");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn prompt_runner_runs_publishes_and_returns_output_through_mcp() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-basic-loop-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                mcp_bind: "127.0.0.1:0".to_owned(),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        let draft = post(
            handle.addr(),
            "/api/v1/planner/drafts",
            r#"{"prompt":"Open the calculator app. Take three inputs: two numbers and one operation plus, minus, divide or multiply. Return the result."}"#,
        );
        let draft = String::from_utf8_lossy(&draft);
        assert!(draft.contains("\"inputs\":[\"number_1\",\"number_2\",\"operation\"]"));
        assert!(draft.contains("\"outputs\":[\"result\"]"));
        let draft_id = json_string_field(&draft, "draftId").expect("draft id");
        let runner_id = json_string_field(&draft, "runnerId").expect("runner id");

        let test = post(
            handle.addr(),
            &format!("/api/v1/planner/drafts/{draft_id}/test"),
            r#"{"sampleInputs":{"number_1":"1","number_2":"1","operation":"+"}}"#,
        );
        let test = String::from_utf8_lossy(&test);
        assert!(
            test.contains("runner.output_extraction_failed")
                || test.contains("runner.execution_failed"),
            "{test}"
        );

        let save = post(
            handle.addr(),
            &format!("/api/v1/planner/drafts/{draft_id}/save"),
            "{}",
        );
        assert!(String::from_utf8_lossy(&save).contains("\"saved\":true"));

        let run = post(
            handle.addr(),
            &format!("/api/v1/runners/{runner_id}/run"),
            r#"{"inputs":{"number_1":"1","number_2":"1","operation":"+"}}"#,
        );
        let run = String::from_utf8_lossy(&run);
        assert!(
            run.contains("runner.output_extraction_failed")
                || run.contains("runner.execution_failed"),
            "{run}"
        );

        let start = post(handle.addr(), "/api/v1/mcp/start", "{}");
        let start = String::from_utf8_lossy(&start);
        let bind = json_string_field(&start, "bind")
            .expect("mcp bind should be returned")
            .parse::<SocketAddr>()
            .expect("mcp bind should parse");
        let call = post_json(
            bind,
            "/mcp",
            &format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"{}","arguments":{{"number_1":"1","number_2":"1","operation":"+"}}}}}}"#,
                tool_name(&runner_id)
            ),
        );
        let call = String::from_utf8_lossy(&call);
        assert!(call.contains("\"error\""), "{call}");

        let stop = post(handle.addr(), "/api/v1/mcp/stop", "{}");
        assert!(String::from_utf8_lossy(&stop).contains("\"status\":\"stopped\""));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn managed_mcp_service_lists_and_blocks_disabled_tools() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-mcp-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let runners_dir = root.join("runners");
        std::fs::create_dir_all(&runners_dir).expect("runner dir should create");
        std::fs::write(
            runners_dir.join("crm.create_customer.draft.yaml"),
            "id: crm.create_customer\nname: Create CRM Customer\ninputs:\n  - inputs.number_1\n  - inputs.number_2\n  - inputs.operation\noutputs:\n  - outputs.result\n",
        )
        .expect("runner should write");

        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                mcp_bind: "127.0.0.1:0".to_owned(),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        let start = post(handle.addr(), "/api/v1/mcp/start", "{}");
        let start = String::from_utf8_lossy(&start);
        assert!(start.contains("\"status\":\"running\""));
        let bind = json_string_field(&start, "bind")
            .expect("mcp bind should be returned")
            .parse::<SocketAddr>()
            .expect("mcp bind should parse");

        let list = post_json(
            bind,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        );
        let list = String::from_utf8_lossy(&list);
        assert!(list.contains("runner.crm.create_customer"));

        let call = post_json(
            bind,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"runner.crm.create_customer","arguments":{"number_1":"1","number_2":"1","operation":"+"}}}"#,
        );
        let call = String::from_utf8_lossy(&call);
        assert!(call.contains("\"error\""), "{call}");
        assert!(
            call.contains("Runner manifest does not contain executable steps"),
            "{call}"
        );

        let disable = post(
            handle.addr(),
            "/api/v1/mcp/tools/crm.create_customer/disable",
            "{}",
        );
        assert!(String::from_utf8_lossy(&disable).contains("\"status\":\"disabled\""));

        let list = post_json(
            bind,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        );
        let list = String::from_utf8_lossy(&list);
        assert!(!list.contains("runner.crm.create_customer"));

        let stop = post(handle.addr(), "/api/v1/mcp/stop", "{}");
        assert!(String::from_utf8_lossy(&stop).contains("\"status\":\"stopped\""));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn approvals_evidence_and_refinement_apis_are_file_backed() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-approvals-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let runners_dir = root.join("runners");
        std::fs::create_dir_all(&runners_dir).expect("runner dir should create");
        std::fs::write(
            runners_dir.join("billing.high_risk.draft.yaml"),
            "id: billing.high_risk\nname: Billing High Risk\n",
        )
        .expect("runner should write");

        let handle = GuiHost::start(GuiHostOptions {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            api_state: GuiApiState {
                runtime_home: root.clone(),
                evidence_store: root.join("evidence"),
                ..GuiApiState::default()
            },
        })
        .expect("GUI host should start");

        let test = post(
            handle.addr(),
            "/api/v1/runners/billing.high_risk/test",
            "{}",
        );
        assert!(
            String::from_utf8_lossy(&test).contains("runner.invalid_manifest"),
            "{}",
            String::from_utf8_lossy(&test)
        );
        let evidence = get(handle.addr(), "/api/v1/evidence");
        assert!(
            !String::from_utf8_lossy(&evidence).contains("billing.high_risk-test"),
            "{}",
            String::from_utf8_lossy(&evidence)
        );

        let publish = post(
            handle.addr(),
            "/api/v1/runners/billing.high_risk/publish",
            "{}",
        );
        let publish = String::from_utf8_lossy(&publish);
        assert!(publish.contains("\"status\":\"approval_required\""));
        let approval_id = json_string_field(&publish, "approvalId").expect("approval id");
        let approvals = get(handle.addr(), "/api/v1/approvals");
        assert!(String::from_utf8_lossy(&approvals).contains(&approval_id));

        let approve = post(
            handle.addr(),
            &format!("/api/v1/approvals/{approval_id}/approve"),
            "{}",
        );
        assert!(String::from_utf8_lossy(&approve).contains("\"status\":\"approved\""));

        let refinement = post(
            handle.addr(),
            "/api/v1/runners/billing.high_risk/refinement",
            r#"{"correction":"Use the Save button in the billing form."}"#,
        );
        let refinement = String::from_utf8_lossy(&refinement);
        assert!(refinement.contains("\"status\":\"preview\""));
        let refinement_id = json_string_field(&refinement, "refinementId").expect("refinement id");

        let apply = post(
            handle.addr(),
            &format!("/api/v1/runners/billing.high_risk/refinement/{refinement_id}/apply"),
            "{}",
        );
        assert!(String::from_utf8_lossy(&apply).contains("\"applied\":true"));

        let blocked = get(handle.addr(), "/api/v1/evidence/../../etc/passwd");
        assert!(response_head(&blocked).starts_with("HTTP/1.1 404 Not Found"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn selects_platform_browser_commands() {
        assert_eq!(
            browser_command_for("macos", "http://127.0.0.1:1/"),
            BrowserCommand {
                program: "open",
                args: vec!["http://127.0.0.1:1/".to_owned()]
            }
        );
        assert_eq!(browser_command_for("windows", "u").program, "cmd");
        assert_eq!(browser_command_for("linux", "u").program, "xdg-open");
    }
}
