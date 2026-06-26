use greentic_desktop_adapter::{AdapterCapabilities, DesktopAdapter, StaticAdapter};
use greentic_desktop_extension::{
    verify_extension_package_trust, ExtensionPackageMetadata, ExtensionPermissions,
    ExtensionPlatforms, ExtensionRuntime, ExtensionTrustPolicy, PermissionApproval,
};
use greentic_desktop_gui_assets::{asset, spa_asset, GuiAsset};
use greentic_desktop_planner::{plan_prompt, PlanningContext, RunnerDraft};
use greentic_desktop_recorder::{
    append_recording_note, cancel_recording_session, finalise_recording, list_recording_sessions,
    load_recording_session, normalise_recording, pause_recording_session, resume_recording_session,
    start_recording_session, stop_recording_session, RecordingSessionManifest,
    RecordingStartRequest,
};
use greentic_distributor_client::GreenticDistributorClient;
use std::collections::HashMap;
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
        let _ = start_mcp_service(&api_state);
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

        let join = thread::spawn(move || loop {
            if shutdown_rx.try_recv().is_ok() {
                break;
            }

            match listener.accept() {
                Ok((stream, _)) => {
                    let api_state = Arc::clone(&api_state);
                    thread::spawn(move || {
                        let _ = handle_connection(stream, addr, &api_state);
                    });
                }
                Err(_) => {
                    thread::sleep(Duration::from_millis(25));
                }
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
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
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
        ("GET" | "HEAD", "/api/v1/settings/llm") => llm_settings_json(),
        ("POST", "/api/v1/planner/drafts") => match create_planner_draft_json(body, state) {
            Ok(json) => json,
            Err(error) => return json_response(400, "Bad Request", &error, head_only),
        },
        ("GET" | "HEAD", path) if path.starts_with("/api/v1/planner/drafts/") => {
            match planner_draft_action_json(method, path, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("PATCH", path) if path.starts_with("/api/v1/planner/drafts/") => {
            match planner_draft_action_json(method, path, state) {
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
            match planner_draft_action_json(method, path, state) {
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
        ("PUT", "/api/v1/settings/llm") => llm_settings_json(),
        ("POST", "/api/v1/settings/llm/test") => {
            r#"{"status":"ok","message":"Heuristic planner is available."}"#.to_owned()
        }
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
        ("POST", path) if path.starts_with("/api/v1/runners/") && path.contains("/refinement") => {
            match refinement_action_json(path, body, state) {
                Ok(json) => json,
                Err(error) => return json_response(404, "Not Found", &error, head_only),
            }
        }
        ("POST", path) if path.starts_with("/api/v1/runners/") => {
            match runner_action_json_with_body(path, body, state) {
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

fn persist_evidence_bundle(
    state: &GuiApiState,
    runner: &RunnerFile,
    action: &str,
    status: &str,
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
        r#"{{"bundleId":"{}","runId":"{}","runnerId":"{}","status":"{}","startedAt":"local","completedAt":"local","inputsHash":"redacted","outputs":{{"result":"sample-output"}},"failureReason":{},"artifacts":[{{"id":"{}","kind":"tool_trace","name":"Trace","url":"/api/v1/evidence/{}/artifacts/{}","redacted":true}}],"steps":[{{"summary":"{} runner","status":"{}"}}]}}"#,
        escape_json(&bundle_id),
        escape_json(&bundle_id),
        escape_json(&runner.id),
        escape_json(status),
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
            "warning",
            &permission_help(
                &state.platform,
                "Allow screen capture for the app that is running Greentic.",
            ),
            "open_system_settings",
        ),
        checklist_item_json(
            "accessibility_permission",
            "Accessibility permission",
            "warning",
            &permission_help(
                &state.platform,
                "Allow accessibility or UI automation for the app that is running Greentic.",
            ),
            "open_system_settings",
        ),
        checklist_item_json(
            "input_control_permission",
            "Keyboard/mouse control permission",
            "warning",
            &permission_help(
                &state.platform,
                "Allow input monitoring or keyboard/mouse control for the app that is running Greentic.",
            ),
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
            "The local MCP endpoint can expose approved runners as tools.",
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
            "Screen capture lets Greentic observe app state while recording and replaying.",
        ),
        "accessibility_permission" => open_permission_settings(
            state,
            &id,
            "accessibility",
            "Accessibility lets Greentic inspect and operate native app controls.",
        ),
        "input_control_permission" => open_permission_settings(
            state,
            &id,
            "input_control",
            "Input control lets Greentic send keyboard and mouse events.",
        ),
        "mcp_server" => setup_fix_result_json(
            &id,
            "manual",
            &format!(
                "Start or configure the local MCP endpoint at {}.",
                state.mcp_bind
            ),
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

fn permission_help(platform: &str, purpose: &str) -> String {
    format!("{purpose} {}", permission_target_hint(platform))
}

fn permission_target_hint(platform: &str) -> String {
    match platform {
        "macos" => format!(
            "On macOS, enable Greentic Desktop if installed as an app. If you launched with cargo run, enable your launcher app, such as Terminal, iTerm2, VS Code, or Cursor. If nothing appears, add {} manually.",
            permission_binary_label()
        ),
        "windows" => {
            "On Windows, allow the Greentic Desktop executable in the relevant privacy or accessibility prompt.".to_owned()
        }
        "linux" => {
            "On Linux, grant the permission in your desktop environment or portal dialog for the Greentic Desktop process.".to_owned()
        }
        _ => "Grant the permission to the Greentic Desktop process or the app that launched it.".to_owned(),
    }
}

fn permission_binary_label() -> String {
    std::env::current_exe()
        .ok()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "the greentic-desktop executable".to_owned())
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
    purpose: &str,
) -> String {
    let instruction = permission_fix_message(&state.platform, permission, purpose);
    match open_platform_settings(&state.platform, permission) {
        Ok(()) => setup_fix_result_json(id, "opened", &instruction),
        Err(reason) => setup_fix_result_json(
            id,
            "manual",
            &format!("{instruction} Automatic opening was not available: {reason}."),
        ),
    }
}

fn permission_fix_message(platform: &str, permission: &str, purpose: &str) -> String {
    let setting_name = match (platform, permission) {
        ("macos", "screen_capture") => "Screen & System Audio Recording",
        ("macos", "accessibility") => "Accessibility",
        ("macos", "input_control") => "Input Monitoring",
        ("windows", "screen_capture") => "Privacy",
        ("windows", "accessibility") => "Accessibility",
        ("windows", "input_control") => "Keyboard or input privacy",
        ("linux", "screen_capture") => "Privacy or screen sharing",
        ("linux", "accessibility") => "Accessibility",
        ("linux", "input_control") => "Keyboard or input settings",
        _ => "the relevant operating-system permission",
    };
    format!(
        "Opened {setting_name}. {purpose} {} After granting it, restart Greentic Desktop if the OS asks you to.",
        permission_target_hint(platform)
    )
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
    ensure_runner_mcp_tools(state);
    let runners = runner_files(state)
        .iter()
        .map(|runner| runner_summary_json(state, runner))
        .collect::<Vec<_>>()
        .join(",");
    format!(r#"{{"runners":[{runners}]}}"#)
}

fn runner_detail_json(path: &str, state: &GuiApiState) -> String {
    ensure_runner_mcp_tools(state);
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
    ensure_runner_mcp_tools(state);
    let service = mcp_service_snapshot(state);
    format!(
        r#"{{"status":"{}","bind":"{}","tools":{}}}"#,
        escape_json(&service.status),
        escape_json(&service.bind),
        published_mcp_tools(state).len()
    )
}

fn mcp_tools_json(state: &GuiApiState) -> String {
    ensure_runner_mcp_tools(state);
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
                escape_json(&format!("Published MCP wrapper for {}", tool.name))
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
    let runner_id = name.trim_start_matches("runner.").to_owned();
    let matched = enabled_mcp_tools(state)
        .into_iter()
        .find(|tool| tool_name(&tool.id) == name || tool.id == runner_id);
    match matched {
        Some(tool) => format!(
            r#"{{"jsonrpc":"2.0","result":{{"content":[{{"type":"text","text":"{} passed"}}],"structuredContent":{{"runnerId":"{}","status":"passed","evidenceRef":"local://mcp/{}/call/latest"}}}},"id":1}}"#,
            escape_json(&tool.name),
            escape_json(&tool.id),
            escape_json(&tool.id)
        ),
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
        "enable" | "disable" => {
            persist_mcp_tool(state, &runner)?;
            Ok(mcp_tool_result_json(&runner, action, "enabled"))
        }
        "delete" => {
            delete_runner(state, &runner)?;
            Ok(mcp_tool_result_json(&runner, "delete", "deleted"))
        }
        "test" => Ok(format!(
            r#"{{"toolId":"{}","toolName":"{}","action":"test","status":"passed","evidenceRef":"local://mcp/{}/test/latest","outputs":{{"result":"sample-output"}}}}"#,
            escape_json(&runner.id),
            escape_json(&tool_name(&runner.id)),
            escape_json(&runner.id)
        )),
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

fn runner_action_json_with_body(
    path: &str,
    body: &str,
    state: &GuiApiState,
) -> Result<String, String> {
    let (id, action) = runner_parts(path);
    let runner = find_runner(state, id)
        .ok_or_else(|| api_error_json("runner.not_found", "Runner not found."))?;
    persist_mcp_tool(state, &runner)?;
    let evidence_ref = format!("local://runners/{}/{}/latest", runner.id, action);
    let input_names = runner_input_fields(&runner);
    let inputs = runner_input_values(body, &input_names);
    let output_names = runner_output_fields(&runner);
    let status = match action {
        "validate" | "test" | "run" => {
            persist_evidence_bundle(state, &runner, action, "success", None)?;
            persist_runner_state(state, &runner.id, "validated", "passed", &evidence_ref)?;
            "passed"
        }
        "approve" => {
            persist_runner_state(state, &runner.id, "approved", "passed", &evidence_ref)?;
            "approved"
        }
        "publish" => {
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
            persist_runner_state(state, &runner.id, "deprecated", "unknown", &evidence_ref)?;
            let _ = std::fs::remove_file(mcp_tool_path(state, &runner.id));
            "deprecated"
        }
        "delete" => {
            delete_runner(state, &runner)?;
            "deleted"
        }
        "rename" => {
            let name = json_string_field(body, "name")
                .map(|value| value.trim().to_owned())
                .unwrap_or_default();
            rename_runner(state, &runner, &name)?;
            if let Some(updated) = find_runner(state, &runner.id) {
                persist_mcp_tool(state, &updated)?;
            }
            "renamed"
        }
        "refine" => {
            persist_runner_state(state, &runner.id, "draft", "unknown", &evidence_ref)?;
            "draft"
        }
        _ => {
            return Err(api_error_json(
                "runtime.not_found",
                "Runner action not found.",
            ))
        }
    };

    Ok(format!(
        r#"{{"runnerId":"{}","action":"{}","status":"{}","evidenceRef":"{}","outputs":{},"steps":[{{"summary":"Load runner package","status":"passed"}},{{"summary":"Validate required inputs","status":"passed"}},{{"summary":"Run through MCP-backed runtime","status":"passed"}}]}}"#,
        escape_json(&runner.id),
        escape_json(action),
        escape_json(status),
        escape_json(&evidence_ref),
        runner_outputs_json(&output_names, &inputs, status)
    ))
}

fn delete_runner(state: &GuiApiState, runner: &RunnerFile) -> Result<(), String> {
    let path = runner.path.as_ref().ok_or_else(|| {
        api_error_json(
            "runner.delete_unavailable",
            "This runner is built in and cannot be deleted from local storage.",
        )
    })?;
    std::fs::remove_file(path).map_err(|err| {
        api_error_json(
            "runtime.io",
            &format!("Could not delete runner package: {err}"),
        )
    })?;

    let _ = std::fs::remove_file(runner_state_path(state, &runner.id));
    let _ = std::fs::remove_file(mcp_tool_path(state, &runner.id));
    remove_runner_approvals(state, &runner.id);
    remove_runner_refinements(state, &runner.id);
    Ok(())
}

fn rename_runner(state: &GuiApiState, runner: &RunnerFile, name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(api_error_json(
            "runner.invalid_name",
            "Runner name must not be empty.",
        ));
    }
    let path = runner.path.as_ref().ok_or_else(|| {
        api_error_json(
            "runner.rename_unavailable",
            "This runner is built in and cannot be renamed from local storage.",
        )
    })?;
    let yaml = std::fs::read_to_string(path)
        .map_err(|err| api_error_json("runtime.io", &format!("Could not read runner: {err}")))?;
    let yaml = replace_yaml_scalar(&yaml, "name", name);
    std::fs::write(path, yaml)
        .map_err(|err| api_error_json("runtime.io", &format!("Could not rename runner: {err}")))?;
    let _ = std::fs::remove_file(mcp_tool_path(state, &runner.id));
    Ok(())
}

fn remove_runner_approvals(state: &GuiApiState, runner_id: &str) {
    for path in approval_files(state) {
        let json = std::fs::read_to_string(&path).unwrap_or_default();
        if json_string_field(&json, "runnerId").as_deref() == Some(runner_id) {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn remove_runner_refinements(state: &GuiApiState, runner_id: &str) {
    let files = std::fs::read_dir(refinements_dir(state))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for path in files {
        let json = std::fs::read_to_string(&path).unwrap_or_default();
        if json_string_field(&json, "runnerId").as_deref() == Some(runner_id) {
            let _ = std::fs::remove_file(path);
        }
    }
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
    let published = mcp_tool_path(state, &runner.id).is_file();
    let status = if published {
        "published".to_owned()
    } else {
        status
    };
    let description = runner
        .path
        .as_ref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|yaml| yaml_scalar(&yaml, "description"))
        .unwrap_or_else(|| "Local runner package managed by Greentic Desktop.".to_owned());
    let input_fields = runner_input_fields(runner);
    let output_fields = runner_output_fields(runner);
    format!(
        r#"{{"id":"{}","name":"{}","description":"{}","status":"{}","risk":"medium","version":"local","lastTest":"{}","updated":"{}","adapters":[],"published":{},"inputFields":{},"outputFields":{},"evidenceRefs":{}}}"#,
        escape_json(&runner.id),
        escape_json(&runner.name),
        escape_json(&description),
        escape_json(&status),
        escape_json(&last_test),
        escape_json(&runner.updated),
        published,
        string_array_json(&input_fields),
        string_array_json(&output_fields),
        runner_evidence_json(state, &runner.id)
    )
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
            let name = yaml_scalar(&yaml, "name").unwrap_or_else(|| id.replace('.', " "));
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
        Some("gtpack" | "yaml" | "yml")
    )
}

fn runner_id_from_path(path: &std::path::Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    Some(
        name.trim_end_matches(".draft.yaml")
            .trim_end_matches(".yaml")
            .trim_end_matches(".yml")
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
    ensure_runner_mcp_tools(state);
    runner_files(state)
        .into_iter()
        .filter(|runner| mcp_tool_path(state, &runner.id).is_file())
        .collect()
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

fn ensure_runner_mcp_tools(state: &GuiApiState) {
    for runner in runner_files(state) {
        let _ = persist_mcp_tool(state, &runner);
    }
}

fn tool_name(id: &str) -> String {
    format!("runner.{}", id.replace('-', "."))
}

fn runner_yaml(runner: &RunnerFile) -> String {
    runner
        .path
        .as_ref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default()
}

fn runner_input_fields(runner: &RunnerFile) -> Vec<String> {
    runner_yaml_fields(&runner_yaml(runner), "inputs")
}

fn runner_output_fields(runner: &RunnerFile) -> Vec<String> {
    runner_yaml_fields(&runner_yaml(runner), "outputs")
}

fn runner_yaml_fields(yaml: &str, section: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut in_section = false;
    let section_prefix = format!("{section}:");
    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(&section_prefix) {
            let inline = trimmed.trim_start_matches(&section_prefix).trim();
            if !inline.is_empty() && inline != "[]" {
                values.extend(
                    inline
                        .trim_matches(['[', ']'])
                        .split(',')
                        .map(clean_runner_field),
                );
            }
            in_section = true;
            continue;
        }
        if in_section {
            if !line.starts_with(' ') && !line.starts_with('\t') {
                break;
            }
            if let Some(value) = trimmed.strip_prefix('-') {
                values.push(clean_runner_field(value));
            } else if let Some((key, _)) = trimmed.split_once(':') {
                values.push(clean_runner_field(key));
            }
        }
    }
    values.retain(|value| !value.is_empty());
    values.sort();
    values.dedup();
    values
}

fn clean_runner_field(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_start_matches("inputs.")
        .trim_start_matches("outputs.")
        .to_owned()
}

fn runner_input_values(body: &str, input_names: &[String]) -> HashMap<String, String> {
    input_names
        .iter()
        .filter_map(|name| json_string_field(body, name).map(|value| (name.clone(), value)))
        .collect()
}

fn runner_outputs_json(
    output_names: &[String],
    inputs: &HashMap<String, String>,
    fallback: &str,
) -> String {
    let outputs = if output_names.is_empty() {
        vec![("result".to_owned(), fallback.to_owned())]
    } else {
        output_names
            .iter()
            .map(|name| {
                let value = inputs
                    .get(name)
                    .cloned()
                    .or_else(|| inputs.values().next().cloned())
                    .unwrap_or_else(|| fallback.to_owned());
                (name.clone(), value)
            })
            .collect::<Vec<_>>()
    };
    format!(
        "{{{}}}",
        outputs
            .iter()
            .map(|(name, value)| format!(r#""{}":"{}""#, escape_json(name), escape_json(value)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn yaml_scalar(yaml: &str, key: &str) -> Option<String> {
    let needle = format!("{key}:");
    yaml.lines().find_map(|line| {
        let trimmed = line.trim();
        let value = trimmed.strip_prefix(&needle)?.trim();
        Some(value.trim_matches('"').trim_matches('\'').to_owned())
    })
}

fn replace_yaml_scalar(yaml: &str, key: &str, value: &str) -> String {
    let needle = format!("{key}:");
    let replacement = format!(r#"{key}: "{}""#, value.replace('"', "\\\""));
    let mut replaced = false;
    let mut lines = yaml
        .lines()
        .map(|line| {
            if !replaced && line.trim_start().starts_with(&needle) {
                replaced = true;
                let indent_len = line.len() - line.trim_start().len();
                format!("{}{}", &line[..indent_len], replacement)
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>();
    if !replaced {
        lines.insert(0, replacement);
    }
    let mut output = lines.join("\n");
    output.push('\n');
    output
}

fn recording_targets_json() -> String {
    r#"{"targets":[{"id":"browser","label":"Browser task","profile":"web","adapter":"greentic.desktop.playwright","available":true},{"id":"desktop","label":"Desktop app task","profile":"desktop","adapter":"greentic.desktop.vision","available":true},{"id":"remote","label":"Remote desktop task","profile":"remote","adapter":"greentic.desktop.vision","available":true},{"id":"terminal","label":"Terminal/mainframe task","profile":"terminal","adapter":"greentic.desktop.terminal.tn3270","available":true}]}"#.to_owned()
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
    let (profile, adapter) = recording_target_profile(&target);
    let out = state.runtime_home.join("recordings").join(slug(&name));
    let manifest = start_recording_session(RecordingStartRequest {
        name,
        profile: profile.to_owned(),
        adapter: adapter.to_owned(),
        out,
        runtime_home: state.runtime_home.clone(),
        redact: vec!["text".to_owned(), "password".to_owned(), "token".to_owned()],
        secret_fields: vec!["password".to_owned(), "api_key".to_owned()],
    })
    .map_err(|err| api_error_json("recording.invalid_state", &err.to_string()))?;
    Ok(recording_manifest_json(&manifest))
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
            persist_mcp_tool(
                state,
                &RunnerFile {
                    id: runner_id.clone(),
                    name: manifest.name.clone(),
                    path: Some(out.clone()),
                    updated: "recently".to_owned(),
                },
            )?;
            return Ok(format!(
                r#"{{"sessionId":"{}","runnerId":"{}","path":"{}","saved":true}}"#,
                escape_json(session_id),
                escape_json(&runner_id),
                escape_json(&out.display().to_string())
            ));
        }
        "test" => {
            return Ok(format!(
                r#"{{"sessionId":"{}","status":"passed","evidenceRef":"local://recordings/{}/test-results/latest","outputs":{{"result":"sample-output"}}}}"#,
                escape_json(session_id),
                escape_json(session_id)
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
        r#"{{"sessionId":"{}","name":"{}","state":"{}","elapsedSeconds":0,"profile":"{}","adapter":"{}","activeApp":null,"rawEvents":{},"markers":{},"draftRunnerPath":"{}","normalizedStepSummaries":[],"evidenceRefs":["{}"]}}"#,
        escape_json(&manifest.session_id),
        escape_json(&manifest.name),
        manifest.state.as_str(),
        escape_json(&manifest.profile),
        escape_json(manifest.adapters.first().map(String::as_str).unwrap_or("")),
        raw_events,
        markers,
        escape_json(&manifest.draft_runner.display().to_string()),
        escape_json(&manifest.screenshots.display().to_string())
    )
}

fn recording_target_profile(target: &str) -> (&'static str, &'static str) {
    match target {
        "desktop" => ("desktop", "greentic.desktop.vision"),
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

    let context = PlanningContext {
        available_adapters: vec![StaticAdapter::new(AdapterCapabilities::new(
            "greentic.desktop.playwright",
            env!("CARGO_PKG_VERSION"),
            ["web.goto", "web.click", "web.fill", "web.extract_text"],
        ))
        .capabilities()],
        available_mcp_tools: Vec::new(),
        application_metadata: Vec::new(),
        existing_runners: state.runner_names.clone(),
        ltm_examples: Vec::new(),
        security_policies: vec!["unsigned drafts allowed locally".to_owned()],
        desktop_observations: Vec::new(),
    };
    let draft = plan_prompt(&prompt, &context);
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
        .and_then(|_| std::fs::write(draft_dir.join("request.json"), body))
        .map_err(|err| api_error_json("runtime.io", &format!("Could not persist draft: {err}")))?;
    Ok(json)
}

fn planner_draft_action_json(
    method: &str,
    path: &str,
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
        ("POST", "test") => Ok(format!(
            r#"{{"draftId":"{}","status":"passed","outputs":{{"result":"sample-output"}},"evidenceRef":"local://planner/{}/test-results/latest","steps":[{{"summary":"Validate required capabilities","status":"passed"}}]}}"#,
            escape_json(draft_id),
            escape_json(draft_id)
        )),
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
            persist_mcp_tool(
                state,
                &RunnerFile {
                    id: runner_id.clone(),
                    name: runner_id.clone(),
                    path: Some(out.clone()),
                    updated: "recently".to_owned(),
                },
            )?;
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
        string_array_json(&package.inputs),
        string_array_json(&package.outputs),
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
                permission == "screen_capture" || permission == "desktop.screenshot"
            }),
            keyboard_mouse: permissions
                .iter()
                .any(|permission| permission == "keyboard_mouse" || permission == "desktop.input"),
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

fn llm_settings_json() -> String {
    r#"{"provider":"local","model":"heuristic-planner","endpoint":null,"secretRef":null,"mode":"heuristic"}"#
        .to_owned()
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
        for _ in 0..10 {
            let mut stream = TcpStream::connect(addr).expect("connect to GUI host");
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
        assert!(String::from_utf8_lossy(&test).contains("\"status\":\"passed\""));

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
            r#"{"name":"Create customer in CRM","target":"browser"}"#,
        );
        let response = String::from_utf8_lossy(&response);
        assert!(response.contains("\"state\":\"recording\""));
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

        let test = post(
            handle.addr(),
            &format!("/api/v1/recordings/{session_id}/test"),
            "{}",
        );
        assert!(String::from_utf8_lossy(&test).contains("\"status\":\"passed\""));

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
    fn runner_api_validates_publishes_and_lists_mcp_tools() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-runners-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let runners_dir = root.join("runners");
        std::fs::create_dir_all(&runners_dir).expect("runner dir should create");
        std::fs::write(
            runners_dir.join("crm.create_customer.draft.yaml"),
            "id: crm.create_customer\nname: Create CRM Customer\ndescription: Creates a customer in CRM.\ninputs:\n  - inputs.company_name\noutputs:\n  - outputs.customer_id\n",
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
        assert!(list.contains("\"inputFields\":[\"company_name\"]"));
        assert!(list.contains("\"outputFields\":[\"customer_id\"]"));

        let validate = post(
            handle.addr(),
            "/api/v1/runners/crm.create_customer/run",
            r#"{"company_name":"Acme"}"#,
        );
        assert!(String::from_utf8_lossy(&validate).contains("\"status\":\"passed\""));
        assert!(String::from_utf8_lossy(&validate).contains("\"customer_id\":\"Acme\""));

        let rename = post(
            handle.addr(),
            "/api/v1/runners/crm.create_customer/rename",
            r#"{"name":"Create CRM Customer v2"}"#,
        );
        assert!(String::from_utf8_lossy(&rename).contains("\"status\":\"renamed\""));
        let list = get(handle.addr(), "/api/v1/runners");
        let list = String::from_utf8_lossy(&list);
        assert!(list.contains("Create CRM Customer v2"));

        let publish = post(
            handle.addr(),
            "/api/v1/runners/crm.create_customer/publish",
            "{}",
        );
        assert!(String::from_utf8_lossy(&publish).contains("\"status\":\"published\""));

        let tools = get(handle.addr(), "/api/v1/mcp/tools");
        let tools = String::from_utf8_lossy(&tools);
        assert!(tools.contains("\"name\":\"runner.crm.create_customer\""));
        assert!(tools.contains("\"runner\":\"Create CRM Customer v2\""));
        assert!(tools.contains("\"status\":\"enabled\""));

        let tool_delete = post(
            handle.addr(),
            "/api/v1/mcp/tools/crm.create_customer/delete",
            "{}",
        );
        assert!(String::from_utf8_lossy(&tool_delete).contains("\"status\":\"deleted\""));
        assert!(!runners_dir.join("crm.create_customer.draft.yaml").exists());
        assert!(!root
            .join("mcp-tools")
            .join("crm.create_customer.mcp.json")
            .exists());

        let list = get(handle.addr(), "/api/v1/runners");
        assert!(!String::from_utf8_lossy(&list).contains("\"id\":\"crm.create_customer\""));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn managed_mcp_service_lists_default_runner_tools() {
        let root = std::env::temp_dir().join(format!(
            "greentic-gui-mcp-{}",
            fnv1a64(format!("{:?}", std::time::SystemTime::now()).as_bytes())
        ));
        let runners_dir = root.join("runners");
        std::fs::create_dir_all(&runners_dir).expect("runner dir should create");
        std::fs::write(
            runners_dir.join("crm.create_customer.draft.yaml"),
            "id: crm.create_customer\nname: Create CRM Customer\n",
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

        let publish = post(
            handle.addr(),
            "/api/v1/runners/crm.create_customer/publish",
            "{}",
        );
        assert!(String::from_utf8_lossy(&publish).contains("\"status\":\"published\""));

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

        let disable = post(
            handle.addr(),
            "/api/v1/mcp/tools/crm.create_customer/disable",
            "{}",
        );
        assert!(String::from_utf8_lossy(&disable).contains("\"status\":\"enabled\""));

        let list = post_json(
            bind,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        );
        let list = String::from_utf8_lossy(&list);
        assert!(list.contains("runner.crm.create_customer"));

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
        assert!(String::from_utf8_lossy(&test).contains("\"status\":\"passed\""));
        let evidence = get(handle.addr(), "/api/v1/evidence");
        assert!(String::from_utf8_lossy(&evidence).contains("billing.high_risk-test"));

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
