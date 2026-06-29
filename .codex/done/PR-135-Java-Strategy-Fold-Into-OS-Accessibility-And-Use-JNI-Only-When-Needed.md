# PR-135 - Java Strategy Fold Into OS Accessibility And Use JNI Only When Needed

## Goal

Stop treating Java as a generic fallback for non-Java desktop apps, and route Java apps through OS accessibility APIs unless direct JVM integration is explicitly required.

## User Outcome

A Word prompt never selects Java. Java applications work through the same platform accessibility spine as other desktop apps, with Java-specific bridges only where necessary.

## Current Evidence

- Java Access Bridge is Windows-specific and Java-specific.
- On macOS and Linux, Java UI should usually appear through AX or AT-SPI.
- Direct JVM calls are rarely needed for desktop automation and should not be the default.

## Scope

1. Define Java routing rules:
   - use Windows UIA/Java Access Bridge only when target app is Java.
   - use macOS AX for Java apps on macOS where AX exposes controls.
   - use AT-SPI for Java apps on Linux where available.
   - never select Java adapter for native apps solely because generic document primitives exist.
2. Evaluate direct JVM crates:
   - `jni`.
   - `j4rs`.
   - only adopt if a concrete fixture requires JVM-level introspection.
3. Add Java target detection:
   - process metadata.
   - accessibility role/class metadata.
   - explicit app profile.
4. Update planner/router to ask a question when Java/native target is ambiguous.
5. Add tests preventing native document prompts from choosing Java.

## File Targets

- `crates/greentic-desktop-java/src/lib.rs`
- `crates/greentic-desktop-planner/src/lib.rs`
- `crates/greentic-desktop-workflow/src/lib.rs`
- `docs/adapters/java-accessibility.md`
- `docs/capability-matrix.md`

## Out of Scope

- Hardcoded Java app scripts.
- Direct JVM control without a fixture proving it is needed.

## Acceptance Tests

1. Word/native document prompts route to native OS adapter, not Java.
2. Java fixture routes to Java/OS accessibility path only when app profile or metadata indicates Java.
3. Ambiguous prompts produce an open question instead of guessing Java.
4. Direct JNI/JVM integration remains absent unless justified by a failing fixture.
5. Docs explain Java is app-specific, not a generic desktop fallback.

## Done Means

Java support is correctly scoped and cannot hijack native desktop workflows.
