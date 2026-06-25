use greentic_desktop_config::RuntimeConfig;
use greentic_desktop_extension::built_in_extension;
use greentic_desktop_runtime::{discover_runners, DesktopRuntime};
use std::fmt;
use std::io::{self, Write};

#[derive(Debug)]
pub enum CliError {
    Io(std::io::Error),
    Runtime(greentic_desktop_runtime::RuntimeError),
    Usage(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
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
        "usage: {prefix} <info|init|config show|extension install ID|extension list|extension update|extension verify [ID]|extension sidecar ID|runner list|mcp serve [--bind ADDR]>"
    )
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
}
