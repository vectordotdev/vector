# Datadog Agent -> Remap -> Datadog Logs

This soak tests Datadog agent source feeding into the Datadog logs source
through a simplistic remap transform. It is a straight pipe.

## Method

Lading `http_gen` is used to generate log load into vector, `http_blackhole`
acts as a Datadog API sink.
