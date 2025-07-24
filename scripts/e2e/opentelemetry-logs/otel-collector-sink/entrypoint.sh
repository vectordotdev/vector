#!/bin/sh
rm -rf /tmp/*
touch /tmp/collector-file-exporter.log
exec /otelcol-contrib --config=/etc/otelcol-contrib/config.yaml
