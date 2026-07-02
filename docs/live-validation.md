# Live Desktop Validation

Live validation is the local proof path for desktop automation. Normal CI can check schemas, replay models, and deterministic fixtures, but it cannot prove that Microsoft Excel, Word, Calculator, or a user desktop session actually accepted input and produced side effects. Use live validation when changing adapters, replay, recording, or runner generation.

## Command

Run a workflow and assert real desktop side effects:

```sh
cargo run --bin greentic-desktop -- desktop validate \
  --workflow examples/runners/macos-excel-tabs-formula-save.yaml \
  --input workbook_path=/tmp/greentic-test.xls \
  --input source_number=10 \
  --expect-file-changed /tmp/greentic-test.xls \
  --expect-no-modal \
  --json
```

The command fails if the runner fails, the expected file is missing or unchanged, an expected output is wrong, or a blocking modal remains open.

## Assertions

- `--expect-file PATH`: the file must exist after the run.
- `--expect-file-changed PATH`: the file must be created or have changed size/timestamp during the run.
- `--expect-output KEY=VALUE`: the runner output JSON must contain the exact string value.
- `--expect-no-modal`: the desktop must not have a blocking modal after the run.
- `--expect-frontmost-app APP`: the frontmost app must match the expected app name.
- `--json`: emit a structured validation summary with runner output, steps, live state, and assertion failures.

## Local Check Integration

`ci/local_check.sh` does not run GUI desktop automation by default. To include live validation:

```sh
GREENTIC_LIVE_DESKTOP_TESTS=1 bash ci/local_check.sh
```

The live check writes logs to `target/greentic-live-validation/`.

For the macOS Excel fixture:

```sh
GREENTIC_LIVE_DESKTOP_TESTS=1 \
GREENTIC_LIVE_EXCEL=1 \
GREENTIC_LIVE_EXCEL_WORKBOOK=/tmp/greentic-live.xls \
bash ci/local_check.sh
```

## macOS Permissions

If Greentic is launched with `cargo run` inside Terminal, VS Code, Cursor, or another shell, macOS permissions apply to that launcher. Grant Accessibility, Screen Recording, and Input Monitoring to the launcher or to the installed Greentic app. Restart the app or terminal after changing permissions if macOS asks for it.

## Evidence

Live validation is designed to replace manual screenshots. Failures should include:

- failed step id.
- runner evidence reference.
- file assertion status.
- frontmost app.
- blocking modal summary and buttons when available.

Screenshots and richer OS snapshots are tracked by the follow-up live probe and evidence PRs.
