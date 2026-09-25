Added a new `victoriametrics` sink that delivers metrics using the VictoriaMetrics remote write protocol (zstd-compressed remote write).

The sink encodes distributions as VictoriaMetrics `vmrange` histograms and accumulates incremental distributions in bounded memory. It supports single-node VictoriaMetrics, cluster tenants in the `vminsert` URL path or in labels, and per-event `vmauth` credentials through the `victoriametrics_token` event secret.

authors: missedone
