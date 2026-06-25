PR-32 — macOS/Linux CI and Test Desktop Harness

Goal: make this testable, not theoretical.

Add test harnesses for
Environment	Purpose
macOS GitHub Actions runner	build + permission-free unit tests
Ubuntu X11 virtual display	Linux X11 adapter tests
Ubuntu Wayland detection tests	capability detection / graceful degradation
Sample GTK app	Linux accessibility test
Sample Qt app	Linux accessibility test
Sample SwiftUI/AppKit app	macOS accessibility test
Java Swing sample app	Cross-platform Java adapter test
Acceptance criteria
Linux X11 adapter has automated integration tests.
Wayland tests verify graceful capability limitation.
macOS adapter has unit tests plus manual permission-gated integration tests.
Sample apps live in examples/desktop-targets.
CI matrix builds Windows/macOS/Linux.