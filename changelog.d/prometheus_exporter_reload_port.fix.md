The `prometheus_exporter` sink now closes its listening socket before it finishes. A configuration reload that reuses the same `address` no longer fails intermittently because the old sink still holds that address.

authors: thomasqueirozb
