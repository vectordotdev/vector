prometheus: "http://prometheus:9090"
experiment_name: "${experiment_name}"
variant: "${experiment_variant}"
vector_id: "${vector_id}"
queries:
  - query: sum(rate((bytes_written[2s])))
    id: throughput
    unit: bytes
  - query: sum(bytes_written)
    id: cumulative_bytes_written
    unit: bytes
capture_path: "/captures/${experiment_variant}.captures"
