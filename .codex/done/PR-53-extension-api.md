# PR-53 - Extension Manager API for Web UI

Goal: expose extension install/update/remove to the simple web interface.

Since you want a web UI for users who do not know CLI, the extension manager needs an API layer.

API endpoints
GET  /api/v1/extensions/recommended
GET  /api/v1/extensions/installed
GET  /api/v1/extensions/search?q=browser
GET  /api/v1/extensions/:id
GET  /api/v1/extensions/:id/versions
POST /api/v1/extensions/install
POST /api/v1/extensions/:id/update
POST /api/v1/extensions/:id/remove
POST /api/v1/extensions/:id/enable
POST /api/v1/extensions/:id/disable
POST /api/v1/extensions/:id/verify
POST /api/v1/extensions/:id/health
Install request
{
  "source": "store://greentic.desktop.playwright"
}
Install response
{
  "status": "installed",
  "id": "greentic.desktop.playwright",
  "version": "1.0.0",
  "capabilities": [
    "web.goto",
    "web.click",
    "web.fill"
  ],
  "needs_restart": false
}
Acceptance criteria
Web UI can list recommended extensions.
Web UI can install an extension without showing OCI details.
Web UI can show progress: resolving, downloading, verifying, installing, complete.
Web UI can show permission prompts in plain English.
Web UI can test extension health after install.
The endpoint names and DTOs match PR-37 and PR-38.
