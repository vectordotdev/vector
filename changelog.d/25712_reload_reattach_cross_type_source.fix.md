Fixed a config reload bug that could silently stop event delivery. If a reload changes a component's kind while keeping the same name (for example, replacing an enrichment table's derived source named `X` with a regular source named `X`, or replacing a transform named `X` with a source named `X`), any downstream sink or transform that still reads from `X` now correctly reconnects to the new component instead of going silent until the next restart.

authors: pront
