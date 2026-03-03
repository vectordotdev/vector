Added a `component-probes` Cargo feature (disabled by default) that enables bpftrace-based per-component CPU attribution. When enabled, a shared-memory array and uprobe symbol allow external bpftrace scripts to attribute CPU samples to individual Vector components (sources, transforms, sinks).

authors: connoryy
