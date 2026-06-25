# PR-52 - Extension Signing, Verification and Trust Policy

Goal: make extension installation safe enough for desktop automation.

Desktop extensions are powerful because they may control keyboard, mouse, screenshots, files or apps. They need a real trust model.

Trust policy
[extensions.trust]
allow_unsigned = false
allow_local_unsigned_drafts = true
trusted_publishers = ["greenticai"]

[extensions.permissions]
require_approval_for_screen_capture = true
require_approval_for_keyboard_mouse = true
require_approval_for_filesystem_write = true
Verification checks
Artifact digest matches expected digest.
Signature is valid.
Publisher is trusted.
Manifest is valid.
Requested permissions are allowed.
Platform compatibility matches current machine.
Extension binary/sidecar path is valid.
SBOM is present for production packages.
GUI integration
The Automate Hub extension install flow must surface trust-policy results before installation. Permission prompts should be rendered in plain English, grouped by risk area such as screen capture, keyboard/mouse control, filesystem write, network access, and native binary execution. Blocked installs must show the exact policy reason and should not leave partially installed files.

Acceptance criteria
Unsigned extensions are refused in production mode.
Local unsigned development extensions can be allowed in dev mode.
Trust policy can block high-risk extensions.
User gets clear explanation when install is blocked.
Verification result is stored in installed extension metadata.
