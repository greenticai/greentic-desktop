# PR-134 - Vision And Remote Adapters Use Xcap Imageproc OCR And Real Protocols

## Goal

Make vision fallback and remote desktop adapters real capture/protocol implementations rather than screenshot placeholders or model-only contracts.

## User Outcome

Vision can be used as an honest fallback with explicit confidence and evidence, and remote desktop flows use real RDP/VNC protocols or fail closed.

## Current Evidence

- Vision fallback is useful, but it cannot prove side effects without structured assertions and evidence.
- Remote desktop support needs real viewport/protocol backends.

## Scope

1. Add dependencies:
   - `xcap` for screenshot capture.
   - `image` and `imageproc` for template matching and annotation.
   - `leptess` or `rusty-tesseract` for OCR where system dependencies are acceptable.
   - `reqwest` for optional vision LLM calls.
   - `ironrdp` for RDP.
   - `vnc-rs` for VNC.
2. Implement vision fallback:
   - screenshot capture.
   - OCR extraction.
   - template/region matching.
   - confidence scoring.
   - annotated evidence.
3. Implement remote viewport abstraction:
   - local calibrated viewport.
   - RDP viewport through `ironrdp`.
   - VNC viewport through `vnc-rs`.
4. Require output proof:
   - OCR/vision output must have confidence.
   - side-effect outputs must be verified by file/API/structured assertion where possible.
5. Update docs to keep vision/remote experimental until fixture E2Es pass.

## File Targets

- `crates/greentic-desktop-vision/src/lib.rs`
- `crates/greentic-desktop-workspaces/src/lib.rs`
- `crates/greentic-desktop-adapter/src/lib.rs`
- `docs/adapters/vision.md`
- `docs/aws-workspaces-mcp.md`
- `docs/capability-matrix.md`

## Out of Scope

- Treating a screenshot match as proof of durable business side effects.
- Proprietary OCR services as the only supported path.

## Acceptance Tests

1. Vision fixture locates a template/region and returns an annotated evidence artifact.
2. OCR fixture extracts text with confidence and fails below threshold.
3. Remote adapter fixture connects to a test VNC/RDP server or uses a protocol mock that exercises the real protocol client boundary.
4. Remote/vision failures are structured and do not fabricate outputs.
5. Evidence bundle stores screenshot/annotation metadata without leaking secrets.

## Done Means

Vision and remote desktop paths are honest, evidence-backed fallback/protocol adapters.
