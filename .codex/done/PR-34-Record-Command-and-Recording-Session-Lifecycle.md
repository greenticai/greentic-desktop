# PR-34 Record Command and Recording Session Lifecycle

## Goal

Add a working `gtc desktop record` command that starts, manages and finalises a recording session so PR-10 can be executed from the CLI.

This PR turns the recording engine from an internal concept into a usable workflow.

## Background

PR-01 defines the core CLI commands but does not include a record command.

PR-10 defines recording modes and the event model, but it does not define the CLI lifecycle needed to start, pause, resume, stop, normalise and save recordings.

This PR fills that gap.

## CLI Commands

```bash
gtc desktop record start \
  --name crm.create_customer \
  --profile local-crm \
  --adapter greentic.desktop.playwright \
  --out ./recordings/crm.create_customer

gtc desktop record pause --session <session_id>
gtc desktop record resume --session <session_id>
gtc desktop record stop --session <session_id>
gtc desktop record cancel --session <session_id>
gtc desktop record status --session <session_id>
gtc desktop record list

gtc desktop record normalise \
  --recording ./recordings/crm.create_customer/raw \
  --out ./runners/crm.create_customer.draft.yaml

gtc desktop record finalise \
  --recording ./recordings/crm.create_customer \
  --runner ./runners/crm.create_customer.draft.yaml
```

## Recording Lifecycle

```text
start
  → create recording session
  → attach adapters
  → capture desktop/app context
  → stream raw events
  → pause/resume as needed
  → stop
  → normalise raw events
  → redact secrets
  → infer inputs/outputs/assertions
  → save draft runner package
  → write evidence bundle
```

## Session State Model

```rust
pub enum RecordingSessionState {
    Starting,
    Recording,
    Paused,
    Stopping,
    Normalising,
    Completed,
    Cancelled,
    Failed,
}
```

## Session Manifest

Each recording session should write a manifest.

```yaml
session_id: rec_01J...
name: crm.create_customer
profile: local-crm
state: recording
started_at: "2026-06-25T10:00:00Z"
adapters:
  - greentic.desktop.playwright
platform:
  os: linux
  display_server: x11
paths:
  raw_events: raw/events.jsonl
  screenshots: evidence/screenshots/
  normalised_steps: normalised/steps.yaml
  draft_runner: runner.draft.yaml
```

## Raw Event Stream

Raw events should be append-only JSONL so recordings survive crashes.

```json
{"type":"session_started","timestamp":"...","adapter":"greentic.desktop.playwright"}
{"type":"click","timestamp":"...","target":{"text":"New Customer"},"screenshot_ref":"evidence://..."}
{"type":"type_text","timestamp":"...","redacted":true,"field_hint":"email"}
{"type":"assert_visible","timestamp":"...","target":{"text":"Customer created"}}
```

## Adapter Requirements

A recordable adapter must implement:

```rust
#[async_trait::async_trait]
pub trait RecordableAdapter: DesktopAdapter {
    async fn start_recording(&self, ctx: RecordingContext) -> Result<()>;
    async fn pause_recording(&self, session_id: RecordingSessionId) -> Result<()>;
    async fn resume_recording(&self, session_id: RecordingSessionId) -> Result<()>;
    async fn stop_recording(&self, session_id: RecordingSessionId) -> Result<RecordingSummary>;
}
```

Adapters that cannot stream structured events may still participate through screenshots and input hooks if the platform allows it.

## Normalisation

The normaliser converts raw events into logical runner steps.

```text
raw click at x/y
  → locate UI element
  → prefer stable selector/accessibility metadata
  → attach visual fallback
  → emit logical runner step
```

Normalisation should produce:

- Stable selectors.
- Fallback selectors.
- Input placeholders.
- Secret placeholders.
- Assertions.
- Extracted outputs.
- Timing hints only where needed.
- Evidence references.

## Secret and PII Handling

The record command must support redaction during capture.

```bash
gtc desktop record start \
  --name crm.create_customer \
  --redact text,password,email,token \
  --secret-fields password,api_key
```

Rules:

- Password fields are never stored as plaintext.
- Tokens and obvious secrets are redacted.
- User can mark the next typed value as a secret.
- Redacted values become runner inputs or secret references.

## Interactive Controls

Provide local controls for recording sessions:

```bash
gtc desktop record mark-input company_name
gtc desktop record mark-secret password
gtc desktop record mark-output customer_id
gtc desktop record add-assertion "Customer created"
gtc desktop record note "This dialog appears only for new customers"
```

These commands let the user improve the recording without editing YAML manually.

## Failure Handling

The command should handle:

- Adapter not recordable.
- Missing desktop permissions.
- App/window not found.
- Session already recording.
- Interrupted process.
- Crash recovery from JSONL event stream.
- Normalisation failure.
- Evidence write failure.

## Tests

Add tests for:

- Start/stop recording lifecycle.
- Pause/resume lifecycle.
- Cancel lifecycle.
- JSONL raw event persistence.
- Crash recovery from partial raw event log.
- Redaction of typed secrets.
- Normalisation into draft runner YAML.
- Adapter that supports replay but not recording.
- Missing permissions diagnostics.

## Acceptance Criteria

- `gtc desktop record start` creates a recording session manifest.
- `gtc desktop record stop` closes the session and flushes raw events.
- `gtc desktop record normalise` converts raw events into draft runner YAML.
- Secrets and sensitive typed values are redacted.
- Recording can be paused, resumed and cancelled.
- A failed recording can be inspected and partially recovered.
- A completed recording can be replayed through the existing replay engine once validated.
