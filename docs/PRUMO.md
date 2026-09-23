# Prumo — PhotoShow

> Canonical documentation router for humans and agents.

## Product

- [README](../README.md) — product overview and installation.
- [Usage overview](uso/visao-geral.md) — current user-facing workflows.
- [Roadmap](../ROADMAP.md) — product feature direction.

## Architecture

- [Architecture Contract](architecture/photoshow-architecture.md) — module boundaries, dependency direction, hard invariants.
- [Current Project State](../PROJECT_STATE.md) — active phase, current decisions and completion rule.

## Quality and verification

- [Testing Strategy](development/testing-strategy.md)
- [Performance Baseline Protocol](quality/performance-baseline.md)

## UI/UX

- [UI contract overview](ui-ux/README.md)
- `ui-ux/interface-map.json` — canonical component/relationship inventory.
- `ui-ux/state-matrix.json` — loading/error/empty/selected/editing/saving behavior.

## Prumo authority

- `../prumo.json` — canonical Prumo project configuration.
- `../project-profile.json` — profile used for resolution/bootstrap.
- `../ENTRYPOINT.md` — minimum-context recovery path.

## Projection boundary

The existing `.ai/skills/` tree predates the current Prumo 0.6 adoption. Treat it as imported/generated projection material, not as higher authority than the files above. It should be regenerated/reconciled through Prumo rather than hand-edited piecemeal.

## Change discipline

For every non-trivial change:

```text
canonical contract / decision
→ inspect current implementation
→ define the smallest delta
→ add or adapt verification
→ implement
→ run gates
→ capture evidence
→ update impacted canonical documentation
```
