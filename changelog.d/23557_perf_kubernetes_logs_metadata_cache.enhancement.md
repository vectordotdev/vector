## Performance

- **kubernetes_logs source**: Improved metadata enrichment performance by adding
  an internal cache keyed by `pod_uuid + container_name`.  
  This cache stores the fully constructed Kubernetes metadata structure, avoiding
  repeated VRL `BTreeMap` insertions for each log event.  
  Reduces CPU usage and increases throughput in high-ingestion scenarios.

authors: huanghuangzym
