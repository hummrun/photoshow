# Performance Baseline and Budgets

> Authority: canonical measurement protocol. Numeric regression budgets must be promoted only after a reproducible baseline exists.

## Principle

Do not claim that a renderer, cache strategy, or refactor is faster because it appears simpler. Measure it on the same corpus and hardware class.

## Required scenarios

### Startup
- cold start to first interactive frame
- warm start to first interactive frame
- RSS after 10 seconds idle

### Folder scan
- 1k, 10k, and 50k filesystem entries
- time to first usable result
- total scan time
- peak RSS
- number of recoverable scan errors

### Viewer
- JPEG around 12 MP and 24 MP
- large PNG
- TIFF at least 200 MB when test infrastructure permits
- selection-to-visible latency
- rapid next/previous navigation for 100 selections
- peak number of decode workers
- stale-result count

### Gallery
- 50k photo entries
- continuous scroll for at least 30 seconds
- frame time distribution
- number of thumbnail cells built per frame
- thumbnail cache bytes/items

### Save
- save-as
- overwrite
- failure injection before replacement
- output readability
- metadata preservation
- peak RSS

### Packaging
- release binary size
- archive/package size
- one-line installer smoke test

## Evidence format

Store benchmark evidence under `docs/evidence/performance/` as dated Markdown/JSON. Every report must identify:
- commit SHA
- OS
- CPU
- RAM
- GPU/renderer when relevant
- image corpus
- command/build profile
- raw measurements
- interpretation

## Initial regression rule

Until a stable baseline is recorded, CI treats correctness regressions as hard failures and performance changes as evidence-required review items. Do not invent thresholds.

## Candidate budgets after baseline

Budgets should eventually cover:
- maximum idle RSS
- maximum peak RSS for standard viewer scenario
- maximum concurrent full decodes
- selection latency percentile
- gallery frame-time percentile
- startup latency
- binary/package size

WGPU vs Glow remains an evidence question. Both may be benchmarked; neither is declared superior by architecture policy.
