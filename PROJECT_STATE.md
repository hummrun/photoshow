# Current Project State

- Project: **PhotoShow**
- Prumo baseline: **0.6.0**
- Current phase: **P01 — Reliability, Performance & Architecture Hardening**
- Active branch: **refactor/prumo-performance-hardening**
- UI toolkit: **egui/eframe — retained**
- Status: **hardening code gates green; performance evidence and release-tag smoke remain**
- Context methodology: **Lean Progressive Context**

## Current verified direction

- Preserve PhotoShow as a small, fast native photo viewer with light editing.
- Avoid a GUI toolkit migration during this hardening phase.
- Decouple media/domain code from egui incrementally.
- Reduce resident full-resolution image memory and bound asynchronous work.
- Make saving and release installation failure-safe.
- Establish deterministic CI and Prumo documentation contracts.
- Do not add product features until P0/P1 reliability gates are green.

## Current work packages

- PS-W0 — Prumo adoption, CI, authority and measurement baseline.
- PS-W1 — release integrity and atomic persistence.
- PS-W2 — media runtime/memory hardening.
- PS-W3 — UI/app decomposition and real virtualization.
- PS-W4 — UI contracts/accessibility.
- PS-W5 — release hardening.

## Verified baseline

Snapshot `c4648d43` passed, on the same commit:

- Linux: `cargo fmt --check`, strict Clippy, all tests, release build.
- Windows: strict Clippy, all tests, release build.
- MSRV Rust 1.95: `cargo check --all-targets --all-features --locked`.
- Prumo 0.6: project validation, doctor and declared UI-map validation.

Every later snapshot must pass the same gates before merge.

## Remaining P01 evidence

- Run and record the 1k/10k/50k folder-scan performance harness.
- Capture startup/idle RSS and real viewer/gallery frame-time on representative hardware.
- Exercise the release workflow from an actual prerelease tag, including installer + checksum download.

## Completion rule

This phase is complete only when Linux and Windows CI are green, release packaging has a smoke-tested install path, user writes preserve the original on failure, the large-folder UI is viewport-bounded, and the performance measurement protocol has reproducible evidence.
