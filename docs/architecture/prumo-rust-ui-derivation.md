# Prumo UI derivation — Rust compatibility note

Status: **known upstream limitation in Prumo 0.6 current line**.

PhotoShow declares its canonical interface in `docs/ui-ux/interface-map.json` and
sets:

```json
"derivation": "off"
```

The current Prumo `internal/uimap.Compile` implementation still instantiates
`GoSymbolDeriver` unconditionally before merge, even when effective derivation is
`off`. Running `prumo ui verify` against a Rust source scope therefore attempts
to parse `.rs` as Go and fails before declared-map validation completes.

PhotoShow CI does **not** suppress interface validation. Instead it:

1. runs `prumo validate` and `prumo doctor`;
2. resolves `prumo ui config` and requires `derivation=off`;
3. uses the checked-out current Prumo package itself to load the interface map,
   load its state/token vocabulary, and run `uimap.Validate(..., nil)`;
4. fails on any declared-map finding with severity `error`.

This keeps the map governed by Prumo while refusing a false Go derivation step.

When Prumo provides a Rust `SymbolDeriver` or stops invoking Go derivation in
`off` mode, replace this compatibility check with normal
`prumo ui verify --path <repo>` and promote derivation back to
`verify-only`.
