prometheus: "http://prometheus:9090"
experiment_name: "${experiment_name}"
variant: "${experiment_variant}"
vector_id: "${vector_id}"
queries:
  - query: sum(rate((bytes_written[30s])))
    id: throughput
    unit: bytes
capture_path: "/captures/${experiment_variant}.captures"
