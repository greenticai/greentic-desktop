use greentic_desktop_config::RuntimeConfig;
use greentic_desktop_extension::built_in_extension;
use greentic_desktop_gui::{
    export_runner_yaml_file, import_runner_yaml_file, open_default_browser, run_runner_yaml,
    GuiApiState, GuiHost, GuiHostOptions,
};
use greentic_desktop_planner::{
    plan_prompt_with_default_llm, save_draft_runner, PlannerDiagnostic, PlannerOptions,
    PlanningContext,
};
use greentic_desktop_recorder::{
    append_recording_note, cancel_recording_session, finalise_recording, list_recording_sessions,
    load_recording_session, normalise_recording, pause_recording_session, resume_recording_session,
    start_recording_session, stop_recording_session, RecordingLifecycleError,
    RecordingStartRequest, RecordingTargetKind,
};
use greentic_desktop_runtime::{discover_extensions, discover_runners, DesktopRuntime};
use serde_json::json;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime};

#[derive(Debug)]
pub enum CliError {
    Io(std::io::Error),
    Planner(PlannerDiagnostic),
    Recording(RecordingLifecycleError),
    Gui(greentic_desktop_gui::GuiError),
    Runtime(greentic_desktop_runtime::RuntimeError),
    Usage(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Planner(err) => write!(f, "{err}"),
            Self::Recording(err) => write!(f, "{err}"),
            Self::Gui(err) => write!(f, "{err}"),
            Self::Runtime(err) => write!(f, "{err}"),
            Self::Usage(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<std::io::Error> for CliError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<greentic_desktop_runtime::RuntimeError> for CliError {
    fn from(value: greentic_desktop_runtime::RuntimeError) -> Self {
        Self::Runtime(value)
    }
}

impl From<PlannerDiagnostic> for CliError {
    fn from(value: PlannerDiagnostic) -> Self {
        Self::Planner(value)
    }
}

impl From<RecordingLifecycleError> for CliError {
    fn from(value: RecordingLifecycleError) -> Self {
        Self::Recording(value)
    }
}

impl From<greentic_desktop_gui::GuiError> for CliError {
    fn from(value: greentic_desktop_gui::GuiError) -> Self {
        Self::Gui(value)
    }
}

pub fn run_desktop_cli(args: impl IntoIterator<Item = String>) -> Result<(), CliError> {
    run(args, false, &mut io::stdout(), true)
}

pub fn run_gtc_cli(args: impl IntoIterator<Item = String>) -> Result<(), CliError> {
    run(args, true, &mut io::stdout(), true)
}

pub fn run_with_writer(
    args: impl IntoIterator<Item = String>,
    require_desktop_prefix: bool,
    writer: &mut dyn Write,
) -> Result<(), CliError> {
    run(args, require_desktop_prefix, writer, false)
}

fn run(
    args: impl IntoIterator<Item = String>,
    require_desktop_prefix: bool,
    writer: &mut dyn Write,
    block_gui: bool,
) -> Result<(), CliError> {
    let mut args: Vec<String> = args.into_iter().collect();
    if !require_desktop_prefix && args.is_empty() {
        return run_gui(GuiCliOptions::default(), writer, block_gui);
    }

    if require_desktop_prefix {
        if args.first().map(String::as_str) != Some("desktop") {
            return Err(CliError::Usage(usage(true)));
        }
        args.remove(0);
        if args.is_empty() {
            return Err(CliError::Usage(usage(true)));
        }
    }

    if matches!(args.as_slice(), [command] if command == "--help" || command == "-h" || command == "help")
    {
        writer.write_all(usage(require_desktop_prefix).as_bytes())?;
        writeln!(writer)?;
        return Ok(());
    }

    if !require_desktop_prefix && args.first().map(String::as_str) == Some("gui") {
        return run_gui(parse_gui_args(&args[1..])?, writer, block_gui);
    }

    let config = RuntimeConfig::default();
    let runtime = DesktopRuntime::new(config.clone());

    match args.as_slice() {
        [command] if command == "info" => {
            writer.write_all(runtime.info().render_human().as_bytes())?;
        }
        [command] if command == "init" => {
            runtime.init()?;
            writeln!(writer, "initialized: {}", config.runner.home.display())?;
        }
        [flag, path] if flag == "--import" => {
            handle_runner_import(path, &runtime, &config, writer)?;
        }
        [flag, target, rest @ ..] if flag == "--run" => {
            handle_runner_run(target, rest, &config, writer)?;
        }
        [command, subcommand, rest @ ..] if command == "desktop" && subcommand == "validate" => {
            handle_desktop_validate(rest, &config, writer)?;
        }
        [command, rest @ ..] if command == "validate" => {
            handle_desktop_validate(rest, &config, writer)?;
        }
        [flag, target, rest @ ..] if flag == "--export" => {
            handle_runner_export(target, rest, &config, writer)?;
        }
        [command, subcommand] if command == "config" && subcommand == "show" => {
            writer.write_all(config.render_toml().as_bytes())?;
        }
        [command, subcommand] if command == "extension" && subcommand == "list" => {
            for extension in runtime.list_extensions()? {
                writeln!(
                    writer,
                    "{}\t{}\t{}",
                    extension.id,
                    extension.version,
                    extension.capabilities.join(",")
                )?;
            }
        }
        [command, subcommand, query] if command == "extension" && subcommand == "search" => {
            for extension in runtime.search_extension_store(query) {
                writeln!(
                    writer,
                    "{}\t{}\t{}\t{}",
                    extension.id, extension.latest, extension.name, extension.source
                )?;
            }
        }
        [command, subcommand, extension_id]
            if command == "extension" && subcommand == "install" =>
        {
            let manifest = runtime.install_extension(extension_id)?;
            writeln!(writer, "installed: {}", manifest.id)?;
        }
        [command, subcommand, extension_id]
            if command == "extension" && subcommand == "versions" =>
        {
            let versions = runtime
                .extension_versions(extension_id)
                .ok_or_else(|| CliError::Usage(format!("unknown extension: {extension_id}")))?;
            writeln!(writer, "{}\t{}", extension_id, versions.join(","))?;
        }
        [command, subcommand, extension_id] if command == "extension" && subcommand == "info" => {
            let manifest = runtime
                .list_extensions()?
                .into_iter()
                .find(|manifest| manifest.id == *extension_id)
                .ok_or_else(|| CliError::Usage(format!("unknown extension: {extension_id}")))?;
            writeln!(
                writer,
                "{}\t{}\t{}\t{}",
                manifest.id,
                manifest.version,
                manifest.runtime.as_str(),
                manifest.capabilities.join(",")
            )?;
        }
        [command, subcommand, extension_id] if command == "extension" && subcommand == "update" => {
            let manifest = runtime.update_extension(extension_id)?;
            writeln!(writer, "updated: {}", manifest.id)?;
        }
        [command, subcommand, extension_id] if command == "extension" && subcommand == "remove" => {
            runtime.remove_extension(extension_id)?;
            writeln!(writer, "removed: {extension_id}")?;
        }
        [command, subcommand, extension_id] if command == "extension" && subcommand == "enable" => {
            runtime.set_extension_enabled(extension_id, true)?;
            writeln!(writer, "enabled: {extension_id}")?;
        }
        [command, subcommand, extension_id]
            if command == "extension" && subcommand == "disable" =>
        {
            runtime.set_extension_enabled(extension_id, false)?;
            writeln!(writer, "disabled: {extension_id}")?;
        }
        [command, subcommand, extension_id] if command == "extension" && subcommand == "health" => {
            let health = runtime.extension_health(extension_id)?;
            writeln!(writer, "health: {}\t{}", health.id, health.status)?;
        }
        [command, subcommand] if command == "extension" && subcommand == "update" => {
            let installed = runtime.verify_extensions()?;
            writeln!(writer, "checked: {} extensions", installed.len())?;
        }
        [command, subcommand] if command == "extension" && subcommand == "verify" => {
            let installed = runtime.verify_extensions()?;
            writeln!(writer, "verified: {} extensions", installed.len())?;
        }
        [command, subcommand, extension_id] if command == "extension" && subcommand == "verify" => {
            let manifest = built_in_extension(extension_id)
                .ok_or_else(|| CliError::Usage(format!("unknown extension: {extension_id}")))?;
            runtime
                .extension_manager()
                .verify(&manifest)
                .map_err(|err| CliError::Usage(err.to_string()))?;
            writeln!(writer, "verified: {}", manifest.id)?;
        }
        [command, subcommand, extension_id]
            if command == "extension" && subcommand == "sidecar" =>
        {
            let sidecar = runtime.start_sidecar(extension_id)?;
            writeln!(
                writer,
                "sidecar: {} {} {}",
                sidecar.extension_id,
                sidecar.command,
                sidecar.args.join(" ")
            )?;
        }
        [command, subcommand] if command == "runner" && subcommand == "list" => {
            for runner in discover_runners(&config.runner.home)? {
                writeln!(writer, "{runner}")?;
            }
        }
        [command, subcommand, path] if command == "runner" && subcommand == "import" => {
            handle_runner_import(path, &runtime, &config, writer)?;
        }
        [command, subcommand, target, rest @ ..] if command == "runner" && subcommand == "run" => {
            handle_runner_run(target, rest, &config, writer)?;
        }
        [command, subcommand, rest @ ..] if command == "runner" && subcommand == "validate" => {
            handle_desktop_validate(rest, &config, writer)?;
        }
        [command, subcommand, target, rest @ ..]
            if command == "runner" && subcommand == "export" =>
        {
            handle_runner_export(target, rest, &config, writer)?;
        }
        [command, subcommand, rest @ ..] if command == "runner" && subcommand == "plan" => {
            handle_runner_plan(rest, &runtime, &config, writer)?;
        }
        [command, subcommand, runner_id, flag, out]
            if command == "runner" && subcommand == "pack" && flag == "--out" =>
        {
            let result = runtime.pack_runner(runner_id, &PathBuf::from(out))?;
            writeln!(
                writer,
                "packed: {runner_id} -> {out} using greentic-pack --answers {}",
                result
                    .answers_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "<unknown>".to_owned())
            )?;
            if !result.stdout.trim().is_empty() {
                writer.write_all(result.stdout.as_bytes())?;
            }
            if !result.stderr.trim().is_empty() {
                writer.write_all(result.stderr.as_bytes())?;
            }
        }
        [command, subcommand, path] if command == "runner" && subcommand == "verify-pack" => {
            let result = runtime.verify_runner_pack(&PathBuf::from(path))?;
            writeln!(writer, "verified: {path}")?;
            if !result.stdout.trim().is_empty() {
                writer.write_all(result.stdout.as_bytes())?;
            }
            if !result.stderr.trim().is_empty() {
                writer.write_all(result.stderr.as_bytes())?;
            }
        }
        [command, subcommand, path] if command == "runner" && subcommand == "install-pack" => {
            let installed = runtime.install_runner_pack(&PathBuf::from(path))?;
            writeln!(writer, "installed runner pack: {}", installed.display())?;
        }
        [command, rest @ ..] if command == "record" => {
            handle_record(rest, &config, writer)?;
        }
        [command, subcommand, flag, bind]
            if command == "mcp" && subcommand == "serve" && flag == "--bind" =>
        {
            runtime.serve_mcp(bind)?;
        }
        [command, subcommand] if command == "mcp" && subcommand == "serve" => {
            runtime.serve_mcp(&config.mcp.bind)?;
        }
        _ => return Err(CliError::Usage(usage(require_desktop_prefix))),
    }

    Ok(())
}

fn usage(require_desktop_prefix: bool) -> String {
    let prefix = if require_desktop_prefix {
        "gtc desktop"
    } else {
        "greentic-desktop"
    };
    let gui_command = if require_desktop_prefix {
        ""
    } else {
        "gui [--bind ADDR] [--no-open]|"
    };

    format!(
        "usage: {prefix} <{gui_command}info|init|--import (PATH|file://PATH|oci://REF|store://ID|repo://REF)|--run (PATH|ID) [--input KEY=VALUE] [--inputs-json JSON|--inputs-file PATH]|desktop validate --workflow (PATH|ID) [--input KEY=VALUE] [--expect-file PATH] [--expect-file-changed PATH] [--expect-output KEY=VALUE] [--expect-no-modal] [--json]|--export (PATH|ID) --out PATH|config show|extension search QUERY|extension install ID|extension list|extension info ID|extension versions ID|extension update [ID]|extension remove ID|extension enable ID|extension disable ID|extension health ID|extension verify [ID]|extension sidecar ID|runner list|runner import (PATH|file://PATH|oci://REF|store://ID|repo://REF)|runner run (PATH|ID) [--input KEY=VALUE] [--inputs-json JSON|--inputs-file PATH]|runner validate --workflow (PATH|ID) [--input KEY=VALUE] [--expect-file PATH] [--expect-file-changed PATH] [--expect-output KEY=VALUE] [--expect-no-modal] [--json]|runner export (PATH|ID) --out PATH|runner plan (--prompt TEXT|--prompt-file PATH) [--profile ID] [--context PATH] [--dry-run] [--out PATH]|runner pack ID --out PATH|runner verify-pack PATH|runner install-pack PATH|record <start|pause|resume|stop|cancel|status|list|normalise|finalise|mark-input|mark-secret|mark-output|add-assertion|note>|mcp serve [--bind ADDR]>"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GuiCliOptions {
    bind: SocketAddr,
    open_browser: bool,
}

impl Default for GuiCliOptions {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            open_browser: true,
        }
    }
}

fn parse_gui_args(args: &[String]) -> Result<GuiCliOptions, CliError> {
    let mut options = GuiCliOptions::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--no-open" => {
                options.open_browser = false;
                index += 1;
            }
            "--bind" => {
                let Some(bind) = args.get(index + 1) else {
                    return Err(CliError::Usage("gui --bind requires an address".to_owned()));
                };
                options.bind = bind
                    .parse()
                    .map_err(|_| CliError::Usage(format!("invalid GUI bind address: {bind}")))?;
                index += 2;
            }
            "--help" | "-h" => {
                return Err(CliError::Usage(
                    "usage: greentic-desktop gui [--bind ADDR] [--no-open]".to_owned(),
                ));
            }
            other => return Err(CliError::Usage(format!("unknown gui option: {other}"))),
        }
    }
    Ok(options)
}

fn run_gui(
    options: GuiCliOptions,
    writer: &mut dyn Write,
    block_gui: bool,
) -> Result<(), CliError> {
    let config = RuntimeConfig::default();
    let runtime = DesktopRuntime::new(config.clone());
    runtime.init()?;

    let info = runtime.info();
    let api_state = GuiApiState {
        app_version: info.version,
        platform: info.os,
        runtime_home: config.runner.home.clone(),
        evidence_store: config.evidence.store.clone(),
        mcp_bind: config.mcp.bind.clone(),
        installed_core_adapter_ids: info.installed_adapters,
        installed_extension_ids: discover_extensions(&config.runner.home).unwrap_or_default(),
        runner_names: discover_runners(&config.runner.home).unwrap_or_default(),
        gui_token: gui_session_token(),
    };
    let gui_token = api_state.gui_token.clone();
    let handle = GuiHost::start(GuiHostOptions {
        bind: options.bind,
        api_state,
    })?;
    let url = handle.token_url(&gui_token);
    if !options.bind.ip().is_loopback() {
        writeln!(
            writer,
            "warning: GUI is bound outside loopback; only use this on a trusted local network."
        )?;
    }
    writeln!(writer, "Greentic Automate Hub: {url}")?;
    let log_path = config.runner.home.join("greentic-desktop.log");
    let _ = fs::create_dir_all(&config.runner.home);
    let _ = fs::write(
        &log_path,
        format!(
            "version={}\ngui_url={}\nruntime_home={}\nmcp_bind={}\n",
            env!("CARGO_PKG_VERSION"),
            handle.url(),
            config.runner.home.display(),
            config.mcp.bind
        ),
    );

    if options.open_browser {
        if let Err(err) = open_default_browser(&url) {
            let _ = fs::create_dir_all(&config.runner.home);
            let _ = fs::write(&log_path, format!("browser_open_error={err}\n"));
            writeln!(writer, "warning: {err}")?;
        }
    }

    if block_gui {
        loop {
            thread::sleep(Duration::from_secs(3600));
        }
    }

    drop(handle);
    Ok(())
}

fn gui_session_token() -> String {
    let seed = format!("{:?}-{}", SystemTime::now(), std::process::id());
    format!("{:016x}", fnv1a64(seed.as_bytes()))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn handle_record(
    args: &[String],
    config: &RuntimeConfig,
    writer: &mut dyn Write,
) -> Result<(), CliError> {
    let Some((command, rest)) = args.split_first() else {
        return Err(CliError::Usage("record requires a subcommand".to_owned()));
    };
    match command.as_str() {
        "start" => {
            let name = option_value(rest, "--name")?
                .ok_or_else(|| CliError::Usage("record start requires --name".to_owned()))?;
            let profile = option_value(rest, "--profile")?.unwrap_or_else(|| "default".to_owned());
            let adapter = option_value(rest, "--adapter")?
                .ok_or_else(|| CliError::Usage("record start requires --adapter".to_owned()))?;
            let out = option_value(rest, "--out")?
                .ok_or_else(|| CliError::Usage("record start requires --out".to_owned()))?;
            let manifest = start_recording_session(RecordingStartRequest {
                name,
                profile,
                target_kind: recording_target_kind_for_adapter(&adapter),
                adapter,
                out: PathBuf::from(out),
                runtime_home: config.runner.home.clone(),
                redact: csv_option(option_value(rest, "--redact")?),
                secret_fields: csv_option(option_value(rest, "--secret-fields")?),
            })?;
            writeln!(writer, "session: {}", manifest.session_id)?;
            writeln!(writer, "state: {}", manifest.state.as_str())?;
            writeln!(writer, "path: {}", manifest.root.display())?;
        }
        "pause" => {
            let session = session_arg(rest)?;
            let manifest = pause_recording_session(&config.runner.home, &session)?;
            writeln!(
                writer,
                "{}\t{}",
                manifest.session_id,
                manifest.state.as_str()
            )?;
        }
        "resume" => {
            let session = session_arg(rest)?;
            let manifest = resume_recording_session(&config.runner.home, &session)?;
            writeln!(
                writer,
                "{}\t{}",
                manifest.session_id,
                manifest.state.as_str()
            )?;
        }
        "stop" => {
            let session = session_arg(rest)?;
            let manifest = stop_recording_session(&config.runner.home, &session)?;
            writeln!(
                writer,
                "{}\t{}",
                manifest.session_id,
                manifest.state.as_str()
            )?;
        }
        "cancel" => {
            let session = session_arg(rest)?;
            let manifest = cancel_recording_session(&config.runner.home, &session)?;
            writeln!(
                writer,
                "{}\t{}",
                manifest.session_id,
                manifest.state.as_str()
            )?;
        }
        "status" => {
            let session = session_arg(rest)?;
            let manifest = load_recording_session(&config.runner.home, &session)?;
            writeln!(
                writer,
                "{}\t{}\t{}",
                manifest.session_id,
                manifest.state.as_str(),
                manifest.root.display()
            )?;
        }
        "list" => {
            for manifest in list_recording_sessions(&config.runner.home)? {
                writeln!(
                    writer,
                    "{}\t{}\t{}",
                    manifest.session_id,
                    manifest.state.as_str(),
                    manifest.name
                )?;
            }
        }
        "normalise" => {
            let recording = option_value(rest, "--recording")?.ok_or_else(|| {
                CliError::Usage("record normalise requires --recording".to_owned())
            })?;
            let out = option_value(rest, "--out")?
                .ok_or_else(|| CliError::Usage("record normalise requires --out".to_owned()))?;
            let package = normalise_recording(&PathBuf::from(recording), &PathBuf::from(out))?;
            writeln!(writer, "normalised: {}", package.id)?;
        }
        "finalise" => {
            let recording = option_value(rest, "--recording")?.ok_or_else(|| {
                CliError::Usage("record finalise requires --recording".to_owned())
            })?;
            let runner = option_value(rest, "--runner")?
                .ok_or_else(|| CliError::Usage("record finalise requires --runner".to_owned()))?;
            let out = finalise_recording(&PathBuf::from(recording), &PathBuf::from(runner))?;
            writeln!(writer, "finalised: {}", out.display())?;
        }
        "mark-input" | "mark-secret" | "mark-output" | "add-assertion" | "note" => {
            let session = option_value(rest, "--session")?.unwrap_or_else(|| ".".to_owned());
            let value = rest
                .iter()
                .rev()
                .find(|value| !value.starts_with("--") && **value != session)
                .ok_or_else(|| CliError::Usage(format!("record {command} requires a value")))?;
            append_recording_note(&config.runner.home, &session, command, value)?;
            writeln!(writer, "marked: {command}")?;
        }
        other => return Err(CliError::Usage(format!("unknown record command: {other}"))),
    }
    Ok(())
}

fn recording_target_kind_for_adapter(adapter: &str) -> RecordingTargetKind {
    if adapter.contains("terminal") || adapter.contains("tn3270") {
        RecordingTargetKind::Terminal
    } else if adapter.contains("java") {
        RecordingTargetKind::Java
    } else if adapter.contains("vision") {
        RecordingTargetKind::Desktop
    } else {
        RecordingTargetKind::Web
    }
}

fn handle_runner_import(
    source: &str,
    runtime: &DesktopRuntime,
    config: &RuntimeConfig,
    writer: &mut dyn Write,
) -> Result<(), CliError> {
    let import_path = resolve_runner_import_source(source, runtime)?;
    let imported =
        import_runner_yaml_file(&config.runner.home, &import_path).map_err(CliError::Usage)?;
    writeln!(
        writer,
        "imported: {}\t{}\t{}",
        imported.runner_id,
        imported.runner_name,
        imported.path.display()
    )?;
    Ok(())
}

fn resolve_runner_import_source(
    source: &str,
    runtime: &DesktopRuntime,
) -> Result<PathBuf, CliError> {
    if let Some(path) = source.strip_prefix("file://") {
        return Ok(PathBuf::from(path));
    }
    if !source.contains("://") {
        return Ok(PathBuf::from(source));
    }
    let artifact = runtime.resolve_runner_source(source)?;
    if artifact.local_path.exists() {
        Ok(artifact.local_path)
    } else {
        Err(CliError::Usage(format!(
            "resolved runner source {} to {} but the YAML artifact is not present in the local distributor cache. Expected cached file: {}",
            artifact.source_uri,
            artifact.resolved_uri,
            artifact.local_path.display()
        )))
    }
}

fn handle_runner_run(
    target: &str,
    args: &[String],
    config: &RuntimeConfig,
    writer: &mut dyn Write,
) -> Result<(), CliError> {
    let inputs_json = runner_run_inputs_json(args)?;
    let execution =
        run_runner_yaml(&config.runner.home, target, &inputs_json).map_err(CliError::Usage)?;
    writeln!(writer, "runner: {}", execution.runner_id)?;
    writeln!(writer, "status: {}", execution.status)?;
    writeln!(writer, "evidence: {}", execution.evidence_ref)?;
    writeln!(writer, "outputs: {}", execution.outputs_json)?;
    writeln!(writer, "steps: {}", execution.steps_json)?;
    Ok(())
}

#[derive(Debug, Clone)]
struct DesktopValidateOptions {
    workflow: String,
    input_args: Vec<String>,
    expect_files: Vec<PathBuf>,
    expect_file_changed: Vec<PathBuf>,
    expect_outputs: Vec<(String, String)>,
    expect_no_modal: bool,
    expect_frontmost_app: Option<String>,
    json: bool,
}

#[derive(Debug, Clone)]
struct LiveFileState {
    exists: bool,
    modified: Option<SystemTime>,
    size: Option<u64>,
}

impl LiveFileState {
    fn capture(path: &Path) -> Self {
        match fs::metadata(path) {
            Ok(metadata) => Self {
                exists: true,
                modified: metadata.modified().ok(),
                size: Some(metadata.len()),
            },
            Err(_) => Self {
                exists: false,
                modified: None,
                size: None,
            },
        }
    }

    fn changed_from(&self, before: &Self) -> bool {
        self.exists
            && (!before.exists || self.modified != before.modified || self.size != before.size)
    }
}

fn handle_desktop_validate(
    args: &[String],
    config: &RuntimeConfig,
    writer: &mut dyn Write,
) -> Result<(), CliError> {
    let options = parse_desktop_validate_args(args)?;
    let before = options
        .expect_file_changed
        .iter()
        .map(|path| (path.clone(), LiveFileState::capture(path)))
        .collect::<Vec<_>>();
    let inputs_json = runner_run_inputs_json(&options.input_args)?;
    let execution =
        run_runner_yaml(&config.runner.home, &options.workflow, &inputs_json).map_err(|err| {
            CliError::Usage(format!("live validation runner execution failed: {err}"))
        })?;
    let mut failures = Vec::new();
    let outputs = serde_json::from_str::<serde_json::Value>(&execution.outputs_json)
        .unwrap_or_else(|_| json!({}));

    for path in &options.expect_files {
        if !path.exists() {
            failures.push(format!("expected file does not exist: {}", path.display()));
        }
    }
    for (path, before_state) in &before {
        let after = LiveFileState::capture(path);
        if !after.changed_from(before_state) {
            failures.push(format!(
                "expected file was not created or changed: {}",
                path.display()
            ));
        }
    }
    for (key, expected) in &options.expect_outputs {
        let actual = outputs.get(key).and_then(|value| value.as_str());
        if actual != Some(expected.as_str()) {
            failures.push(format!(
                "expected output {key}={expected}, got {}",
                actual.unwrap_or("<missing>")
            ));
        }
    }
    let modal = if options.expect_no_modal {
        live_modal_summary()
    } else {
        LiveModalSummary::default()
    };
    if options.expect_no_modal && modal.blocking {
        failures.push(format!(
            "blocking modal remains: {}",
            modal.summary.as_deref().unwrap_or("unknown modal")
        ));
    }
    let frontmost = live_frontmost_app();
    if let Some(expected) = &options.expect_frontmost_app {
        match &frontmost {
            Some(actual) if actual.eq_ignore_ascii_case(expected) => {}
            Some(actual) => {
                failures.push(format!("expected frontmost app {expected}, got {actual}"))
            }
            None => failures.push(format!("expected frontmost app {expected}, got <unknown>")),
        }
    }

    let passed = failures.is_empty();
    if options.json {
        let summary = json!({
            "ok": passed,
            "runnerId": execution.runner_id,
            "status": if passed { "passed" } else { "failed" },
            "evidenceRef": execution.evidence_ref,
            "outputs": outputs,
            "steps": serde_json::from_str::<serde_json::Value>(&execution.steps_json).unwrap_or_else(|_| json!([])),
            "liveAssertions": {
                "expectFiles": options.expect_files.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
                "expectFileChanged": options.expect_file_changed.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
                "expectOutputs": options.expect_outputs.iter().map(|(key, value)| format!("{key}={value}")).collect::<Vec<_>>(),
                "expectNoModal": options.expect_no_modal,
                "expectFrontmostApp": options.expect_frontmost_app,
            },
            "liveState": {
                "frontmostApp": frontmost,
                "modal": {
                    "blocking": modal.blocking,
                    "summary": modal.summary,
                }
            },
            "failures": failures,
        });
        writeln!(writer, "{summary}")?;
    } else {
        writeln!(writer, "runner: {}", execution.runner_id)?;
        writeln!(
            writer,
            "status: {}",
            if passed { "passed" } else { "failed" }
        )?;
        writeln!(writer, "evidence: {}", execution.evidence_ref)?;
        writeln!(writer, "outputs: {}", execution.outputs_json)?;
        if let Some(frontmost) = frontmost {
            writeln!(writer, "frontmost_app: {frontmost}")?;
        }
        if options.expect_no_modal {
            writeln!(
                writer,
                "modal: {}",
                modal.summary.as_deref().unwrap_or("none")
            )?;
        }
        for failure in &failures {
            writeln!(writer, "validation_error: {failure}")?;
        }
    }

    if passed {
        Ok(())
    } else {
        Err(CliError::Usage(format!(
            "live validation failed: {}",
            failures.join("; ")
        )))
    }
}

fn parse_desktop_validate_args(args: &[String]) -> Result<DesktopValidateOptions, CliError> {
    let mut workflow = None;
    let mut input_args = Vec::new();
    let mut expect_files = Vec::new();
    let mut expect_file_changed = Vec::new();
    let mut expect_outputs = Vec::new();
    let mut expect_no_modal = false;
    let mut expect_frontmost_app = None;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--workflow" | "--runner" => {
                index += 1;
                workflow = Some(required_arg(args, index, "--workflow")?.to_owned());
            }
            "--input" => {
                index += 1;
                input_args.push("--input".to_owned());
                input_args.push(required_arg(args, index, "--input")?.to_owned());
            }
            "--inputs-json" => {
                index += 1;
                input_args.push("--inputs-json".to_owned());
                input_args.push(required_arg(args, index, "--inputs-json")?.to_owned());
            }
            "--inputs-file" => {
                index += 1;
                input_args.push("--inputs-file".to_owned());
                input_args.push(required_arg(args, index, "--inputs-file")?.to_owned());
            }
            "--expect-file" => {
                index += 1;
                expect_files.push(PathBuf::from(required_arg(args, index, "--expect-file")?));
            }
            "--expect-file-changed" => {
                index += 1;
                expect_file_changed.push(PathBuf::from(required_arg(
                    args,
                    index,
                    "--expect-file-changed",
                )?));
            }
            "--expect-output" => {
                index += 1;
                let raw = required_arg(args, index, "--expect-output")?;
                let (key, value) = raw.split_once('=').ok_or_else(|| {
                    CliError::Usage("--expect-output requires KEY=VALUE".to_owned())
                })?;
                expect_outputs.push((key.to_owned(), value.to_owned()));
            }
            "--expect-no-modal" => expect_no_modal = true,
            "--expect-frontmost-app" => {
                index += 1;
                expect_frontmost_app =
                    Some(required_arg(args, index, "--expect-frontmost-app")?.to_owned());
            }
            "--json" => json = true,
            "--help" | "-h" => {
                return Err(CliError::Usage(
                    "usage: greentic-desktop desktop validate --workflow (PATH|ID) [--input KEY=VALUE] [--expect-file PATH] [--expect-file-changed PATH] [--expect-output KEY=VALUE] [--expect-no-modal] [--json]".to_owned(),
                ));
            }
            value if !value.starts_with("--") && workflow.is_none() => {
                workflow = Some(value.to_owned());
            }
            other => {
                return Err(CliError::Usage(format!(
                    "unknown desktop validate argument: {other}"
                )))
            }
        }
        index += 1;
    }
    Ok(DesktopValidateOptions {
        workflow: workflow.ok_or_else(|| {
            CliError::Usage("desktop validate requires --workflow PATH_OR_ID".to_owned())
        })?,
        input_args,
        expect_files,
        expect_file_changed,
        expect_outputs,
        expect_no_modal,
        expect_frontmost_app,
        json,
    })
}

#[derive(Debug, Clone, Default)]
struct LiveModalSummary {
    blocking: bool,
    summary: Option<String>,
}

fn live_modal_summary() -> LiveModalSummary {
    #[cfg(target_os = "macos")]
    {
        macos_live_modal_summary()
    }
    #[cfg(not(target_os = "macos"))]
    {
        LiveModalSummary::default()
    }
}

fn live_frontmost_app() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        greentic_desktop_macos::macos_live_frontmost_app()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn macos_live_modal_summary() -> LiveModalSummary {
    let summary = greentic_desktop_macos::macos_live_modal_summary();
    LiveModalSummary {
        blocking: summary.blocking,
        summary: summary.summary,
    }
}

fn handle_runner_export(
    target: &str,
    args: &[String],
    config: &RuntimeConfig,
    writer: &mut dyn Write,
) -> Result<(), CliError> {
    let out = runner_export_out(args)?;
    let exported =
        export_runner_yaml_file(&config.runner.home, target, &out).map_err(CliError::Usage)?;
    writeln!(
        writer,
        "exported: {}\t{}\t{}",
        exported.runner_id,
        exported.runner_name,
        exported.path.display()
    )?;
    Ok(())
}

fn runner_export_out(args: &[String]) -> Result<PathBuf, CliError> {
    let mut out = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                index += 1;
                out = Some(PathBuf::from(required_arg(args, index, "--out")?));
            }
            "--help" | "-h" => {
                return Err(CliError::Usage(
                    "usage: greentic-desktop --export (PATH|ID) --out PATH".to_owned(),
                ));
            }
            other => {
                return Err(CliError::Usage(format!(
                    "unknown runner export argument: {other}"
                )))
            }
        }
        index += 1;
    }
    out.ok_or_else(|| CliError::Usage("runner export requires --out PATH".to_owned()))
}

fn runner_run_inputs_json(args: &[String]) -> Result<String, CliError> {
    let mut fields = Vec::new();
    let mut raw_json = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--input" => {
                if raw_json.is_some() {
                    return Err(CliError::Usage(
                        "use either --input, --inputs-json, or --inputs-file".to_owned(),
                    ));
                }
                index += 1;
                let value = required_arg(args, index, "--input")?;
                let (key, value) = value
                    .split_once('=')
                    .ok_or_else(|| CliError::Usage("--input requires KEY=VALUE".to_owned()))?;
                if key.trim().is_empty() {
                    return Err(CliError::Usage("--input key must not be empty".to_owned()));
                }
                fields.push((key.trim().to_owned(), value.to_owned()));
            }
            "--inputs-json" => {
                index += 1;
                if raw_json.is_some() || !fields.is_empty() {
                    return Err(CliError::Usage(
                        "use either --input, --inputs-json, or --inputs-file".to_owned(),
                    ));
                }
                raw_json = Some(required_arg(args, index, "--inputs-json")?.to_owned());
            }
            "--inputs-file" => {
                index += 1;
                if raw_json.is_some() || !fields.is_empty() {
                    return Err(CliError::Usage(
                        "use either --input, --inputs-json, or --inputs-file".to_owned(),
                    ));
                }
                raw_json = Some(fs::read_to_string(required_arg(
                    args,
                    index,
                    "--inputs-file",
                )?)?);
            }
            "--help" | "-h" => {
                return Err(CliError::Usage(
                    "usage: greentic-desktop --run (PATH|ID) [--input KEY=VALUE] [--inputs-json JSON|--inputs-file PATH]".to_owned(),
                ));
            }
            other => {
                return Err(CliError::Usage(format!(
                    "unknown runner run argument: {other}"
                )))
            }
        }
        index += 1;
    }
    if let Some(raw_json) = raw_json {
        let trimmed = raw_json.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            return Err(CliError::Usage(
                "--inputs-json/--inputs-file must contain a JSON object".to_owned(),
            ));
        }
        return Ok(trimmed.to_owned());
    }
    Ok(format!(
        "{{{}}}",
        fields
            .into_iter()
            .map(|(key, value)| format!(
                "\"{}\":\"{}\"",
                escape_cli_json(&key),
                escape_cli_json(&value)
            ))
            .collect::<Vec<_>>()
            .join(",")
    ))
}

fn escape_cli_json(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}

fn handle_runner_plan(
    args: &[String],
    runtime: &DesktopRuntime,
    config: &RuntimeConfig,
    writer: &mut dyn Write,
) -> Result<(), CliError> {
    let mut prompt = None;
    let mut prompt_file = None;
    let mut profile = None;
    let mut context_file = None;
    let mut out = None;
    let mut dry_run = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--prompt" => {
                index += 1;
                prompt = Some(required_arg(args, index, "--prompt")?.to_owned());
            }
            "--prompt-file" => {
                index += 1;
                prompt_file = Some(PathBuf::from(required_arg(args, index, "--prompt-file")?));
            }
            "--profile" => {
                index += 1;
                profile = Some(required_arg(args, index, "--profile")?.to_owned());
            }
            "--context" => {
                index += 1;
                context_file = Some(PathBuf::from(required_arg(args, index, "--context")?));
            }
            "--out" => {
                index += 1;
                out = Some(PathBuf::from(required_arg(args, index, "--out")?));
            }
            "--dry-run" => dry_run = true,
            other => {
                return Err(CliError::Usage(format!(
                    "unknown runner plan argument: {other}"
                )))
            }
        }
        index += 1;
    }

    let prompt = match (prompt, prompt_file) {
        (Some(value), None) => value,
        (None, Some(path)) => fs::read_to_string(path)?,
        (Some(_), Some(_)) => {
            return Err(CliError::Usage(
                "use either --prompt or --prompt-file, not both".to_owned(),
            ))
        }
        (None, None) => {
            return Err(CliError::Usage(
                "runner plan requires --prompt or --prompt-file".to_owned(),
            ))
        }
    };

    let mut planning_context = planning_context(runtime, config)?;
    if let Some(path) = context_file {
        planning_context
            .desktop_observations
            .push(fs::read_to_string(path)?);
    }
    let options = PlannerOptions {
        profile,
        dry_run,
        ..PlannerOptions::default()
    };
    let result = plan_prompt_with_default_llm(&prompt, &planning_context, &options)?;
    writeln!(writer, "planned: {}", result.draft.package.id)?;
    if !result.draft.open_questions.is_empty() {
        writeln!(writer, "open_questions:")?;
        for question in &result.draft.open_questions {
            writeln!(writer, "  - {question}")?;
        }
    }
    if options.dry_run {
        writer.write_all(result.draft.render_yaml().as_bytes())?;
    } else {
        let path = out.ok_or_else(|| {
            CliError::Usage("runner plan requires --out unless --dry-run is set".to_owned())
        })?;
        save_draft_runner(&result.draft, &path)?;
        writeln!(writer, "written: {}", path.display())?;
    }
    Ok(())
}

fn planning_context(
    runtime: &DesktopRuntime,
    config: &RuntimeConfig,
) -> Result<PlanningContext, CliError> {
    let mut available_adapters = runtime.installed_adapters().to_vec();
    available_adapters.extend(
        runtime
            .list_extensions()?
            .into_iter()
            .map(|manifest| manifest.adapter_capabilities()),
    );
    Ok(PlanningContext {
        available_adapters,
        available_mcp_tools: vec!["tools/list".to_owned()],
        application_metadata: Vec::new(),
        existing_runners: discover_runners(&config.runner.home)?,
        ltm_examples: Vec::new(),
        security_policies: vec!["no critical drafts without approval".to_owned()],
        desktop_observations: Vec::new(),
    })
}

fn option_value(args: &[String], flag: &str) -> Result<Option<String>, CliError> {
    let mut index = 0;
    while index < args.len() {
        if args[index] == flag {
            return args
                .get(index + 1)
                .cloned()
                .map(Some)
                .ok_or_else(|| CliError::Usage(format!("{flag} requires a value")));
        }
        index += 1;
    }
    Ok(None)
}

fn session_arg(args: &[String]) -> Result<String, CliError> {
    option_value(args, "--session")?
        .ok_or_else(|| CliError::Usage("record command requires --session".to_owned()))
}

fn csv_option(value: Option<String>) -> Vec<String> {
    value
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn required_arg<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, CliError> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| CliError::Usage(format!("{flag} requires a value")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_home<T>(f: impl FnOnce(PathBuf) -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let old_home = std::env::var_os("GREENTIC_DESKTOP_HOME");
        let home = std::env::temp_dir().join(format!(
            "greentic-cli-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        std::env::set_var("GREENTIC_DESKTOP_HOME", &home);
        let result = f(home.clone());
        restore_home(old_home);
        if home.exists() {
            fs::remove_dir_all(home).expect("test home should be removable");
        }
        result
    }

    fn restore_home(old_home: Option<OsString>) {
        if let Some(value) = old_home {
            std::env::set_var("GREENTIC_DESKTOP_HOME", value);
        } else {
            std::env::remove_var("GREENTIC_DESKTOP_HOME");
        }
    }

    fn install_fake_greentic_pack(home: &std::path::Path) -> (PathBuf, Option<OsString>) {
        let bin = home.join("bin");
        fs::create_dir_all(&bin).expect("fake bin dir");
        let fake = bin.join("greentic-pack");
        fs::write(
            &fake,
            r#"#!/bin/sh
printf '%s\n' "$*" > "$GREENTIC_FAKE_PACK_ARGS"
if [ "$1" = "--answers" ]; then
  cp "$2" "$GREENTIC_FAKE_PACK_ANSWERS"
  exit 0
fi
if [ "$1" = "verify" ]; then
  exit 0
fi
echo "unexpected greentic-pack invocation: $*" >&2
exit 2
"#,
        )
        .expect("fake greentic-pack should write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&fake, fs::Permissions::from_mode(0o755))
                .expect("fake greentic-pack should be executable");
        }
        let old_path = std::env::var_os("PATH");
        let next_path = match &old_path {
            Some(path) => {
                let mut paths = std::env::split_paths(path).collect::<Vec<_>>();
                paths.insert(0, bin);
                std::env::join_paths(paths).expect("PATH should join")
            }
            None => OsString::from(bin),
        };
        std::env::set_var("PATH", next_path);
        (fake, old_path)
    }

    fn restore_path(old_path: Option<OsString>) {
        if let Some(value) = old_path {
            std::env::set_var("PATH", value);
        } else {
            std::env::remove_var("PATH");
        }
    }

    #[test]
    fn desktop_info_prints_required_fields() {
        with_temp_home(|_| {
            let mut output = Vec::new();
            run_with_writer(["info".to_owned()], false, &mut output).expect("info should succeed");
            let output = String::from_utf8(output).expect("output should be utf8");

            assert!(output.contains("version:"));
            assert!(output.contains("os:"));
            assert!(output.contains("adapters:"));
            assert!(output.contains("registry:"));
        });
    }

    #[test]
    fn gtc_requires_desktop_prefix() {
        with_temp_home(|_| {
            let mut output = Vec::new();
            let err = run_with_writer(["info".to_owned()], true, &mut output)
                .expect_err("gtc requires desktop prefix");

            assert!(err.to_string().contains("gtc desktop"));
        });
    }

    #[test]
    fn gtc_desktop_config_show_prints_runtime_config() {
        with_temp_home(|_| {
            let mut output = Vec::new();
            run_with_writer(
                ["desktop".to_owned(), "config".to_owned(), "show".to_owned()],
                true,
                &mut output,
            )
            .expect("config show should succeed");
            let output = String::from_utf8(output).expect("output should be utf8");

            assert!(output.contains("[runner]"));
            assert!(output.contains("[security]"));
        });
    }

    #[test]
    fn verifies_known_extension_manifest() {
        with_temp_home(|_| {
            let mut output = Vec::new();
            run_with_writer(
                [
                    "desktop".to_owned(),
                    "extension".to_owned(),
                    "verify".to_owned(),
                    "greentic.desktop.playwright".to_owned(),
                ],
                true,
                &mut output,
            )
            .expect("known signed extension should verify");

            let output = String::from_utf8(output).expect("output should be utf8");
            assert!(output.contains("verified: greentic.desktop.playwright"));
        });
    }

    #[test]
    fn empty_desktop_args_start_gui_in_nonblocking_test_mode() {
        with_temp_home(|home| {
            let mut output = Vec::new();
            run_with_writer(Vec::<String>::new(), false, &mut output)
                .expect("default desktop invocation should start GUI");
            let output = String::from_utf8(output).expect("output should be utf8");

            assert!(output.contains("Greentic Automate Hub: http://127.0.0.1:"));
            assert!(home.join("extensions").is_dir());
        });
    }

    #[test]
    fn explicit_gui_no_open_and_help_dispatch() {
        with_temp_home(|_| {
            let mut output = Vec::new();
            run_with_writer(
                [
                    "gui".to_owned(),
                    "--no-open".to_owned(),
                    "--bind".to_owned(),
                    "127.0.0.1:0".to_owned(),
                ],
                false,
                &mut output,
            )
            .expect("explicit GUI command should start");
            assert!(String::from_utf8(output)
                .expect("output should be utf8")
                .contains("Greentic Automate Hub: http://127.0.0.1:"));

            let mut help = Vec::new();
            run_with_writer(["--help".to_owned()], false, &mut help).expect("help should print");
            let help = String::from_utf8(help).expect("help should be utf8");
            assert!(help.contains("greentic-desktop"));
            assert!(help.contains("gui [--bind ADDR] [--no-open]"));
        });
    }

    #[test]
    fn gtc_desktop_without_subcommand_prints_usage() {
        with_temp_home(|_| {
            let mut output = Vec::new();
            let err = run_with_writer(["desktop".to_owned()], true, &mut output)
                .expect_err("gtc desktop needs a subcommand");

            assert!(err.to_string().contains("gtc desktop"));
            assert!(!err.to_string().contains("gui [--bind"));
        });
    }

    #[test]
    fn init_extension_and_runner_commands_use_runtime_home() {
        with_temp_home(|home| {
            let mut output = Vec::new();
            run_with_writer(["init".to_owned()], false, &mut output).expect("init should succeed");
            assert!(home.join("extensions").is_dir());

            output.clear();
            run_with_writer(
                [
                    "extension".to_owned(),
                    "search".to_owned(),
                    "browser".to_owned(),
                ],
                false,
                &mut output,
            )
            .expect("extension search should succeed");
            assert!(String::from_utf8(output.clone())
                .expect("output should be utf8")
                .contains("greentic.desktop.playwright"));

            output.clear();
            run_with_writer(
                [
                    "extension".to_owned(),
                    "versions".to_owned(),
                    "playwright".to_owned(),
                ],
                false,
                &mut output,
            )
            .expect("extension versions should succeed");
            assert!(String::from_utf8(output.clone())
                .expect("output should be utf8")
                .contains("1.0.0"));

            output.clear();
            run_with_writer(
                [
                    "extension".to_owned(),
                    "install".to_owned(),
                    "playwright".to_owned(),
                ],
                false,
                &mut output,
            )
            .expect("extension install should succeed");

            output.clear();
            run_with_writer(
                ["extension".to_owned(), "list".to_owned()],
                false,
                &mut output,
            )
            .expect("extension list should succeed");
            let listed = String::from_utf8(output.clone()).expect("output should be utf8");
            assert!(listed.contains("greentic.desktop.playwright"));

            output.clear();
            run_with_writer(
                ["extension".to_owned(), "update".to_owned()],
                false,
                &mut output,
            )
            .expect("extension update should verify installed extensions");
            assert!(String::from_utf8(output.clone())
                .expect("output should be utf8")
                .contains("checked: 1 extensions"));

            output.clear();
            run_with_writer(
                [
                    "extension".to_owned(),
                    "sidecar".to_owned(),
                    "greentic.desktop.playwright".to_owned(),
                ],
                false,
                &mut output,
            )
            .expect("sidecar metadata should render");
            assert!(String::from_utf8(output.clone())
                .expect("output should be utf8")
                .contains("sidecar: greentic.desktop.playwright node ./index.js"));

            let runners = home.join("runners");
            fs::create_dir_all(&runners).expect("runner dir should be created");
            fs::write(runners.join("demo.gtpack"), "runner").expect("runner should write");
            output.clear();
            run_with_writer(["runner".to_owned(), "list".to_owned()], false, &mut output)
                .expect("runner list should succeed");
            assert!(String::from_utf8(output)
                .expect("output should be utf8")
                .contains("demo.gtpack"));
        });
    }

    #[test]
    fn runner_import_and_run_yaml_cli_resolve_runner_manifests() {
        with_temp_home(|home| {
            fs::create_dir_all(&home).expect("temp home should exist");
            let source = home.join("word-example.yaml");
            fs::write(
                &source,
                "id: example.word\nname: Example Word\nversion: 0.1.0\ninputs:\n  - inputs.document_path\noutputs: []\nsteps:\n  - id: open-word\n    action: activate_app\n    required_capability: macos.activate_app\n    value: \"Microsoft Word\"\n",
            )
            .expect("source yaml");

            let mut output = Vec::new();
            run_with_writer(
                ["--import".to_owned(), source.display().to_string()],
                false,
                &mut output,
            )
            .expect("runner import should succeed");
            let imported = String::from_utf8(output.clone()).expect("output should be utf8");
            assert!(imported.contains("imported: example.word"), "{imported}");
            assert!(home.join("runners").join("example.word.yaml").exists());

            output.clear();
            let path_err = run_with_writer(
                ["--run".to_owned(), source.display().to_string()],
                false,
                &mut output,
            )
            .expect_err("missing input should fail before desktop execution");
            assert!(path_err.to_string().contains("runner.input_missing"));

            output.clear();
            let id_err = run_with_writer(
                [
                    "runner".to_owned(),
                    "run".to_owned(),
                    "example.word".to_owned(),
                ],
                false,
                &mut output,
            )
            .expect_err("imported runner id should resolve and validate inputs");
            assert!(id_err.to_string().contains("runner.input_missing"));

            output.clear();
            let export_path = home.join("exports").join("word.yaml");
            run_with_writer(
                [
                    "--export".to_owned(),
                    "example.word".to_owned(),
                    "--out".to_owned(),
                    export_path.display().to_string(),
                ],
                false,
                &mut output,
            )
            .expect("runner export should write YAML");
            assert!(export_path.exists());
            assert!(fs::read_to_string(export_path)
                .expect("exported yaml")
                .contains("id: example.word"));

            let uri_err = run_with_writer(
                [
                    "runner".to_owned(),
                    "import".to_owned(),
                    "store://example.word".to_owned(),
                ],
                false,
                &mut output,
            )
            .expect_err("uri import should require cached artifact");
            assert!(uri_err.to_string().contains("local distributor cache"));
        });
    }

    #[test]
    fn desktop_validate_parser_accepts_workflow_inputs_and_assertions() {
        let args = [
            "--workflow".to_owned(),
            "examples/runners/demo.yaml".to_owned(),
            "--input".to_owned(),
            "inputs.name=Maarten".to_owned(),
            "--expect-file".to_owned(),
            "/tmp/out.txt".to_owned(),
            "--expect-file-changed".to_owned(),
            "/tmp/out.txt".to_owned(),
            "--expect-output".to_owned(),
            "outputs.result=done".to_owned(),
            "--expect-no-modal".to_owned(),
            "--expect-frontmost-app".to_owned(),
            "Microsoft Excel".to_owned(),
            "--json".to_owned(),
        ];

        let options = parse_desktop_validate_args(&args).expect("validate args");

        assert_eq!(options.workflow, "examples/runners/demo.yaml");
        assert_eq!(
            options.input_args,
            vec!["--input".to_owned(), "inputs.name=Maarten".to_owned()]
        );
        assert_eq!(options.expect_files, vec![PathBuf::from("/tmp/out.txt")]);
        assert_eq!(
            options.expect_file_changed,
            vec![PathBuf::from("/tmp/out.txt")]
        );
        assert_eq!(
            options.expect_outputs,
            vec![("outputs.result".to_owned(), "done".to_owned())]
        );
        assert!(options.expect_no_modal);
        assert_eq!(
            options.expect_frontmost_app.as_deref(),
            Some("Microsoft Excel")
        );
        assert!(options.json);
    }

    #[test]
    fn live_file_state_detects_created_or_modified_files() {
        let path = std::env::temp_dir().join(format!(
            "greentic-live-file-state-{}.txt",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let before = LiveFileState::capture(&path);

        fs::write(&path, "created").expect("write file");
        let after_create = LiveFileState::capture(&path);
        assert!(after_create.changed_from(&before));

        let same = LiveFileState::capture(&path);
        assert!(!same.changed_from(&after_create));

        fs::write(&path, "created and changed").expect("rewrite file");
        let after_change = LiveFileState::capture(&path);
        assert!(after_change.changed_from(&same));

        fs::remove_file(path).expect("remove file");
    }

    #[test]
    fn unknown_extension_and_standalone_mcp_serve_return_errors() {
        with_temp_home(|_| {
            let mut output = Vec::new();
            let err = run_with_writer(
                [
                    "extension".to_owned(),
                    "verify".to_owned(),
                    "greentic.desktop.missing".to_owned(),
                ],
                false,
                &mut output,
            )
            .expect_err("unknown extension should fail");
            assert!(err.to_string().contains("unknown extension"));

            let err = run_with_writer(
                [
                    "mcp".to_owned(),
                    "serve".to_owned(),
                    "--bind".to_owned(),
                    "not-an-address".to_owned(),
                ],
                false,
                &mut output,
            )
            .expect_err("standalone mcp serve should fail");
            assert!(
                err.to_string()
                    .contains("standalone CLI MCP server is disabled"),
                "{err}"
            );
        });
    }

    #[test]
    fn runner_plan_writes_draft_package() {
        with_temp_home(|home| {
            let mut output = Vec::new();
            run_with_writer(["init".to_owned()], false, &mut output).expect("init");
            run_with_writer(
                [
                    "extension".to_owned(),
                    "install".to_owned(),
                    "greentic.desktop.playwright".to_owned(),
                ],
                false,
                &mut output,
            )
            .expect("install extension");

            let out = home.join("runners").join("crm.create_customer.draft.yaml");
            output.clear();
            run_with_writer(
                [
                    "runner".to_owned(),
                    "plan".to_owned(),
                    "--prompt".to_owned(),
                    "Create CRM customer with company name and email and return customer id"
                        .to_owned(),
                    "--profile".to_owned(),
                    "local-crm".to_owned(),
                    "--out".to_owned(),
                    out.display().to_string(),
                ],
                false,
                &mut output,
            )
            .expect("runner plan");

            let written = fs::read_to_string(out).expect("draft should write");
            assert!(written.contains("id: crm.create_customer"));
            assert!(String::from_utf8(output)
                .expect("output utf8")
                .contains("written:"));
        });
    }

    #[test]
    fn runner_pack_invokes_greentic_pack_with_answers_json() {
        with_temp_home(|home| {
            let (_fake, old_path) = install_fake_greentic_pack(&home);
            let args_file = home.join("pack.args");
            let answers_copy = home.join("answers.copy.json");
            std::env::set_var("GREENTIC_FAKE_PACK_ARGS", &args_file);
            std::env::set_var("GREENTIC_FAKE_PACK_ANSWERS", &answers_copy);
            let runners = home.join("runners");
            fs::create_dir_all(&runners).expect("runners dir");
            fs::write(
                runners.join("generic.web.append_row.runner.json"),
                r#"{"schema_version":"greentic.runner.v1","runner_definition":{"runner_id":"generic.web.append_row"}}"#,
            )
            .expect("runner manifest");

            let out = home.join("generic.web.append_row.gtpack");
            let mut output = Vec::new();
            run_with_writer(
                [
                    "runner".to_owned(),
                    "pack".to_owned(),
                    "generic.web.append_row".to_owned(),
                    "--out".to_owned(),
                    out.display().to_string(),
                ],
                false,
                &mut output,
            )
            .expect("runner pack should delegate to greentic-pack");

            let rendered = String::from_utf8(output).expect("output should be utf8");
            assert!(
                rendered.contains("using greentic-pack --answers"),
                "{rendered}"
            );
            let args = fs::read_to_string(args_file).expect("fake args");
            assert!(args.starts_with("--answers "), "{args}");
            let answers = fs::read_to_string(answers_copy).expect("answers copy");
            assert!(answers.contains(r#""schema_version": "greentic.pack.answers.v1""#));
            assert!(answers.contains(r#""runner_id": "generic.web.append_row""#));
            assert!(answers.contains("generic.web.append_row.runner.json"));
            assert!(answers.contains(&out.display().to_string()));

            restore_path(old_path);
            std::env::remove_var("GREENTIC_FAKE_PACK_ARGS");
            std::env::remove_var("GREENTIC_FAKE_PACK_ANSWERS");
        });
    }

    #[test]
    fn runner_verify_and_install_pack_delegate_to_greentic_pack() {
        with_temp_home(|home| {
            let (_fake, old_path) = install_fake_greentic_pack(&home);
            let args_file = home.join("pack.args");
            let answers_copy = home.join("answers.copy.json");
            std::env::set_var("GREENTIC_FAKE_PACK_ARGS", &args_file);
            std::env::set_var("GREENTIC_FAKE_PACK_ANSWERS", &answers_copy);
            let pack = home.join("example.gtpack");
            fs::write(&pack, "pack").expect("pack file");

            let mut output = Vec::new();
            run_with_writer(
                [
                    "runner".to_owned(),
                    "verify-pack".to_owned(),
                    pack.display().to_string(),
                ],
                false,
                &mut output,
            )
            .expect("verify-pack should delegate to greentic-pack");
            assert!(String::from_utf8(output.clone())
                .expect("output")
                .contains("verified:"));
            assert!(fs::read_to_string(&args_file)
                .expect("args")
                .starts_with("verify "));

            output.clear();
            run_with_writer(
                [
                    "runner".to_owned(),
                    "install-pack".to_owned(),
                    pack.display().to_string(),
                ],
                false,
                &mut output,
            )
            .expect("install-pack should verify and copy");
            assert!(home.join("runners").join("example.gtpack").exists());

            restore_path(old_path);
            std::env::remove_var("GREENTIC_FAKE_PACK_ARGS");
            std::env::remove_var("GREENTIC_FAKE_PACK_ANSWERS");
        });
    }

    #[test]
    fn gtc_runner_plan_dry_run_does_not_require_out() {
        with_temp_home(|_| {
            let mut output = Vec::new();
            run_with_writer(["desktop".to_owned(), "init".to_owned()], true, &mut output)
                .expect("init");
            run_with_writer(
                [
                    "desktop".to_owned(),
                    "extension".to_owned(),
                    "install".to_owned(),
                    "greentic.desktop.playwright".to_owned(),
                ],
                true,
                &mut output,
            )
            .expect("install extension");

            output.clear();
            run_with_writer(
                [
                    "desktop".to_owned(),
                    "runner".to_owned(),
                    "plan".to_owned(),
                    "--prompt".to_owned(),
                    "Create CRM customer with company name and email".to_owned(),
                    "--dry-run".to_owned(),
                ],
                true,
                &mut output,
            )
            .expect("dry run");

            let text = String::from_utf8(output).expect("output utf8");
            assert!(text.contains("planned: crm.create_customer"));
            assert!(text.contains("steps:"));
        });
    }

    #[test]
    fn record_command_manages_session_lifecycle() {
        with_temp_home(|home| {
            let mut output = Vec::new();
            let out = home.join("recordings").join("crm.create_customer");
            run_with_writer(
                [
                    "desktop".to_owned(),
                    "record".to_owned(),
                    "start".to_owned(),
                    "--name".to_owned(),
                    "crm.create_customer".to_owned(),
                    "--profile".to_owned(),
                    "local-crm".to_owned(),
                    "--adapter".to_owned(),
                    "greentic.desktop.playwright".to_owned(),
                    "--out".to_owned(),
                    out.display().to_string(),
                    "--redact".to_owned(),
                    "text,password,email,token".to_owned(),
                    "--secret-fields".to_owned(),
                    "password,api_key".to_owned(),
                ],
                true,
                &mut output,
            )
            .expect("record start");
            let text = String::from_utf8(output.clone()).expect("utf8");
            let session = text
                .lines()
                .find_map(|line| line.strip_prefix("session: "))
                .expect("session id")
                .to_owned();

            for command in ["pause", "resume", "status", "stop", "list"] {
                output.clear();
                let mut args = vec![
                    "desktop".to_owned(),
                    "record".to_owned(),
                    command.to_owned(),
                ];
                if command != "list" {
                    args.extend(["--session".to_owned(), session.clone()]);
                }
                run_with_writer(args, true, &mut output).expect(command);
            }
            assert!(out.join("manifest.yaml").exists());
            assert!(out.join("raw/events.jsonl").exists());
        });
    }

    #[test]
    fn record_normalise_and_finalise_write_runner_files() {
        with_temp_home(|home| {
            let recording = home.join("recording");
            let raw = recording.join("raw");
            fs::create_dir_all(&raw).expect("raw dir");
            fs::write(
                raw.join("events.jsonl"),
                "{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"web\",\"event\":{\"kind\":\"type_text\",\"target\":{\"label\":\"api token\"},\"value\":\"token=abc\",\"redaction\":\"secret\"},\"evidence\":{}}\n",
            )
            .expect("raw event");
            let out = home.join("runner.yaml");
            let mut output = Vec::new();
            run_with_writer(
                [
                    "record".to_owned(),
                    "normalise".to_owned(),
                    "--recording".to_owned(),
                    raw.display().to_string(),
                    "--out".to_owned(),
                    out.display().to_string(),
                ],
                false,
                &mut output,
            )
            .expect("normalise");
            let runner_yaml = fs::read_to_string(&out).expect("runner");
            assert!(runner_yaml.contains("{{secret}}"));
            assert!(runner_yaml.contains("secrets.recorded_secret"));
            assert!(!runner_yaml.contains("token=abc"));

            output.clear();
            run_with_writer(
                [
                    "record".to_owned(),
                    "finalise".to_owned(),
                    "--recording".to_owned(),
                    recording.display().to_string(),
                    "--runner".to_owned(),
                    out.display().to_string(),
                ],
                false,
                &mut output,
            )
            .expect("finalise");
            assert!(recording.join("runner.draft.yaml").exists());
        });
    }
}
