experiment_name: "${experiment_name}"
variant: "${experiment_variant}"
capture_path: "/captures/${experiment_variant}.captures"
targets:
  - id: vector
    url: "http://vector.soak:9090/metrics"
  - id: lading_http_gen
    url: "http://http-gen.soak:9090"
  - id: lading_http_blackhole
    url: "http://http-blackhole.soak:9090"
  - id: lading_tcp_gen
    url: "http://tcp-gen.soak:9090"
  - id: lading_splunk_hec_blackhole
    url: "http://splunk-hec-blackhole.soak:9090"
  - id: lading_splunk_hec_gen
    url: "http://splunk-hec-gen.soak:9090"
