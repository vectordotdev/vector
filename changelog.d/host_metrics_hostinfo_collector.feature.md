Add new `HostInfo` collector to the `host_metrics` source that emits static host system information as a Prometheus-style info metric. The metric includes 18 tags covering OS details (name, version, kernel), CPU (model, vendor), network (IP, MAC), virtualization (VM UUID, type, container detection), timezone, locale, domain, and Vector version. This enables host fingerprinting and fleet inventory tracking.

authors: zapdos26
