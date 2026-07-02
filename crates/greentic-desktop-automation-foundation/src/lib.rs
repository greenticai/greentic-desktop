use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use enigo::{Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use greentic_desktop_security::redact_sensitive_text_with_values;
use schemars::{schema_for, JsonSchema};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::{Command, Stdio};

pub type FoundationResult<T> = Result<T, FoundationError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundationError {
    pub code: String,
    pub message: String,
}

impl FoundationError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new("foundation.unavailable", message)
    }
}

impl fmt::Display for FoundationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for FoundationError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendAvailability {
    pub available: bool,
    pub reason: Option<String>,
}

impl BackendAvailability {
    pub fn available() -> Self {
        Self {
            available: true,
            reason: None,
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CapturedInputEvent {
    KeyPress { name: Option<String> },
    KeyRelease { name: Option<String> },
    ButtonPress,
    ButtonRelease,
    MouseMove { x: f64, y: f64 },
    Wheel { delta_x: i64, delta_y: i64 },
    Unknown,
}

#[cfg(feature = "rdev-capture")]
impl From<rdev::Event> for CapturedInputEvent {
    fn from(event: rdev::Event) -> Self {
        match event.event_type {
            rdev::EventType::KeyPress(_) => Self::KeyPress { name: event.name },
            rdev::EventType::KeyRelease(_) => Self::KeyRelease { name: event.name },
            rdev::EventType::ButtonPress(_) => Self::ButtonPress,
            rdev::EventType::ButtonRelease(_) => Self::ButtonRelease,
            rdev::EventType::MouseMove { x, y } => Self::MouseMove { x, y },
            rdev::EventType::Wheel { delta_x, delta_y } => Self::Wheel { delta_x, delta_y },
        }
    }
}

pub trait EventCaptureBackend: Send + Sync {
    fn availability(&self) -> BackendAvailability;

    fn listen(
        &self,
        _handler: Box<dyn FnMut(CapturedInputEvent) + Send + 'static>,
    ) -> FoundationResult<()> {
        Err(FoundationError::unavailable(
            "input capture is not active for this backend",
        ))
    }
}

#[derive(Debug, Default, Clone)]
pub struct RdevEventCaptureBackend;

#[cfg(feature = "rdev-capture")]
impl EventCaptureBackend for RdevEventCaptureBackend {
    fn availability(&self) -> BackendAvailability {
        BackendAvailability::available()
    }

    fn listen(
        &self,
        mut handler: Box<dyn FnMut(CapturedInputEvent) + Send + 'static>,
    ) -> FoundationResult<()> {
        rdev::listen(move |event| handler(event.into())).map_err(|err| {
            FoundationError::new(
                "foundation.capture_failed",
                format!("rdev input capture failed: {err:?}"),
            )
        })
    }
}

#[cfg(not(feature = "rdev-capture"))]
impl EventCaptureBackend for RdevEventCaptureBackend {
    fn availability(&self) -> BackendAvailability {
        BackendAvailability::unavailable(
            "rdev event capture is not compiled in; rebuild with feature rdev-capture",
        )
    }
}

#[derive(Debug, Clone)]
pub struct UnavailableEventCaptureBackend {
    reason: String,
}

impl UnavailableEventCaptureBackend {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl EventCaptureBackend for UnavailableEventCaptureBackend {
    fn availability(&self) -> BackendAvailability {
        BackendAvailability::unavailable(&self.reason)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    TypeText(String),
    PressEnter,
    PressEscape,
    Click { x: i32, y: i32 },
}

pub trait InputSynthesisBackend: Send {
    fn availability(&self) -> BackendAvailability;
    fn execute(&mut self, action: InputAction) -> FoundationResult<()>;
}

pub struct EnigoInputSynthesisBackend {
    enigo: Enigo,
}

impl EnigoInputSynthesisBackend {
    pub fn new() -> FoundationResult<Self> {
        let settings = Settings::default();
        let enigo = Enigo::new(&settings).map_err(|err| {
            FoundationError::new(
                "foundation.input_unavailable",
                format!("enigo input synthesis could not initialize: {err}"),
            )
        })?;
        Ok(Self { enigo })
    }
}

impl InputSynthesisBackend for EnigoInputSynthesisBackend {
    fn availability(&self) -> BackendAvailability {
        BackendAvailability::available()
    }

    fn execute(&mut self, action: InputAction) -> FoundationResult<()> {
        match action {
            InputAction::TypeText(text) => self.enigo.text(&text),
            InputAction::PressEnter => self.enigo.key(Key::Return, Direction::Click),
            InputAction::PressEscape => self.enigo.key(Key::Escape, Direction::Click),
            InputAction::Click { x, y } => self
                .enigo
                .move_mouse(x, y, Coordinate::Abs)
                .and_then(|_| self.enigo.button(Button::Left, Direction::Click)),
        }
        .map_err(|err| FoundationError::new("foundation.input_failed", err.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenshotArtifact {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
}

pub trait ScreenshotBackend: Send + Sync {
    fn availability(&self) -> BackendAvailability;
    fn capture_primary_monitor(&self, output: &Path) -> FoundationResult<ScreenshotArtifact>;
}

#[derive(Debug, Default, Clone)]
pub struct XcapScreenshotBackend;

impl ScreenshotBackend for XcapScreenshotBackend {
    fn availability(&self) -> BackendAvailability {
        match xcap::Monitor::all() {
            Ok(monitors) if !monitors.is_empty() => BackendAvailability::available(),
            Ok(_) => BackendAvailability::unavailable("xcap did not report any monitors"),
            Err(err) => {
                BackendAvailability::unavailable(format!("xcap monitor query failed: {err}"))
            }
        }
    }

    fn capture_primary_monitor(&self, output: &Path) -> FoundationResult<ScreenshotArtifact> {
        let monitors = xcap::Monitor::all().map_err(|err| {
            FoundationError::new(
                "foundation.screenshot_unavailable",
                format!("xcap monitor query failed: {err}"),
            )
        })?;
        let monitor = monitors
            .iter()
            .find(|monitor| monitor.is_primary().unwrap_or(false))
            .or_else(|| monitors.first())
            .ok_or_else(|| {
                FoundationError::new("foundation.screenshot_unavailable", "no monitor available")
            })?;
        let image = monitor.capture_image().map_err(|err| {
            FoundationError::new(
                "foundation.screenshot_failed",
                format!("xcap screenshot capture failed: {err}"),
            )
        })?;
        let width = image.width();
        let height = image.height();
        image.save(output).map_err(|err| {
            FoundationError::new(
                "foundation.screenshot_write_failed",
                format!("could not write screenshot: {err}"),
            )
        })?;
        Ok(ScreenshotArtifact {
            path: output.to_path_buf(),
            width,
            height,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpJsonRequest {
    pub method: String,
    pub url: String,
    pub body: Option<Value>,
}

pub trait HttpClient: Send + Sync {
    fn execute_json<'a>(
        &'a self,
        request: HttpJsonRequest,
    ) -> Pin<Box<dyn Future<Output = FoundationResult<Value>> + Send + 'a>>;
}

#[derive(Debug, Default, Clone)]
pub struct ReqwestHttpClient {
    client: reqwest::Client,
}

impl ReqwestHttpClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl HttpClient for ReqwestHttpClient {
    fn execute_json<'a>(
        &'a self,
        request: HttpJsonRequest,
    ) -> Pin<Box<dyn Future<Output = FoundationResult<Value>> + Send + 'a>> {
        Box::pin(async move {
            let method = request.method.parse().map_err(|err| {
                FoundationError::new("foundation.http_invalid_method", format!("{err}"))
            })?;
            let mut builder = self.client.request(method, &request.url);
            if let Some(body) = request.body {
                builder = builder.json(&body);
            }
            let response = builder.send().await.map_err(|err| {
                FoundationError::new(
                    "foundation.http_failed",
                    format!("HTTP request failed: {err}"),
                )
            })?;
            response.json::<Value>().await.map_err(|err| {
                FoundationError::new(
                    "foundation.http_json_failed",
                    format!("HTTP response was not valid JSON: {err}"),
                )
            })
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubprocessInvocation {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubprocessOutput {
    pub status_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub rendered_command: String,
}

#[derive(Debug, Clone, Default)]
pub struct SubprocessRunner {
    known_secret_values: Vec<String>,
}

impl SubprocessRunner {
    pub fn new(known_secret_values: Vec<String>) -> Self {
        Self {
            known_secret_values,
        }
    }

    pub fn rendered_command(&self, invocation: &SubprocessInvocation) -> String {
        let command = std::iter::once(invocation.program.clone())
            .chain(invocation.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ");
        redact_sensitive_text_with_values(&command, &self.known_secret_values)
    }

    pub fn run(&self, invocation: &SubprocessInvocation) -> FoundationResult<SubprocessOutput> {
        // Accepted risk: this foundation helper is the single reviewed boundary for caller-supplied subprocesses.
        // foxguard: ignore[rs/no-command-injection]
        let output = Command::new(&invocation.program)
            .args(&invocation.args)
            .stdin(Stdio::null())
            .output()
            .map_err(|err| {
                FoundationError::new(
                    "foundation.subprocess_failed",
                    format!(
                        "{} failed to start: {err}",
                        self.rendered_command(invocation)
                    ),
                )
            })?;
        Ok(SubprocessOutput {
            status_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: redact_sensitive_text_with_values(
                &String::from_utf8_lossy(&output.stderr),
                &self.known_secret_values,
            ),
            rendered_command: self.rendered_command(invocation),
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct JsonSchemaValidator;

impl JsonSchemaValidator {
    pub fn schema_for<T: JsonSchema>(&self) -> Value {
        serde_json::to_value(schema_for!(T)).unwrap_or(Value::Null)
    }

    pub fn validate(&self, schema: &Value, instance: &Value) -> FoundationResult<()> {
        jsonschema::validator_for(schema)
            .map_err(|err| {
                FoundationError::new(
                    "foundation.schema_invalid",
                    format!("JSON schema could not be compiled: {err}"),
                )
            })?
            .validate(instance)
            .map_err(|err| {
                FoundationError::new(
                    "foundation.schema_validation_failed",
                    format!("JSON value did not match schema: {err}"),
                )
            })
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn verify_ed25519_signature(
    public_key: &[u8; 32],
    message: &[u8],
    signature: &[u8; 64],
) -> FoundationResult<()> {
    let key = VerifyingKey::from_bytes(public_key).map_err(|err| {
        FoundationError::new(
            "foundation.signature_key_invalid",
            format!("invalid Ed25519 public key: {err}"),
        )
    })?;
    key.verify(message, &Signature::from_bytes(signature))
        .map_err(|err| {
            FoundationError::new(
                "foundation.signature_invalid",
                format!("Ed25519 signature verification failed: {err}"),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    #[cfg(feature = "rdev-capture")]
    use std::time::SystemTime;

    #[cfg(feature = "rdev-capture")]
    fn rdev_event(event_type: rdev::EventType, name: Option<&str>) -> rdev::Event {
        rdev::Event {
            time: SystemTime::UNIX_EPOCH,
            name: name.map(ToOwned::to_owned),
            event_type,
        }
    }

    #[test]
    fn foundation_error_and_availability_are_structured() {
        let error = FoundationError::unavailable("screen capture missing");

        assert_eq!(error.code, "foundation.unavailable");
        assert_eq!(
            error.to_string(),
            "foundation.unavailable: screen capture missing"
        );
        assert_eq!(
            BackendAvailability::available(),
            BackendAvailability {
                available: true,
                reason: None,
            }
        );
        assert_eq!(
            BackendAvailability::unavailable("missing permission"),
            BackendAvailability {
                available: false,
                reason: Some("missing permission".to_owned()),
            }
        );
    }

    #[test]
    #[cfg(feature = "rdev-capture")]
    fn rdev_events_are_normalised_without_backend_state() {
        assert_eq!(
            CapturedInputEvent::from(rdev_event(
                rdev::EventType::KeyPress(rdev::Key::KeyA),
                Some("a"),
            )),
            CapturedInputEvent::KeyPress {
                name: Some("a".to_owned()),
            }
        );
        assert_eq!(
            CapturedInputEvent::from(rdev_event(
                rdev::EventType::KeyRelease(rdev::Key::KeyA),
                None
            )),
            CapturedInputEvent::KeyRelease { name: None }
        );
        assert_eq!(
            CapturedInputEvent::from(rdev_event(
                rdev::EventType::MouseMove { x: 12.0, y: 34.0 },
                None,
            )),
            CapturedInputEvent::MouseMove { x: 12.0, y: 34.0 }
        );
        assert_eq!(
            CapturedInputEvent::from(rdev_event(
                rdev::EventType::Wheel {
                    delta_x: 1,
                    delta_y: -2,
                },
                None,
            )),
            CapturedInputEvent::Wheel {
                delta_x: 1,
                delta_y: -2,
            }
        );
        assert_eq!(
            CapturedInputEvent::from(rdev_event(
                rdev::EventType::ButtonPress(rdev::Button::Left),
                None,
            )),
            CapturedInputEvent::ButtonPress
        );
        assert_eq!(
            CapturedInputEvent::from(rdev_event(
                rdev::EventType::ButtonRelease(rdev::Button::Left),
                None,
            )),
            CapturedInputEvent::ButtonRelease
        );
    }

    #[test]
    fn real_backends_report_availability_without_mutating_state() {
        let rdev_availability = RdevEventCaptureBackend.availability();
        if cfg!(feature = "rdev-capture") {
            assert!(rdev_availability.available);
        } else {
            assert!(!rdev_availability.available);
            assert!(rdev_availability
                .reason
                .as_deref()
                .unwrap_or_default()
                .contains("rdev event capture is not compiled in"));
        }
        let screenshot = ScreenshotArtifact {
            path: PathBuf::from("/tmp/example.png"),
            width: 10,
            height: 20,
        };

        assert_eq!(screenshot.path, PathBuf::from("/tmp/example.png"));
        assert_eq!(screenshot.width, 10);
        assert_eq!(screenshot.height, 20);
    }

    #[test]
    fn subprocess_runner_redacts_secrets_from_rendered_command_and_stderr() {
        let runner = SubprocessRunner::new(vec!["cleartext-secret".to_owned()]);
        let invocation = SubprocessInvocation {
            program: "echo".to_owned(),
            args: vec![
                "Authorization: Bearer sk-test-1234".to_owned(),
                "api_key=cleartext-secret".to_owned(),
            ],
        };

        let rendered = runner.rendered_command(&invocation);

        assert!(!rendered.contains("sk-test-1234"));
        assert!(!rendered.contains("cleartext-secret"));
        assert!(rendered.contains("[REDACTED]"));
    }

    #[test]
    fn subprocess_runner_returns_output_and_start_errors() {
        let runner = SubprocessRunner::default();
        let output = runner
            .run(&SubprocessInvocation {
                program: "printf".to_owned(),
                args: vec!["hello".to_owned()],
            })
            .expect("printf should run");

        assert_eq!(output.status_code, Some(0));
        assert_eq!(output.stdout, "hello");
        assert!(output.stderr.is_empty());
        assert_eq!(output.rendered_command, "printf hello");

        let err = runner
            .run(&SubprocessInvocation {
                program: "greentic-missing-command-for-test".to_owned(),
                args: Vec::new(),
            })
            .expect_err("missing command should fail");
        assert_eq!(err.code, "foundation.subprocess_failed");
        assert!(err.message.contains("failed to start"));
    }

    #[test]
    fn unavailable_capture_backend_reports_reason_without_fake_events() {
        let backend = UnavailableEventCaptureBackend::new("event tap not installed");

        let availability = backend.availability();

        assert!(!availability.available);
        assert_eq!(
            availability.reason.as_deref(),
            Some("event tap not installed")
        );
        assert!(backend.listen(Box::new(|_| {})).is_err());
    }

    #[test]
    fn xcap_screenshot_backend_reports_availability_or_concrete_reason() {
        let availability = XcapScreenshotBackend.availability();

        assert!(availability.available || availability.reason.is_some());
    }

    #[derive(JsonSchema, Serialize)]
    struct Example {
        name: String,
    }

    #[test]
    fn json_schema_validator_uses_generated_schema() {
        let validator = JsonSchemaValidator;
        let schema = validator.schema_for::<Example>();
        let good = serde_json::json!({ "name": "Ada" });
        let bad = serde_json::json!({ "name": 42 });

        assert!(validator.validate(&schema, &good).is_ok());
        assert!(validator.validate(&schema, &bad).is_err());

        let invalid_schema = serde_json::json!({ "type": "not-a-json-schema-type" });
        let err = validator
            .validate(&invalid_schema, &good)
            .expect_err("invalid schema should fail");
        assert_eq!(err.code, "foundation.schema_invalid");
    }

    #[test]
    fn sha256_hex_uses_standard_digest_shape() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn ed25519_signature_errors_are_structured() {
        let message = b"hello";
        let invalid_public_key = [0_u8; 32];
        let signature = [0_u8; 64];

        let err = verify_ed25519_signature(&invalid_public_key, message, &signature)
            .expect_err("zero key should not verify zero signature");

        assert!(
            err.code == "foundation.signature_key_invalid"
                || err.code == "foundation.signature_invalid"
        );
    }
}
