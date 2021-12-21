# Datadog Agent -> Remap -> Datadog Logs

This soak tests Datadog agent source feeding into the Datadog logs source
through a simplistic remap transform. It is a straight pipe.

This is the same soak test scenario as `datadog_agent_remap_datadog_logs`
but with end-to-end acknowledgements enabled. When end-to-end
acknowledgements become the default, these tests can be merged.

## Method

Lading `http_gen` is used to generate log load into vector, `http_blackhole`
acts as a Datadog API sink.
