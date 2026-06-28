# PR-71 - Recording End-to-End Test Matrix

## Goal

Prove recording works end to end across target types with automated tests, not manual optimism.

## Problem

Several previous checks passed while the product still had placeholder recording behavior. We need tests that fail when recording does not capture real events, normalize them, replay them, and return outputs.

## Scope

Add an automated recording test matrix:

| Target | Fixture | Required environment | Expected output |
| --- | --- | --- | --- |
| Web | Playwright calculator page | CI all OS | `result=2` |
| Web secret | login fixture | CI all OS | password redacted |
| Native macOS | Calculator or fixture app | macOS runner/manual gate | `result=2` |
| Native Windows | Calculator or fixture app | Windows runner/manual gate | `result=2` |
| Native Linux X11 | GTK/sample app under Xvfb | Linux CI | `result=2` |
| Java | Swing calculator fixture | JDK + access bridge | `result=2` |
| Terminal | PTY calculator script | CI all OS where PTY supported | `result=2` |
| Remote | canvas/vision remote fixture | CI all OS | `result=2` |

## Required Assertions

Each test must verify:

1. Recording backend preflight passes.
2. Start recording returns active capture state.
3. Raw event count is greater than zero.
4. Evidence artifacts are created where expected.
5. Normalization produces a runner with non-empty semantic steps.
6. Runner declares expected inputs/outputs.
7. Replay/test returns expected output.
8. No placeholder strings such as `sample-output`, `recording.recorded`, or CRM defaults appear unless the fixture is CRM-specific.

## CI Integration

- Fast CI runs web, terminal, remote fixture, and pure normalization tests.
- OS-specific jobs run native tests.
- Permission-heavy tests can be `manual` or nightly initially, but must exist and produce clear skip reasons.
- Every skip must be explicit, not a silent pass.

## Acceptance Criteria

- Failing to capture real events fails at least one E2E test.
- Failing to normalize into semantic runner steps fails at least one E2E test.
- Failing to replay and return output fails at least one E2E test.
- The LLM provider list and setup checklist regressions are covered by GUI tests in the same suite.

## Done Means

The repo has automated evidence that recording works for the target classes Greentic advertises.

