# UI/UX contracts

This directory is canonical for the semantic structure and state vocabulary of the PhotoShow desktop UI.

- `interface-map.json` describes composition, purpose, implementation anchors and non-hierarchical relationships.
- `state-matrix.json` defines user-visible states and recovery behavior.

These files describe current implementation reality. Planned UI must use `implementation.status = not-implemented` rather than being presented as already available.

Pixel styling remains in the egui implementation. The contracts exist so refactors and code agents preserve information architecture, accessibility intent and state semantics.
