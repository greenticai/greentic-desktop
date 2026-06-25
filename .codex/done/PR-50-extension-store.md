# PR-50 - Extension Store Index and Friendly Aliases

Goal: let users install friendly names instead of long OCI URLs.

This is what makes the web UI and CLI pleasant.

Store index example
{
  "extensions": [
    {
      "id": "greentic.desktop.playwright",
      "name": "Playwright Web Adapter",
      "description": "Automate browser-based applications.",
      "latest": "1.0.0",
      "source": "oci://ghcr.io/greenticai/greentic-desktop/extensions/playwright:1.0.0",
      "platforms": ["windows", "macos", "linux"],
      "capabilities": [
        "web.goto",
        "web.click",
        "web.fill",
        "web.extract_text"
      ]
    }
  ]
}
Commands
gtc desktop extension search browser
gtc desktop extension install greentic.desktop.playwright
gtc desktop extension install playwright
gtc desktop extension versions greentic.desktop.playwright
Resolution flow
playwright
  → greentic.desktop.playwright
  → store://greentic.desktop.playwright
  → oci://ghcr.io/...
GUI integration
The Automate Hub Settings > Extensions page must consume this index through the GUI API. The UI should show recommended extensions, search results, available versions, platform compatibility, publisher, permissions, and install/update/remove actions without exposing raw OCI URLs unless advanced details are expanded.

Acceptance criteria
Users can install extensions by friendly ID.
Store index supports latest version, available versions and platform compatibility.
Store index can point to GHCR OCI artifacts.
Tenant/private store index can override or add extensions.
UI can call one simple endpoint to list recommended extensions.
