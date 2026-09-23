# Performance Evidence — Synthetic Folder Scan

Date: **2026-09-23**  
Commit: `ab1de4a20ffe0b0b2c6d44d62891faddc48004d8`  
Workflow run: **35826785654**  
Artifact: `photoshow-performance-evidence-ab1de4a20ffe0b0b2c6d44d62891faddc48004d8`

## Environment

- GitHub-hosted Ubuntu **24.04.5 LTS** runner
- Runner image: `ubuntu-24.04`, image version `20260907.300.1`
- CPU exposed to job: **4 logical CPUs**
- CPU model: **AMD EPYC 7763 64-Core Processor**
- RAM exposed to job: **15 GiB**
- Rust stable during run: **1.98.1**
- Release profile: optimized, Thin LTO, one codegen unit

This is a **CI baseline**, not a low-end desktop baseline. Do not use these numbers
as minimum hardware requirements or absolute end-user latency.

## Synthetic scan results

The benchmark creates supported `.jpg` entries in a temporary directory, then
times only PhotoShow's folder scan/sort path.

| Entries | Photos found | Recoverable errors | Scan time |
|---:|---:|---:|---:|
| 1,000 | 1,000 | 0 | **2 ms** |
| 10,000 | 10,000 | 0 | **13 ms** |
| 50,000 | 50,000 | 0 | **68 ms** |

The complete ignored test took **2.15 s**, mostly because it creates 50,000
filesystem fixtures between timed scans.

## Process-level resource evidence

`/usr/bin/time -v` wrapped the complete `cargo test` benchmark command:

- wall clock: **2.41 s**
- user CPU: **0.27 s**
- system CPU: **2.11 s**
- CPU utilization: **99%**
- maximum resident set size: **95,048 KiB**
- major page faults: **0**
- swaps: **0**

The RSS value belongs to the **benchmark command/process tree**, not specifically
to the PhotoShow scan cache. It must not be presented as PhotoShow idle RSS.

## Release binary size

`target/release/photoshow` from the same evidence run:

- **22,450,136 bytes**
- approximately **21.41 MiB**

This is the stripped Linux release binary from the configured release profile.

## Interpretation

The current scan implementation is comfortably sub-100 ms for the synthetic
50k-entry corpus on this hosted runner. This supports the architectural direction
of cached sort keys, shared `PhotoPath` data and a latest-wins scan worker.

It does **not** establish a performance budget yet. Real folders may involve slow
storage, nested directory traversal, permissions, `.gitignore` processing,
network filesystems and mixed non-image files.

## Remaining measurements

Before promoting numeric regression budgets, collect:

1. the same scan test on representative low-end hardware;
2. cold/warm startup and idle RSS;
3. 12 MP / 24 MP JPEG viewer selection latency;
4. a large PNG and 200 MB+ TIFF peak-memory case;
5. 50k-gallery frame-time distribution during continuous scroll;
6. rapid 100-selection navigation with stale-work counters;
7. WGPU × Glow comparison on at least one modest integrated GPU system.

## Reproduce

```bash
cargo test --release --all-features --locked \
  fs_browser::tests::benchmark_scan_synthetic_collections \
  -- --ignored --exact --nocapture
```

For combined process RSS on Linux:

```bash
/usr/bin/time -v cargo test --release --all-features --locked \
  fs_browser::tests::benchmark_scan_synthetic_collections \
  -- --ignored --exact --nocapture
```
