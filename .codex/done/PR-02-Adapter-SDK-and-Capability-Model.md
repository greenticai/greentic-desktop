# PR-02 Adapter SDK and Capability Model

## Goal

Define a universal adapter contract that supports web apps, Windows desktop apps, Java apps, terminal/mainframe systems, Office automation and vision fallback.

## Design Principle

The core runner should not care whether the target is:

- Browser DOM
- Windows UI Automation
- Java accessibility tree
- VT100 terminal
- TN3270 mainframe
- Screenshot-only remote desktop

It should call logical capabilities.

## Core Trait

```rust
#[async_trait::async_trait]
pub trait DesktopAdapter: Send + Sync {
    async fn capabilities(&self) -> AdapterCapabilities;
    async fn observe(&self, ctx: ObserveContext) -> Result<Observation>;
    async fn execute(&self, step: RunnerStep) -> Result<StepResult>;
    async fn validate(&self, assertion: Assertion) -> Result<AssertionResult>;
    async fn record_event(&self) -> Result<Option<RecordedEvent>>;
}
```

## Capability Model

```json
{
  "adapter_id": "greentic.desktop.playwright",
  "version": "1.0.0",
  "capabilities": [
    "web.goto",
    "web.click",
    "web.fill",
    "web.extract_text",
    "web.assert_visible",
    "evidence.screenshot"
  ]
}
```

## Generic Step Model

```yaml
- id: fill_email
  action: fill
  target:
    label: Email
  value: "{{inputs.email}}"
```

The adapter maps this to concrete behaviour.

## Locator Strategy

Each target supports several locator strategies:

```yaml
target:
  preferred:
    automation_id: SaveCustomerButton
  fallback:
    text: Save
    region: bottom_right
  visual_fallback:
    image: baselines/save_button.png
```

## Acceptance Criteria

- All adapters expose capabilities in the same format.
- Runner validation can check whether required capabilities are installed.
- Runner can select the best adapter automatically.
- Unsupported steps fail before execution, not halfway through a run.
