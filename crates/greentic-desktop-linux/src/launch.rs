//! Resolving and spawning the program behind a `linux.open_app` step.
//!
//! The step value is either an executable path/name or a freedesktop
//! `.desktop` entry (an absolute path, or an id such as `org.gnome.Calculator`
//! looked up under `$XDG_DATA_HOME/applications` and every
//! `$XDG_DATA_DIRS/*/applications`).

use greentic_desktop_adapter::{AdapterError, AdapterResult};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCommand {
    pub program: String,
    pub args: Vec<String>,
}

/// Split a desktop entry `Exec=` value into argv, honouring double quotes and
/// dropping field codes (`%f`, `%U`, …) that have no meaning without files.
pub fn parse_desktop_exec(exec: &str) -> Option<LaunchCommand> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut has_token = false;
    let mut characters = exec.trim().chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            '\\' if in_quotes => {
                if let Some(escaped) = characters.next() {
                    current.push(escaped);
                }
            }
            character if character.is_whitespace() && !in_quotes => {
                if has_token {
                    args.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            character => {
                current.push(character);
                has_token = true;
            }
        }
    }
    if has_token {
        args.push(current);
    }
    let args = args
        .into_iter()
        .filter(|arg| !(arg.len() == 2 && arg.starts_with('%')))
        .map(|arg| arg.replace("%%", "%"))
        .collect::<Vec<_>>();
    let (program, rest) = args.split_first()?;
    Some(LaunchCommand {
        program: program.clone(),
        args: rest.to_vec(),
    })
}

/// The `Exec=` value of the `[Desktop Entry]` group.
pub fn desktop_entry_exec(contents: &str) -> Option<String> {
    let mut in_entry = false;
    for line in contents.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if in_entry {
            if let Some(value) = line.strip_prefix("Exec=") {
                return Some(value.trim().to_owned());
            }
        }
    }
    None
}

fn desktop_entry_path(value: &str) -> Option<PathBuf> {
    let direct = Path::new(value);
    if direct.is_absolute() {
        return direct.is_file().then(|| direct.to_path_buf());
    }
    let file_name = if value.ends_with(".desktop") {
        value.to_owned()
    } else {
        format!("{value}.desktop")
    };
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|dirs| !dirs.trim().is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    data_home
        .into_iter()
        .chain(data_dirs.split(':').map(PathBuf::from))
        .map(|dir| dir.join("applications").join(&file_name))
        .find(|candidate| candidate.is_file())
}

/// Resolve a step value into the command to spawn.
pub fn resolve_launch_command(value: &str) -> AdapterResult<LaunchCommand> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AdapterError::ExecutionFailed(
            "linux.open_app requires an executable path or .desktop entry in step.value."
                .to_owned(),
        ));
    }
    if value.ends_with(".desktop") || (!value.contains('/') && desktop_entry_path(value).is_some())
    {
        let path = desktop_entry_path(value).ok_or_else(|| {
            AdapterError::ExecutionFailed(format!("desktop entry {value} was not found"))
        })?;
        let contents = std::fs::read_to_string(&path).map_err(|error| {
            AdapterError::ExecutionFailed(format!("failed to read {}: {error}", path.display()))
        })?;
        return desktop_entry_exec(&contents)
            .and_then(|exec| parse_desktop_exec(&exec))
            .ok_or_else(|| {
                AdapterError::ExecutionFailed(format!(
                    "desktop entry {} has no usable Exec= line",
                    path.display()
                ))
            });
    }
    Ok(LaunchCommand {
        program: value.to_owned(),
        args: Vec::new(),
    })
}

/// Spawn the command detached from the adapter's stdio, with accessibility
/// explicitly enabled for GTK: `NO_AT_BRIDGE` and `GTK_A11Y=none` would stop
/// the application from registering on the accessibility bus.
pub fn spawn_detached(command: &LaunchCommand) -> AdapterResult<u32> {
    // The program comes from the runner step or a desktop entry and is
    // executed directly with an argv array, never through a shell.
    // foxguard: ignore[rs/no-command-injection]
    let child = Command::new(&command.program)
        .args(&command.args)
        .env_remove("NO_AT_BRIDGE")
        .env_remove("GTK_A11Y")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            AdapterError::ExecutionFailed(format!("failed to launch {}: {error}", command.program))
        })?;
    Ok(child.id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_exec_drops_field_codes_and_honours_quotes() {
        let command = parse_desktop_exec(r#""/opt/My App/bin/app" --flag %U"#).expect("exec");
        assert_eq!(command.program, "/opt/My App/bin/app");
        assert_eq!(command.args, vec!["--flag".to_owned()]);
        assert!(parse_desktop_exec("  %f ").is_none());
    }

    #[test]
    fn desktop_entry_exec_reads_only_the_main_group() {
        let contents =
            "[Desktop Entry]\nName=Meridian\nExec=meridian --x\n[Desktop Action New]\nExec=other\n";
        assert_eq!(
            desktop_entry_exec(contents).as_deref(),
            Some("meridian --x")
        );
        assert!(desktop_entry_exec("[Desktop Action X]\nExec=a\n").is_none());
    }

    #[test]
    fn resolves_desktop_entries_from_xdg_data_dirs_and_plain_programs() {
        let root = std::env::temp_dir().join(format!("greentic-launch-{}", std::process::id()));
        let applications = root.join("applications");
        std::fs::create_dir_all(&applications).expect("dirs");
        std::fs::write(
            applications.join("ai.greentic.meridian.desktop"),
            "[Desktop Entry]\nExec=/usr/bin/meridian %F\n",
        )
        .expect("entry");
        let entry = applications.join("ai.greentic.meridian.desktop");
        let command = resolve_launch_command(&entry.display().to_string()).expect("absolute entry");
        assert_eq!(command.program, "/usr/bin/meridian");
        assert!(command.args.is_empty());

        let plain = resolve_launch_command("/usr/bin/true").expect("program");
        assert_eq!(plain.program, "/usr/bin/true");
        assert!(resolve_launch_command("  ").is_err());
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
