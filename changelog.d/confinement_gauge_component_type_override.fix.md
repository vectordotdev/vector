Fixed the `vector_security_confinement_disabled` gauge reporting the wrong `component_type` when a sink delegates its build to another sink's `SinkConfig::build` internally (e.g. `humio_metrics` → `humio_logs`), or is wrapped by an external sink that constructs and builds a Vector sink directly. The gauge now inherits `component_type`/`component_kind` from the ambient tracing span like every other per-component metric, instead of hardcoding the delegate's own type.

authors: vladimir-dd
