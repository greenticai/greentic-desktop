use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopSession {
    pub id: String,
    pub state: SessionState,
    pub created_at: SystemTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Created,
    Attached,
    Closed,
}

impl DesktopSession {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            state: SessionState::Created,
            created_at: SystemTime::now(),
        }
    }

    pub fn attach(&mut self) {
        self.state = SessionState::Attached;
    }

    pub fn close(&mut self) {
        self.state = SessionState::Closed;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionProfile {
    pub id: String,
    pub bootstrap: Vec<BootstrapAction>,
    pub teardown: Vec<TeardownAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapAction {
    StartProcess {
        command: String,
        args: Vec<String>,
        working_dir: Option<String>,
        output_ref: String,
    },
    WaitForHttp {
        url: String,
        timeout_seconds: u64,
    },
    OpenBrowser {
        browser: BrowserKind,
        url: String,
    },
    OpenApp {
        path: String,
    },
    WaitForWindow {
        title_contains: String,
    },
    TerminalConnect {
        protocol: String,
        host: String,
        port: u16,
    },
    AttachWorkspace {
        workspace_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeardownAction {
    StopProcess { reference: String },
    CloseApp { title_contains: String },
    TerminalDisconnect,
    DetachWorkspace { workspace_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserKind {
    Default,
    Chromium,
    Firefox,
    WebKit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapPlan {
    pub profile_id: String,
    pub started_process_refs: Vec<String>,
    pub opened_targets: Vec<String>,
}

impl SessionProfile {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("session profile id must not be empty".to_owned());
        }

        for action in &self.bootstrap {
            if let BootstrapAction::StartProcess {
                command,
                output_ref,
                ..
            } = action
            {
                if command.trim().is_empty() || output_ref.trim().is_empty() {
                    return Err("start_process requires command and output_ref".to_owned());
                }
            }
        }

        Ok(())
    }
}

pub fn plan_bootstrap(profile: &SessionProfile) -> Result<BootstrapPlan, String> {
    profile.validate()?;
    let mut started_process_refs = Vec::new();
    let mut opened_targets = Vec::new();

    for action in &profile.bootstrap {
        match action {
            BootstrapAction::StartProcess { output_ref, .. } => {
                started_process_refs.push(output_ref.clone());
            }
            BootstrapAction::OpenBrowser { url, .. } => opened_targets.push(url.clone()),
            BootstrapAction::OpenApp { path } => opened_targets.push(path.clone()),
            BootstrapAction::TerminalConnect { host, port, .. } => {
                opened_targets.push(format!("{host}:{port}"));
            }
            BootstrapAction::AttachWorkspace { workspace_id } => {
                opened_targets.push(workspace_id.clone());
            }
            BootstrapAction::WaitForHttp { .. } | BootstrapAction::WaitForWindow { .. } => {}
        }
    }

    Ok(BootstrapPlan {
        profile_id: profile.id.clone(),
        started_process_refs,
        opened_targets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_tracks_lifecycle() {
        let mut session = DesktopSession::new("local");
        assert_eq!(session.state, SessionState::Created);
        session.attach();
        assert_eq!(session.state, SessionState::Attached);
        session.close();
        assert_eq!(session.state, SessionState::Closed);
    }

    #[test]
    fn plans_local_web_app_profile() {
        let profile = SessionProfile {
            id: "local_web_app_test".to_owned(),
            bootstrap: vec![
                BootstrapAction::StartProcess {
                    command: "npm".to_owned(),
                    args: vec!["run".to_owned(), "dev".to_owned()],
                    working_dir: Some("{{workspace_dir}}".to_owned()),
                    output_ref: "npm_dev_server".to_owned(),
                },
                BootstrapAction::WaitForHttp {
                    url: "http://localhost:5173".to_owned(),
                    timeout_seconds: 60,
                },
                BootstrapAction::OpenBrowser {
                    browser: BrowserKind::Default,
                    url: "http://localhost:5173".to_owned(),
                },
            ],
            teardown: vec![TeardownAction::StopProcess {
                reference: "npm_dev_server".to_owned(),
            }],
        };

        let plan = plan_bootstrap(&profile).expect("profile should plan");
        assert_eq!(plan.started_process_refs, vec!["npm_dev_server"]);
        assert_eq!(plan.opened_targets, vec!["http://localhost:5173"]);
    }

    #[test]
    fn plans_terminal_and_workspace_profiles() {
        let terminal = SessionProfile {
            id: "mainframe_customer_system".to_owned(),
            bootstrap: vec![BootstrapAction::TerminalConnect {
                protocol: "tn3270".to_owned(),
                host: "{{secrets.mainframe_host}}".to_owned(),
                port: 23,
            }],
            teardown: vec![TeardownAction::TerminalDisconnect],
        };
        let workspace = SessionProfile {
            id: "aws_workspace".to_owned(),
            bootstrap: vec![BootstrapAction::AttachWorkspace {
                workspace_id: "ws-123".to_owned(),
            }],
            teardown: vec![TeardownAction::DetachWorkspace {
                workspace_id: "ws-123".to_owned(),
            }],
        };

        assert_eq!(
            plan_bootstrap(&terminal)
                .expect("terminal profile")
                .opened_targets,
            vec!["{{secrets.mainframe_host}}:23"]
        );
        assert_eq!(
            plan_bootstrap(&workspace)
                .expect("workspace profile")
                .opened_targets,
            vec!["ws-123"]
        );
    }
}
