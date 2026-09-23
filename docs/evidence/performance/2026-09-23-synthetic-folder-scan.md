# Performance Evidence — Synthetic Folder Scan

- Date: 2026-09-23
- Commit: `ab1de4a20ffe0b0b2c6d44d62891faddc48004d8`
- Workflow run: `35826785654`
- Workflow: `performance-evidence`
- Result: **success**
- Rust: stable 1.98.1
- OS: Ubuntu 24.04.5 LTS, GitHub-hosted runner
- CPU: AMD EPYC 7763, 4 logical CPUs exposed to runner
- RAM available to runner: 15 GiB

## Folder-scan measurements

The fixture creation happens before each timed scan. The values below are emitted
inside the benchmark immediately around `ScanController::scan` completion:

| Files | Photos found | Errors | Scan time |
| ---: | ---: | ---: | ---: |
| 1,000 | 1,000 | 0 | **2 ms** |
| 10,000 | 10,000 | 0 | **13 ms** |
| 50,000 | 50,000 | 0 | **68 ms** |

These are **synthetic local-filesystem CI measurements**, not promises for real
user disks, network mounts, antivirus/indexer contention, or deeply nested trees.
They are useful as a repeatable regression baseline.

## Harness process measurements

`/usr/bin/time -v` around the release benchmark command reported:

- Wall clock: **2.41 s**
- User CPU: **0.27 s**
- System CPU: **2.11 s**
- CPU utilization: **99%**
- Maximum resident set size: **95,048 KiB (~92.8 MiB)**
- Major page faults: **0**
- Swaps: **0**
- Exit status: **0**

Important: this RSS is for the **whole Cargo test process/test binary**, including
test harness overhead. It must **not** be reported as PhotoShow GUI idle RAM or as
pure scan-cache memory.

## Release binary

The Linux release binary produced by the same run measured:

- **22,450,136 bytes**
- approximately **21.4 MiB**

This is the raw executable size, not a compressed release archive.

## Interpretation

The 50k synthetic scan target is already comfortably sub-100 ms on this CI
machine, so future optimization work should not chase smaller scan numbers at the
expense of code clarity. More valuable remaining measurements are:

1. cold/warm GUI startup;
2. real idle RSS/VRAM;
3. JPEG/PNG/TIFF selection-to-visible latency;
4. rapid 100-image navigation with stale-work accounting;
5. filmstrip frame-time distribution while scrolling 50k entries;
6. peak RSS during full-resolution save/copy;
7. WGPU vs Glow on representative low-end hardware.

No hard regression threshold is promoted from this single machine yet. A budget
should be set only after at least one representative low-end/local baseline.
