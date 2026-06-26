# PR-59 - Real Replay Dispatch, Assertions and Output Extraction

Goal: make replay execute adapters, resolve values, run assertions, extract real outputs, and return evidence-backed structured results.

Problem
Replay currently validates capabilities and resolves placeholders, but it does not dispatch steps to adapters. Outputs are placeholders like `"resolved"`, so MCP calls cannot represent real desktop results.

Design
Introduce an adapter registry for replay:

- `AdapterRegistry`
- `AdapterHandle`
- `ReplayAdapterSelector`
- `ReplayExecutionContext`

Replay pipeline

1. Validate package and session profile.
2. Resolve input and secret templates.
3. Select adapter for each step.
4. Execute step on adapter.
5. Collect `StepResult`, observations, screenshots, and evidence.
6. Run assertions through adapter validation.
7. Execute output extractors:
   - text locator
   - regex over visible text
   - web selector text
   - native UI element text/value
   - Java component text
   - terminal field
   - vision OCR region
   - downloaded file path
   - JSON/object extraction
8. Return typed outputs and evidence refs.

Failure behavior
Replay should support:

- stop or continue on failure
- retry policy for idempotent/safe steps
- human approval before unsafe submit
- structured failures with evidence URI
- partial output reporting when policy allows it

Acceptance criteria
Replay uses real `DesktopAdapter::execute`, `observe`, and `validate`.
Outputs are extracted from adapter observations or extractor-specific APIs.
MCP calls return actual output values, not `"resolved"`.
Evidence contains per-step trace, assertions, output extraction proof, and failure reason.
Tests cover successful extraction and failing assertions across web, native, Java, terminal, and vision adapters.
