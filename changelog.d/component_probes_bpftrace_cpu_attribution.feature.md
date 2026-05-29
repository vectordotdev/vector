Added a `component-probes` Cargo feature (disabled by default) that enables bpftrace-based per-component CPU attribution. When enabled, per-thread atomic labels and uprobe registration functions allow external bpftrace scripts to attribute CPU samples to individual Vector components (sources, transforms, sinks).

authors: connoryy
