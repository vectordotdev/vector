#!/bin/sh
rm -rf /tmp/collector-sink.log
touch /tmp/collector-sink.log
exec /otelcol-contrib --config=/etc/otelcol-contrib/config.yaml
