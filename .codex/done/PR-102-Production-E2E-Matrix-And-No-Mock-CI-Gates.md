# PR-102 - Production E2E Matrix and No-Mock CI Gates

## Goal

Add CI and local checks that prove core flows work end-to-end with real backends or explicitly fail/skip with a documented environment limitation.

## User Outcome

Green tests mean Greentic actually works for the tested target class, not that a mock accepted steps.

## Current Evidence

- Existing automated tests allowed empty/fake provider paths to pass.
- Some tests assert model behavior rather than real side effects.

## Scope

1. Add no-mock static scan:
   - production code cannot instantiate fake/model adapters for run/test.
   - product strings like `"step accepted"` cannot be success criteria.
2. Add E2E fixtures:
   - web local form
   - macOS fixture app
   - Windows fixture app
   - Linux GTK/Qt fixture app under Xvfb
   - Java Swing fixture
   - terminal PTY fixture
   - vision screenshot/OCR fixture
   - remote viewport fixture
3. Add Playwright GUI tests that:
   - create prompt runner
   - enter inputs
   - run test
   - verify side effects/evidence
   - save runner
   - run via MCP
4. Add platform-specific CI jobs with explicit capability reporting.
5. Require local `ci/local_check.sh` to run no-mock checks and available real fixture tests.
6. Make skipped tests print exact missing permission/dependency.

## Acceptance Tests

1. CI fails if production run/test uses `CapabilityOnlyAdapter`, `StaticAdapter`, or `FakeRecordingBackend::ready`.
2. Web E2E creates real DOM side effects and extracts output.
3. Linux X11 E2E runs under Xvfb and creates a real file.
4. macOS/Windows E2Es run where permissions allow; otherwise fail with documented setup in non-CI local mode.
5. GUI and MCP run the same runner and return the same output/evidence.

