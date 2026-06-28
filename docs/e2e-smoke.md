# End-To-End Smoke Checklist

Use this checklist for release validation and QA.

1. Start with a fresh runtime home:

   ```bash
   GREENTIC_DESKTOP_HOME="$(mktemp -d)" greentic-desktop gui --no-open --bind 127.0.0.1:0
   ```

2. Open the printed Automate Hub URL.
3. Confirm the home page shows runtime info and setup checklist items.
4. Open **Settings** and verify recommended extensions load.
5. Install or verify `greentic.desktop.playwright`.
6. Open **Create**, generate a prompt runner for a generic resource/table task:

   ```text
   Open a resource table, ask for resource_name, name, and email, append a row, save, and return saved_status.
   ```

   Confirm the draft declares `resource_name`, `name`, and `email` inputs, a `saved_status` output, and an output extractor or assertion.
7. Confirm the runner appears under **My Runners**.
8. Run the runner with non-empty values and confirm the result shows `saved_status` plus an evidence reference. The output must not be `sample-output`, a fixed company name, or any value unrelated to the provided inputs.
9. Start or restart the managed MCP service from **My Runners** and confirm the runner appears in MCP `tools/list`.
10. Call the MCP tool with the same input fields and confirm it returns the same output/evidence contract as the Run button.
11. Delete or disable the runner and confirm it is no longer exposed through MCP.
12. Restart the app with the same runtime home and confirm runners, MCP server state, evidence, and extension state persist.

Before release, run the generic recording matrix:

```bash
cargo test -p greentic-desktop-test-harness recording_e2e_matrix_normalises_semantic_runners_without_placeholders
```

This test fails if normalized runners contain fabricated placeholders such as `sample-output`, CRM defaults, or product-specific calculator shortcuts.

For release CI, `ci/gui_smoke.sh` starts the built GUI binary, verifies `/`, verifies `/api/v1/health`, and terminates the process cleanly.
