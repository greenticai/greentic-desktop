# PR-56 - Recorder-Derived Inputs, Secrets, Outputs and Questions

Goal: make recordings produce generic runner drafts by deriving inputs, secrets, outputs, redaction rules, output extractors, and open questions from user markings and observations.

Problem
The recorder currently defaults recorded packages to `inputs.customer_id` and `secrets.password`. That makes recordings appear CRM-specific and prevents broad reuse across arbitrary desktop apps.

Design
Extend recording sessions with explicit user markings:

- mark selected value as input
- mark selected value as secret
- mark visible value as output
- mark screen/text/field as assertion
- mark action as submit
- mark step as optional or retry-safe
- mark region as visual locator
- add open question for missing context

Recorder data model
Add structured recording annotations:

- `RecordedInputCandidate`
- `RecordedSecretCandidate`
- `RecordedOutputCandidate`
- `RecordedAssertionCandidate`
- `RedactionRule`
- `OutputExtractorCandidate`
- `OpenQuestion`

Derivation
During normalization:

- values typed by a human can become literal constants unless marked as inputs or secrets
- repeated or placeholder-like values can be suggested as inputs
- labels containing password/token/secret/key become secret candidates
- copied/read/visible completion values become output candidates
- screenshots and UI trees become evidence and locator sources
- ambiguous cases become open questions

Redaction
Redaction must apply to raw events, normalized steps, screenshots metadata, evidence bundles, and runner package rendering. Secrets should be referenced by name and never written as values.

Acceptance criteria
Recorded packages no longer receive default CRM input/secret fields.
Recording APIs can mark inputs, secrets, outputs, assertions, and submit actions.
Normalization emits open questions when required fields are not known.
Output extractors are generated from marked visible text, terminal fields, web locators, native locators, Java components, or vision regions.
Tests cover web, native desktop, Java, terminal, and vision recording examples without CRM defaults.
