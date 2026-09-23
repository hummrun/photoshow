# PhotoShow Architecture Contract

> Authority: canonical project architecture for the PhotoShow desktop application.

## Product boundary

PhotoShow is a fast native photo viewer with practical organization and light, non-destructive editing. It is not a catalog database, full DAM, GIMP replacement, Lightroom replacement, cloud service, or plugin platform.

## Dependency direction

```text
editor ------------------+
filesystem --------------|
media -------------------|--> app --> ui --> egui/eframe
config ------------------+
```

The UI is an adapter. Product rules and media processing must not depend on egui types.

## Hard rules

1. Heavy decode, encode, scan, save, export, and metadata work must not block the UI thread.
2. Stale asynchronous results must never replace the currently selected image.
3. Memory is budgeted from decoded bytes, not compressed file size.
4. Full-resolution pixels are loaded only for operations that require them.
5. Save/overwrite must use a same-directory temporary file and atomic replacement when the platform permits it.
6. Existing metadata must not be silently discarded when overwriting a supported source format.
7. A 50k-file folder must not create O(n) widgets every frame.
8. UI changes must preserve keyboard operation and explicit loading/error/saving states.
9. No new runtime framework, async runtime, DI container, ECS, event bus, or database is introduced without measured need.
10. Completion claims require test/build/performance evidence appropriate to the change.

## Module target

```text
src/
├── app/
│   ├── state.rs
│   ├── commands.rs
│   └── shortcuts.rs
├── media/
│   ├── loader.rs
│   ├── thumbnails.rs
│   ├── pixels.rs
│   └── save.rs
├── ui/
│   ├── browser.rs
│   ├── viewer.rs
│   ├── filmstrip.rs
│   ├── toolbar.rs
│   ├── dialogs.rs
│   ├── theme.rs
│   └── egui_image.rs
├── editor.rs
├── exif.rs
├── fs_browser.rs
└── config.rs
```

This is a migration target, not permission for a big-bang rewrite. Extract one responsibility at a time while preserving behavior.

## Prumo workflow

For a non-trivial work package:

```text
contract/decision
→ inspect repository reality
→ declare delta
→ add/adjust test
→ implement
→ run quality gates
→ collect performance evidence when relevant
→ update affected documentation
```

Generated agent surfaces and `.ai/skills` are projections. They do not override this document, executable tests, or repository reality.
