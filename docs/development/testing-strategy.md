# Testing Strategy

## Required gates

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-features --locked
cargo build --release --locked
```

## Test layers

### Unit
Pure editor transforms, crop coordinate transforms, config migration, filename validation, cache accounting, generation/latest-wins logic, and thumbnail range calculations.

### Integration
Open folder → select → navigate → edit → save-as; overwrite through atomic writer; rescan preserving selection; large-folder index/filter behavior.

### Failure injection
Write failure before atomic replacement; corrupt config; unreadable files/directories; decode failure; destination collision; canceled/stale load result.

### Performance regression
50k-file scan/browse, rapid navigation, large-image peak RSS, gallery scroll, thumbnail cache bounds, and startup.

### Packaging smoke
Release archives must be installed into a temporary HOME/prefix and the installed binary/assets verified before publishing.

## Evidence rule

A work item is not complete only because the code compiles. The relevant test/build/performance evidence must be linked from the pull request or recorded under `docs/evidence/`.
