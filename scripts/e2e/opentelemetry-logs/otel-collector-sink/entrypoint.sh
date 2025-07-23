#!/bin/sh
: > /tmp/collector-sink.log
exec /otelcol-contrib --config=/etc/otelcol-contrib/config.yaml
