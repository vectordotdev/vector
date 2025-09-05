The `kubernetes_logs` source metadata enrichment performance was improved by adding an internal cache keyed by `pod_uuid + container_name`.  
The cache stores the fully constructed Kubernetes metadata structure, avoiding repeated VRL `BTreeMap` insertions for each log event.  
This cache reduces CPU usage and increases throughput in high-ingestion scenarios.

authors: huanghuangzym
