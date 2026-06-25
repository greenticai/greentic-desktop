use greentic_desktop_config::RuntimeConfig;
use greentic_desktop_extension::built_in_extension;
use greentic_desktop_planner::{
    plan_prompt_with_default_llm, save_draft_runner, PlannerDiagnostic, PlannerOptions,
    PlanningContext,
};
use greentic_desktop_recorder::{
    append_recording_note, cancel_recording_session, finalise_recording, list_recording_sessions,
    load_recording_session, normalise_recording, pause_recording_session, resume_recording_session,
    start_recording_session, stop_recording_session, RecordingLifecycleError,
    RecordingStartRequest,
};
use greentic_desktop_runtime::{discover_runners, DesktopRuntime};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Debug)]
pub enum CliError {
    Io(std::io::Error),
    Planner(PlannerDiagnostic),
    Recording(RecordingLifecycleError),
    Runtime(greentic_desktop_runtime::RuntimeError),
    Usage(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Planner(err) => write!(f, "{err}"),
            Self::Recording(err) => write!(f, "{err}"),
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

pub fn run_desktop_cli(args: impl IntoIterator<Item = String>) -> Result<(), CliError> {
    run(args, false, &mut io::stdout())
}

pub fn run_gtc_cli(args: impl IntoIterator<Item = String>) -> Result<(), CliError> {
    run(args, true, &mut io::stdout())
}

pub fn run_with_writer(
    args: impl IntoIterator<Item = String>,
    require_desktop_prefix: bool,
    writer: &mut dyn Write,
) -> Result<(), CliError> {
    run(args, require_desktop_prefix, writer)
}

fn run(
    args: impl IntoIterator<Item = String>,
    require_desktop_prefix: bool,
    writer: &mut dyn Write,
) -> Result<(), CliError> {
    let mut args: Vec<String> = args.into_iter().collect();
    if args.is_empty() {
        return Err(CliError::Usage(usage(require_desktop_prefix)));
    }

    if require_desktop_prefix {
        if args.first().map(String::as_str) != Some("desktop") {
            return Err(CliError::Usage(usage(true)));
        }
        args.remove(0);
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
        [command, subcommand, extension_id]
            if command == "extension" && subcommand == "install" =>
        {
            let manifest = runtime.install_extension(extension_id)?;
            writeln!(writer, "installed: {}", manifest.id)?;
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
        [command, subcommand, rest @ ..] if command == "runner" && subcommand == "plan" => {
            handle_runner_plan(rest, &runtime, &config, writer)?;
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

    format!(
        "usage: {prefix} <info|init|config show|extension install ID|extension list|extension update|extension verify [ID]|extension sidecar ID|runner list|runner plan (--prompt TEXT|--prompt-file PATH) [--profile ID] [--context PATH] [--dry-run] [--out PATH]|record <start|pause|resume|stop|cancel|status|list|normalise|finalise|mark-input|mark-secret|mark-output|add-assertion|note>|mcp serve [--bind ADDR]>"
    )
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
    fn empty_args_print_usage() {
        with_temp_home(|_| {
            let mut output = Vec::new();
            let err =
                run_with_writer(Vec::<String>::new(), false, &mut output).expect_err("usage error");

            assert!(err.to_string().contains("greentic-desktop"));
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
                    "install".to_owned(),
                    "greentic.desktop.playwright".to_owned(),
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
    fn unknown_extension_and_invalid_mcp_bind_return_errors() {
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
            .expect_err("invalid bind should fail");
            assert!(!err.to_string().is_empty());
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
                "{\"type\":\"type_text\",\"value\":\"token=abc\"}\n",
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
            assert!(fs::read_to_string(&out)
                .expect("runner")
                .contains("{{secret}}"));

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
