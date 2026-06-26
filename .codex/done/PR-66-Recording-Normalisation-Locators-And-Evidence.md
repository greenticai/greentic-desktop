# PR-66 - Recording Normalisation, Locators, Redaction, and Evidence

## Goal

Turn raw captured event streams into reliable generic `DesktopWorkflow` runner drafts with stable locators, parameterized inputs/secrets, output extractors, assertions, evidence, and open questions.

## Problem

`normalise_recording` currently reads ad-hoc JSON lines, skips lifecycle events, and maps remaining lines into `recording.<action>` steps with empty targets. It does not use structured locators, evidence, screenshots, UI trees, browser selectors, terminal buffers, output extractors, assertions, or confidence.

## User Outcome

After recording, the review screen shows meaningful steps:

- "Open Calculator"
- "Type input `number_1` into focused numeric field"
- "Click button `+`"
- "Read result text as `result`"

The generated runner is generic and replayable across supported environments when equivalent locators exist.

## Design

Add a normalization pipeline:

```text
raw events
  -> redact
  -> segment into user intents
  -> merge duplicate/low-level events
  -> resolve semantic targets
  -> infer inputs/secrets/constants
  -> infer output extractors/assertions
  -> emit DesktopWorkflow/runner schema
  -> write evidence manifest
```

## Implementation Plan

1. Replace string parsing in `normalise_recording` with typed serde event parsing.
2. Add event segmentation:
   - collapse keydown/keyup into text commit
   - merge click + focus + value change into a fill/select action
   - associate screenshots before/after actions
   - detect submit actions
3. Add locator ranking:
   - web: role/name, label, test id, text, CSS, XPath
   - Windows: AutomationId, Name, ControlType, ClassName, window/app
   - macOS: AXIdentifier, AXRole, AXTitle, AXDescription, bundle ID, window
   - Linux: AT-SPI role/name/path, app/window, fallback region
   - terminal: row/column/field, prompt, regex, command boundary
   - vision: region/image/text with confidence
4. Add redaction:
   - redact before writing raw events where possible
   - apply secondary redaction during normalization
   - store redaction decisions in evidence metadata
5. Add input/secret/output derivation:
   - repeated user-entered values become input candidates
   - password/token/key fields become secrets
   - final observed values after submit become output candidates
   - copied/read values become output candidates
6. Add output extractors:
   - web locator text
   - native accessibility text/value
   - terminal region/regex
   - visual OCR/region
7. Emit open questions for ambiguity:
   - weak locator
   - unsupported OS capability
   - no output observed
   - value may be secret
   - coordinate-only replay fallback

## Evidence

Write an `evidence/manifest.json` for each recording:

- screenshots
- browser traces
- terminal transcripts
- UI tree snapshots
- selected locator candidates
- redaction summary
- normalized step confidence

The GUI should load evidence through local API artifact endpoints, not direct file paths.

## Reuse Existing Libraries

- `serde` and `serde_json` for event parsing.
- Existing `greentic-desktop-runner-schema` and `greentic-desktop-workflow` types for output, not custom YAML builders.
- Existing evidence crate for artifact references.
- `regex` only for user-visible output extractors and terminal derivation if already accepted by the workspace.

## Acceptance Criteria

- Normalization uses typed events and rejects malformed events with actionable warnings.
- Recorded web/native/terminal examples emit real semantic actions, not `recording.<action>`.
- Generated runner steps include stable locators and required capabilities.
- Inputs, secrets, outputs, assertions, and open questions are derived from events plus user markers.
- Evidence manifest links each normalized step to supporting raw events and screenshots/traces.

## Test Plan

- Golden tests for raw web events -> web workflow.
- Golden tests for raw Windows/macOS/Linux accessibility events -> native workflow.
- Golden tests for terminal transcript -> command workflow with output extractor.
- Redaction tests prove secrets are absent from raw JSONL, normalized YAML, evidence metadata, and GUI previews.
- Fuzz-ish malformed JSONL tests ensure partial recordings can still be recovered.

## Risks

- Over-normalization can hide real user behavior. Keep raw evidence links.
- Locator ranking is critical; weak locators should remain visible as warnings.
- Some apps expose poor accessibility metadata. Vision fallback must be explicit and confidence-scored.

