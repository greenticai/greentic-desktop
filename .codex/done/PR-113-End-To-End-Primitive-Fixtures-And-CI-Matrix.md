# PR-113 - End-to-End Primitive Fixtures and CI Matrix

## Goal

Add real end-to-end tests proving primitive workflows work across supported technologies and operating systems.

## User Outcome

The product stops passing tests while core prompting/recording workflows are unusable.

## Current Evidence

- Automated tests previously passed while the Word save workflow still failed.
- Existing tests often verify fail-closed behavior, not useful successful automation.

## Scope

1. Add fixture apps:
   - macOS document fixture.
   - Windows UIA document fixture.
   - Linux X11 document fixture.
   - Java Swing document fixture.
   - Web document fixture.
   - terminal text fixture.
2. Add E2E flows:
   - create document.
   - type content.
   - save as path.
   - assert file exists.
   - return file path.
3. Add spreadsheet/table E2E flow:
   - append row.
   - save.
   - verify content.
4. Add recording E2E:
   - record document creation.
   - normalize into primitives.
   - replay successfully.
5. CI gates:
   - always run web and non-GUI file fallback.
   - run OS-native tests on matching OS runners.
   - fail if primitive workflow uses raw mock/model-only execution.

## Out of Scope

- Full external app coverage.
- Paid/proprietary app dependencies in CI.

## Acceptance Tests

1. Prompt-generated document workflow succeeds in at least one real fixture per OS.
2. Recorded document workflow replays successfully in at least one real fixture per OS.
3. Spreadsheet append workflow verifies file contents.
4. CI fails if a runner reports success without proof.
5. The local test runner surfaces the same evidence as CI.

