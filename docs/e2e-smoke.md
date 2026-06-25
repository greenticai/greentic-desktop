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
6. Open **Create**, generate a prompt runner, test it, and save it.
7. Confirm the runner appears under **My Runners**.
8. Test the runner and open the generated evidence summary.
9. Publish the runner as an MCP tool. If approval is required, approve it from the runner page.
10. Open **MCP Tools**, start the MCP service, and confirm the tool appears.
11. Copy the tool name or client configuration.
12. Disable the tool and confirm it is not exposed through MCP `tools/list`.
13. Re-enable it, test it, and confirm an evidence reference is shown.
14. Restart the app with the same runtime home and confirm runners, MCP publication state, approvals, evidence, and extension state persist.

For release CI, `ci/gui_smoke.sh` starts the built GUI binary, verifies `/`, verifies `/api/v1/health`, and terminates the process cleanly.
