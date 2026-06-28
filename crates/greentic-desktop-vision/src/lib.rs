use greentic_desktop_adapter::{
    AdapterCapabilities, AdapterError, AdapterResult, Assertion, AssertionResult, DesktopAdapter,
    Observation, ObserveContext, RecordedEvent, RunnerStep, StepResult,
};
use greentic_desktop_recorder::{
    RecordingBackend, RecordingCaptureState, RecordingEventEnvelope, RecordingEventSink,
    RecordingHandle, RecordingPreflight, RecordingStartRequest, RecordingTargetKind,
};
use std::sync::{Arc, Mutex};

pub const VISION_ADAPTER_ID: &str = "greentic.desktop.vision";
pub const REMOTE_RECORDER_BACKEND_ID: &str = "greentic.recording.remote.vision";

pub fn vision_capabilities() -> AdapterCapabilities {
    AdapterCapabilities::new(
        VISION_ADAPTER_ID,
        env!("CARGO_PKG_VERSION"),
        [
            "vision.screenshot",
            "vision.find_text",
            "vision.find_button",
            "vision.click_region",
            "vision.compare_baseline",
            "vision.assert_visual",
            "vision.extract_text",
        ],
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisionMatch {
    pub label: String,
    pub region: Region,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualEvidence {
    pub before_screenshot: String,
    pub annotated_region: Region,
    pub confidence: f32,
    pub after_screenshot: String,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteViewportCalibration {
    pub origin_x: u32,
    pub origin_y: u32,
    pub width: u32,
    pub height: u32,
    pub scale_percent: u32,
}

#[derive(Debug, Clone)]
pub struct RemoteVisionRecordingBackend {
    greentic_owned_session: bool,
    screen_capture_available: bool,
    input_control_available: bool,
    calibration: Option<RemoteViewportCalibration>,
}

impl RemoteVisionRecordingBackend {
    pub fn new(
        greentic_owned_session: bool,
        screen_capture_available: bool,
        input_control_available: bool,
        calibration: Option<RemoteViewportCalibration>,
    ) -> Self {
        Self {
            greentic_owned_session,
            screen_capture_available,
            input_control_available,
            calibration,
        }
    }
}

impl RecordingBackend for RemoteVisionRecordingBackend {
    fn id(&self) -> &'static str {
        REMOTE_RECORDER_BACKEND_ID
    }

    fn target_kind(&self) -> RecordingTargetKind {
        RecordingTargetKind::Remote
    }

    fn preflight(&self, _request: &RecordingStartRequest) -> RecordingPreflight {
        let mut reasons = Vec::new();
        if !self.greentic_owned_session {
            reasons.push(
                "Remote recording requires a Greentic-owned RDP/VNC/WorkSpaces/browser canvas session."
                    .to_owned(),
            );
        }
        if !self.screen_capture_available {
            reasons.push("Screen capture permission is required for remote recording.".to_owned());
        }
        if !self.input_control_available {
            reasons.push(
                "Keyboard and mouse control permission is required for replayable remote recording."
                    .to_owned(),
            );
        }
        if self.calibration.is_none() {
            reasons.push(
                "Remote viewport calibration is required before recording input coordinates."
                    .to_owned(),
            );
        }

        if reasons.is_empty() {
            RecordingPreflight::ready()
        } else {
            RecordingPreflight {
                available: false,
                blocked_reasons: reasons,
            }
        }
    }

    fn start(&self, _request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle {
        let calibration = self.calibration.unwrap_or(RemoteViewportCalibration {
            origin_x: 0,
            origin_y: 0,
            width: 0,
            height: 0,
            scale_percent: 100,
        });
        let mut event = RecordingEventEnvelope::new(
            sink.session_id(),
            REMOTE_RECORDER_BACKEND_ID,
            RecordingTargetKind::Remote,
            1,
            "focus_session",
        );
        event.target_json = format!(
            r#"{{"viewport":{{"x":{},"y":{},"width":{},"height":{},"scale_percent":{}}},"ownership":"greentic-owned"}}"#,
            calibration.origin_x,
            calibration.origin_y,
            calibration.width,
            calibration.height,
            calibration.scale_percent
        );
        event.value = Some("remote viewport focused".to_owned());
        event.screenshot_ref = Some("evidence://remote/initial.png".to_owned());
        let _ = sink.append_event(event);
        let _ = sink.update_heartbeat();

        RecordingHandle {
            backend_id: REMOTE_RECORDER_BACKEND_ID.to_owned(),
            capture_state: RecordingCaptureState::Recording,
        }
    }
}

pub fn remote_recording_event(
    session_id: &str,
    sequence: u64,
    kind: &str,
    region: Region,
    value: Option<String>,
    screenshot_ref: Option<String>,
) -> RecordingEventEnvelope {
    let mut event = RecordingEventEnvelope::new(
        session_id,
        REMOTE_RECORDER_BACKEND_ID,
        RecordingTargetKind::Remote,
        sequence,
        kind,
    );
    event.target_json = format!(
        r#"{{"region":{{"x":{},"y":{},"width":{},"height":{}}}}}"#,
        region.x, region.y, region.width, region.height
    );
    event.value = value;
    event.redaction = if event.value.is_some() {
        "input_candidate".to_owned()
    } else {
        "none".to_owned()
    };
    event.screenshot_ref = screenshot_ref;
    event
}

#[derive(Debug, Clone, Default)]
pub struct VisionAdapter {
    state: Arc<Mutex<VisionState>>,
}

#[derive(Debug, Clone, Default)]
struct VisionState {
    screenshot: String,
    visible_text: Vec<VisionMatch>,
    evidence: Vec<VisualEvidence>,
    recorded: Vec<RecordedEvent>,
}

impl VisionAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_screenshot(&self, screenshot: impl Into<String>) {
        self.state
            .lock()
            .expect("vision adapter mutex poisoned")
            .screenshot = screenshot.into();
    }

    pub fn add_text_match(&self, label: impl Into<String>, region: Region, confidence: f32) {
        self.state
            .lock()
            .expect("vision adapter mutex poisoned")
            .visible_text
            .push(VisionMatch {
                label: label.into(),
                region,
                confidence,
            });
    }

    pub fn find_text(&self, text: &str, min_confidence: f32) -> Option<VisionMatch> {
        self.state
            .lock()
            .expect("vision adapter mutex poisoned")
            .visible_text
            .iter()
            .find(|item| item.label.contains(text) && item.confidence >= min_confidence)
            .cloned()
    }

    pub fn compare_baseline(&self, baseline: &str) -> VisualEvidence {
        let mut state = self.state.lock().expect("vision adapter mutex poisoned");
        let passed = state.screenshot == baseline;
        let evidence = VisualEvidence {
            before_screenshot: baseline.to_owned(),
            annotated_region: Region {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
            confidence: if passed { 1.0 } else { 0.0 },
            after_screenshot: state.screenshot.clone(),
            explanation: if passed {
                "current screen matches baseline".to_owned()
            } else {
                "current screen differs from baseline".to_owned()
            },
        };
        state.evidence.push(evidence.clone());
        evidence
    }

    pub fn latest_evidence(&self) -> Option<VisualEvidence> {
        self.state
            .lock()
            .expect("vision adapter mutex poisoned")
            .evidence
            .last()
            .cloned()
    }
}

impl DesktopAdapter for VisionAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        vision_capabilities()
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        let state = self.state.lock().expect("vision adapter mutex poisoned");
        Ok(Observation {
            adapter_id: VISION_ADAPTER_ID.to_owned(),
            summary: format!(
                "vision session {} screenshot={}",
                ctx.session_id, state.screenshot
            ),
            visible_text: state
                .visible_text
                .iter()
                .map(|item| item.label.clone())
                .collect(),
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }

        let mut state = self.state.lock().expect("vision adapter mutex poisoned");
        match step.required_capability.as_str() {
            "vision.screenshot" => {
                state.screenshot = step.value.clone().unwrap_or_else(|| "screen".to_owned());
            }
            "vision.click_region" => {
                let region = Region {
                    x: 10,
                    y: 10,
                    width: 40,
                    height: 20,
                };
                let evidence = VisualEvidence {
                    before_screenshot: state.screenshot.clone(),
                    annotated_region: region,
                    confidence: 0.95,
                    after_screenshot: state.screenshot.clone(),
                    explanation: "clicked visually identified region".to_owned(),
                };
                state.evidence.push(evidence);
            }
            "vision.find_text"
            | "vision.find_button"
            | "vision.compare_baseline"
            | "vision.assert_visual"
            | "vision.extract_text" => {}
            _ => {}
        }

        state.recorded.push(RecordedEvent {
            action: step.action.clone(),
            target: step.target,
            value: step.value,
        });

        Ok(StepResult {
            step_id: step.id,
            success: true,
            message: "vision step accepted".to_owned(),
        })
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        if !self.capabilities().supports(&assertion.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                assertion.required_capability,
            ));
        }

        let passed = match assertion.required_capability.as_str() {
            "vision.find_text" | "vision.find_button" | "vision.extract_text" => {
                self.find_text(&assertion.expected, 0.70).is_some()
            }
            "vision.compare_baseline" | "vision.assert_visual" => {
                self.compare_baseline(&assertion.expected).confidence >= 0.99
            }
            _ => true,
        };

        Ok(AssertionResult {
            assertion_id: assertion.id,
            passed,
            message: self
                .latest_evidence()
                .map(|evidence| evidence.explanation)
                .unwrap_or_else(|| {
                    if passed {
                        "vision assertion passed".to_owned()
                    } else {
                        "vision assertion failed".to_owned()
                    }
                }),
        })
    }

    fn record_event(&self) -> AdapterResult<Option<RecordedEvent>> {
        Ok(self
            .state
            .lock()
            .expect("vision adapter mutex poisoned")
            .recorded
            .last()
            .cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_adapter::LocatorTarget;

    #[test]
    fn exposes_vision_capabilities() {
        let capabilities = vision_capabilities();

        assert!(capabilities.supports("vision.screenshot"));
        assert!(capabilities.supports("vision.assert_visual"));
        assert_eq!(capabilities.adapter_id, VISION_ADAPTER_ID);
    }

    #[test]
    fn locates_visible_text_on_screen() {
        let adapter = VisionAdapter::new();
        adapter.add_text_match(
            "Submit",
            Region {
                x: 20,
                y: 30,
                width: 80,
                height: 24,
            },
            0.91,
        );

        let found = adapter
            .find_text("Submit", 0.80)
            .expect("text should match");
        assert_eq!(found.region.x, 20);
    }

    #[test]
    fn clicks_button_and_records_visual_evidence() {
        let adapter = VisionAdapter::new();
        adapter.load_screenshot("before");
        adapter
            .execute(RunnerStep {
                id: "click".to_owned(),
                action: "click_region".to_owned(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: "vision.click_region".to_owned(),
            })
            .expect("click should be accepted");

        let evidence = adapter.latest_evidence().expect("evidence should exist");
        assert_eq!(evidence.before_screenshot, "before");
        assert!(evidence.confidence >= 0.9);
    }

    #[test]
    fn explains_visual_assertion_result() {
        let adapter = VisionAdapter::new();
        adapter.load_screenshot("current");

        let result = adapter
            .validate(Assertion {
                id: "baseline".to_owned(),
                required_capability: "vision.assert_visual".to_owned(),
                target: LocatorTarget::default(),
                expected: "baseline".to_owned(),
            })
            .expect("visual assertion should run");

        assert!(!result.passed);
        assert!(result.message.contains("differs"));
    }

    #[test]
    fn remote_recording_backend_blocks_missing_screen_capture_and_calibration() {
        let backend = RemoteVisionRecordingBackend::new(true, false, true, None);
        let preflight = backend.preflight(&RecordingStartRequest {
            name: "remote.record".to_owned(),
            profile: "remote".to_owned(),
            adapter: VISION_ADAPTER_ID.to_owned(),
            target_kind: RecordingTargetKind::Remote,
            out: std::env::temp_dir().join("remote-record"),
            runtime_home: std::env::temp_dir().join("remote-record-home"),
            redact: Vec::new(),
            secret_fields: Vec::new(),
        });

        assert!(!preflight.available);
        assert!(preflight
            .blocked_reasons
            .iter()
            .any(|reason| reason.contains("Screen capture permission")));
        assert!(preflight
            .blocked_reasons
            .iter()
            .any(|reason| reason.contains("viewport calibration")));
    }

    #[test]
    fn remote_recording_event_includes_region_and_screenshot_evidence() {
        let event = remote_recording_event(
            "rec_remote",
            4,
            "click_region",
            Region {
                x: 12,
                y: 20,
                width: 80,
                height: 24,
            },
            None,
            Some("evidence://remote/after-click.png".to_owned()),
        );

        let json = event.render_json();
        assert!(json.contains("\"target_kind\":\"remote\""));
        assert!(json.contains("\"x\":12"));
        assert!(json.contains("after-click.png"));
    }
}
