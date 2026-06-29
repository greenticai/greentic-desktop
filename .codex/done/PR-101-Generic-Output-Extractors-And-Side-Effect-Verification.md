# PR-101 - Generic Output Extractors and Side Effect Verification

## Goal

Make runner success depend on declared outputs, assertions, and side effects being proven by adapter observations or filesystem/network evidence.

## User Outcome

If a runner claims it created `/tmp/test.docx`, Greentic verifies that file exists and attaches evidence. If it cannot prove the output, the test fails.

## Current Evidence

- Output extraction has relied on visible text and previously on step messages.
- Typed output extractors exist but are not consistently enforced across flat YAML, typed manifests, GUI tests, and MCP calls.

## Scope

1. Make typed `RunnerOutput`/`WorkflowOutputExtractor` mandatory for saved runners.
2. Add generic extractors:
   - visible text
   - regex
   - accessibility property
   - file exists
   - file content/metadata
   - download artifact
   - terminal field
   - screenshot OCR
3. Add `failure_behavior` enforcement:
   - fail runner
   - warn
   - optional output
4. Require every declared output to be extracted or explicitly optional.
5. Verify local path outputs exist.
6. Persist output proof artifacts in evidence.
7. Update GUI Test Runner to show extractor status per output.
8. Update MCP response to include extractor proof or failure details.

## Acceptance Tests

1. `/tmp/missing.docx` output fails.
2. Existing file output passes and evidence includes file metadata.
3. Visible text output passes only when adapter observation contains matching text.
4. Optional outputs warn but do not fail.
5. MCP and GUI return identical output validation behavior.

