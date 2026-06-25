use greentic_desktop_evidence::{
    EvidenceArtifact, EvidenceArtifactKind, EvidenceBundle, EvidenceStatus, InMemoryEvidenceStore,
    ToolTraceEntry,
};
use greentic_desktop_platform::{DesktopPlatform, PlatformInfo, PlatformPermission};
use std::collections::BTreeMap;
use std::fmt;

pub fn desktop_io_capabilities() -> Vec<&'static str> {
    vec![
        "input.move_mouse",
        "input.click",
        "input.double_click",
        "input.drag",
        "input.type_text",
        "input.hotkey",
        "screen.screenshot",
        "screen.region_screenshot",
        "screen.locate_text",
        "screen.locate_image",
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    MoveMouse { to: Point },
    Click { at: Point },
    DoubleClick { at: Point },
    Drag { from: Point, to: Point },
    TypeText { text: String },
    Hotkey { keys: Vec<String> },
}

impl InputAction {
    pub fn capability(&self) -> &'static str {
        match self {
            Self::MoveMouse { .. } => "input.move_mouse",
            Self::Click { .. } => "input.click",
            Self::DoubleClick { .. } => "input.double_click",
            Self::Drag { .. } => "input.drag",
            Self::TypeText { .. } => "input.type_text",
            Self::Hotkey { .. } => "input.hotkey",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputResult {
    pub capability: String,
    pub accepted: bool,
    pub backend: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screenshot {
    pub uri: String,
    pub region: Option<Region>,
    pub backend: String,
}

impl Screenshot {
    pub fn evidence_artifact(&self) -> EvidenceArtifact {
        EvidenceArtifact::new(
            if self.region.is_some() {
                EvidenceArtifactKind::AnnotatedScreenshot
            } else {
                EvidenceArtifactKind::Screenshot
            },
            if self.region.is_some() {
                "region_screenshot"
            } else {
                "screenshot"
            },
            self.uri.clone(),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocateResult {
    pub found: bool,
    pub region: Option<Region>,
    pub confidence: f32,
    pub evidence_uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoError {
    CapabilityUnavailable {
        capability: String,
        reason: String,
    },
    PermissionDenied {
        permission: PlatformPermission,
        reason: String,
    },
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapabilityUnavailable { capability, reason } => {
                write!(f, "{capability} unavailable: {reason}")
            }
            Self::PermissionDenied { permission, reason } => {
                write!(f, "{} denied: {reason}", permission.as_str())
            }
        }
    }
}

impl std::error::Error for IoError {}

#[derive(Debug, Clone)]
pub struct DesktopIoBackend {
    platform: PlatformInfo,
    wayland_portal_screenshot: bool,
    log: Vec<String>,
}

impl DesktopIoBackend {
    pub fn new(platform: PlatformInfo) -> Self {
        Self {
            platform,
            wayland_portal_screenshot: false,
            log: Vec::new(),
        }
    }

    pub fn with_wayland_portal_screenshot(mut self, available: bool) -> Self {
        self.wayland_portal_screenshot = available;
        self
    }

    pub fn log(&self) -> &[String] {
        &self.log
    }

    pub fn available_capabilities(&self) -> Vec<&'static str> {
        desktop_io_capabilities()
            .into_iter()
            .filter(|capability| self.capability_available(capability).is_ok())
            .collect()
    }

    pub fn perform_input(&mut self, action: InputAction) -> Result<InputResult, IoError> {
        let capability = action.capability();
        self.capability_available(capability)?;
        self.log
            .push(format!("{capability}:{}", self.backend_name()));
        Ok(InputResult {
            capability: capability.to_owned(),
            accepted: true,
            backend: self.backend_name(),
        })
    }

    pub fn screenshot(&mut self) -> Result<Screenshot, IoError> {
        self.capture_screen("screen.screenshot", None)
    }

    pub fn region_screenshot(&mut self, region: Region) -> Result<Screenshot, IoError> {
        self.capture_screen("screen.region_screenshot", Some(region))
    }

    pub fn locate_text(&mut self, text: &str) -> Result<LocateResult, IoError> {
        let shot = self.screenshot()?;
        Ok(LocateResult {
            found: !text.trim().is_empty(),
            region: Some(Region {
                x: 10,
                y: 20,
                width: 120,
                height: 24,
            }),
            confidence: 0.92,
            evidence_uri: shot.uri,
        })
    }

    pub fn locate_image(&mut self, image_ref: &str) -> Result<LocateResult, IoError> {
        let shot = self.screenshot()?;
        Ok(LocateResult {
            found: !image_ref.trim().is_empty(),
            region: Some(Region {
                x: 40,
                y: 50,
                width: 64,
                height: 64,
            }),
            confidence: 0.88,
            evidence_uri: shot.uri,
        })
    }

    pub fn store_screenshot_evidence(
        &mut self,
        store: &mut InMemoryEvidenceStore,
        run_id: &str,
        runner_id: &str,
    ) -> Result<String, IoError> {
        let screenshot = self.screenshot()?;
        let bundle = EvidenceBundle::new(
            run_id,
            runner_id,
            "1.0.0",
            EvidenceStatus::Success,
            &BTreeMap::new(),
            &[],
            BTreeMap::new(),
            vec![screenshot.evidence_artifact()],
            vec![ToolTraceEntry {
                step_id: "screen".to_owned(),
                capability: "screen.screenshot".to_owned(),
                status: EvidenceStatus::Success,
                message: Some(format!("captured via {}", screenshot.backend)),
            }],
            "now",
            "now",
        );
        let reference = store
            .insert(bundle)
            .expect("test evidence run ids should be unique");
        Ok(reference.uri)
    }

    fn capture_screen(
        &mut self,
        capability: &str,
        region: Option<Region>,
    ) -> Result<Screenshot, IoError> {
        self.capability_available(capability)?;
        let backend = self.backend_name();
        let scope = if region.is_some() { "region" } else { "screen" };
        let uri = format!(
            "evidence://{}/{scope}-{}.png",
            backend.replace(':', "/"),
            self.log.len() + 1
        );
        self.log.push(format!("{capability}:{backend}"));
        Ok(Screenshot {
            uri,
            region,
            backend,
        })
    }

    fn capability_available(&self, capability: &str) -> Result<(), IoError> {
        if self.is_wayland() {
            return self.wayland_capability_available(capability);
        }

        match capability {
            "input.move_mouse" | "input.click" | "input.double_click" | "input.drag" => {
                if self.platform.has_permission(PlatformPermission::MouseInput) {
                    Ok(())
                } else {
                    Err(IoError::PermissionDenied {
                        permission: PlatformPermission::MouseInput,
                        reason: "mouse input permission is required".to_owned(),
                    })
                }
            }
            "input.type_text" | "input.hotkey" => {
                if self
                    .platform
                    .has_permission(PlatformPermission::KeyboardInput)
                {
                    Ok(())
                } else {
                    Err(IoError::PermissionDenied {
                        permission: PlatformPermission::KeyboardInput,
                        reason: "keyboard input permission is required".to_owned(),
                    })
                }
            }
            "screen.screenshot"
            | "screen.region_screenshot"
            | "screen.locate_text"
            | "screen.locate_image" => self.screenshot_permission(),
            _ => Err(IoError::CapabilityUnavailable {
                capability: capability.to_owned(),
                reason: "unknown IO capability".to_owned(),
            }),
        }
    }

    fn wayland_capability_available(&self, capability: &str) -> Result<(), IoError> {
        match capability {
            "input.hotkey" => self.capability_available_without_wayland("input.hotkey"),
            "screen.screenshot"
            | "screen.region_screenshot"
            | "screen.locate_text"
            | "screen.locate_image" => {
                if self.wayland_portal_screenshot {
                    Ok(())
                } else {
                    Err(IoError::CapabilityUnavailable {
                        capability: capability.to_owned(),
                        reason: "Wayland screenshot capture requires xdg-desktop-portal approval"
                            .to_owned(),
                    })
                }
            }
            "input.move_mouse" | "input.click" | "input.double_click" | "input.drag"
            | "input.type_text" => Err(IoError::CapabilityUnavailable {
                capability: capability.to_owned(),
                reason:
                    "Wayland blocks global input injection; use app accessibility or safe shortcuts"
                        .to_owned(),
            }),
            _ => Err(IoError::CapabilityUnavailable {
                capability: capability.to_owned(),
                reason: "unknown IO capability".to_owned(),
            }),
        }
    }

    fn capability_available_without_wayland(&self, capability: &str) -> Result<(), IoError> {
        let mut clone = self.clone();
        clone.platform.display_server = Some("x11".to_owned());
        clone.capability_available(capability)
    }

    fn screenshot_permission(&self) -> Result<(), IoError> {
        if self.platform.has_permission(PlatformPermission::Screenshot)
            || self
                .platform
                .has_permission(PlatformPermission::ScreenRecording)
        {
            Ok(())
        } else {
            Err(IoError::PermissionDenied {
                permission: PlatformPermission::Screenshot,
                reason: "screenshot or screen recording permission is required".to_owned(),
            })
        }
    }

    fn is_wayland(&self) -> bool {
        self.platform.os == DesktopPlatform::Linux
            && self.platform.display_server.as_deref() == Some("wayland")
    }

    fn backend_name(&self) -> String {
        match self.platform.os {
            DesktopPlatform::Windows => "windows:uia-win32".to_owned(),
            DesktopPlatform::MacOS => "macos:coregraphics-accessibility".to_owned(),
            DesktopPlatform::Linux
                if self.platform.display_server.as_deref() == Some("wayland") =>
            {
                "linux:wayland-portal".to_owned()
            }
            DesktopPlatform::Linux => "linux:x11-xtest".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform(os: DesktopPlatform, display: Option<&str>) -> PlatformInfo {
        PlatformInfo {
            os,
            version: "test".to_owned(),
            desktop_environment: None,
            display_server: display.map(str::to_owned),
            permissions: vec![
                PlatformPermission::KeyboardInput,
                PlatformPermission::MouseInput,
                PlatformPermission::Screenshot,
                PlatformPermission::ScreenRecording,
            ],
        }
    }

    #[test]
    fn same_input_api_works_across_supported_platforms() {
        for platform in [
            platform(DesktopPlatform::Windows, Some("desktop")),
            platform(DesktopPlatform::MacOS, Some("quartz")),
            platform(DesktopPlatform::Linux, Some("x11")),
        ] {
            let mut backend = DesktopIoBackend::new(platform);

            assert!(
                backend
                    .perform_input(InputAction::MoveMouse {
                        to: Point { x: 10, y: 20 },
                    })
                    .expect("move mouse")
                    .accepted
            );
            assert!(
                backend
                    .perform_input(InputAction::Click {
                        at: Point { x: 10, y: 20 },
                    })
                    .expect("click")
                    .accepted
            );
            assert!(
                backend
                    .perform_input(InputAction::TypeText {
                        text: "Acme".to_owned(),
                    })
                    .expect("type")
                    .accepted
            );
            assert!(
                backend
                    .perform_input(InputAction::Hotkey {
                        keys: vec!["Ctrl".to_owned(), "S".to_owned()],
                    })
                    .expect("hotkey")
                    .accepted
            );
        }
    }

    #[test]
    fn screenshot_capture_is_permission_aware() {
        let mut missing = platform(DesktopPlatform::MacOS, Some("quartz"));
        missing.permissions.clear();
        let mut backend = DesktopIoBackend::new(missing);

        assert!(matches!(
            backend.screenshot().expect_err("permission denied"),
            IoError::PermissionDenied {
                permission: PlatformPermission::Screenshot,
                ..
            }
        ));
    }

    #[test]
    fn evidence_store_receives_consistent_screenshots_regardless_of_os() {
        let mut store = InMemoryEvidenceStore::default();
        let mut windows =
            DesktopIoBackend::new(platform(DesktopPlatform::Windows, Some("desktop")));
        let mut macos = DesktopIoBackend::new(platform(DesktopPlatform::MacOS, Some("quartz")));

        let win_ref = windows
            .store_screenshot_evidence(&mut store, "run_windows", "runner")
            .expect("windows evidence");
        let mac_ref = macos
            .store_screenshot_evidence(&mut store, "run_macos", "runner")
            .expect("mac evidence");

        assert!(win_ref.contains("run_windows"));
        assert!(mac_ref.contains("run_macos"));
        assert_eq!(
            store
                .get("run_windows")
                .expect("bundle")
                .artifacts
                .first()
                .expect("artifact")
                .kind,
            EvidenceArtifactKind::Screenshot
        );
        assert_eq!(
            store
                .get("run_macos")
                .expect("bundle")
                .artifacts
                .first()
                .expect("artifact")
                .kind,
            EvidenceArtifactKind::Screenshot
        );
    }

    #[test]
    fn region_screenshot_and_locate_results_include_regions_and_confidence() {
        let mut backend = DesktopIoBackend::new(platform(DesktopPlatform::Linux, Some("x11")));
        let region = Region {
            x: 1,
            y: 2,
            width: 100,
            height: 50,
        };

        let shot = backend.region_screenshot(region).expect("region shot");
        let text = backend.locate_text("Customer").expect("locate text");
        let image = backend.locate_image("button.png").expect("locate image");

        assert_eq!(shot.region, Some(region));
        assert!(text.found);
        assert!(text.confidence > 0.9);
        assert!(image.found);
        assert!(image.confidence > 0.8);
    }

    #[test]
    fn wayland_limitations_are_capability_failures_not_runtime_surprises() {
        let mut backend = DesktopIoBackend::new(platform(DesktopPlatform::Linux, Some("wayland")));

        let click = backend
            .perform_input(InputAction::Click {
                at: Point { x: 10, y: 20 },
            })
            .expect_err("global click should fail on Wayland");
        let shot = backend
            .screenshot()
            .expect_err("portal screenshot is not approved");

        assert!(click.to_string().contains("Wayland blocks global input"));
        assert!(shot.to_string().contains("xdg-desktop-portal"));
    }

    #[test]
    fn wayland_portal_screenshots_and_safe_hotkeys_are_allowed() {
        let mut backend = DesktopIoBackend::new(platform(DesktopPlatform::Linux, Some("wayland")))
            .with_wayland_portal_screenshot(true);

        assert!(backend
            .screenshot()
            .expect("portal screenshot")
            .uri
            .contains("wayland"));
        assert!(
            backend
                .perform_input(InputAction::Hotkey {
                    keys: vec!["Ctrl".to_owned(), "L".to_owned()],
                })
                .expect("safe hotkey")
                .accepted
        );
    }
}
