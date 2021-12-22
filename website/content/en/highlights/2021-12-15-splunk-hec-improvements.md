---
date: "2021-12-15"
title: "Splunk HEC Improvements"
description: "Improved compatibility with Splunk HTTP Event Collector"
authors: ["barieom"]
pr_numbers: [10261, 10135]
release: "0.19.0"
hide_on_release_notes: false
badges:
  type: enhancement
---

We're excited to share that we've made a couple improvements to Vector's support
for [Splunk HEC][Splunk HEC]; Vector now supports Splunk HEC indexer acknowledgements and
token passthrough routing.

## Indexer Acknowledgements

With the new support for Splunk HEC indexer acknowledgement, Vector now provides a
<<<<<<< HEAD
better end-to-end experience to ensure that no data is lost. As you may be
aware, Splunk HEC does not guarantee that data is successfully written and saved
if it responds to the incoming request successfully. To address this, Vector now
interacts with the [Splunk HEC indexer acknowledgements][indexer] protocol to
acknowledge and verify that data has been successfully delivered. This allows Vector
to notify that data has been successfully processed. In addition, `splunk_hec` source
is now integrated into Vector's end-to-end acknowledgements so that it only acknowledges
events that have been processed by sinks or disk buffers. To learn more about
how it works, check out the short explanation in the Vector [`splunk_hec` sinks][indexer how it works]
reference page.


## Passthrough Token Routing

Vector now also supports token passthrough routing. When `store_hec_token` is enabled
in a `splunk_hec` source, tokens included in requests to the source will be stored and
used by downstream `splunk_hec` sinks. Any passed through token takes precedence over
the `default_token` configuration set in the sink. With this update, you can now
partition event batches in the `splunk_hec` sinks by token.


We hope that these improvements can make your experience using Vector with Splunk
better! If you any feedback for us, let us know on our [Discord chat] or on [Twitter].

[Splunk HEC]: https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/UsetheHTTPEventCollector
[indexer]: https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/AboutHECIDXAck
[indexer how it works]: https://master.vector.dev/docs/reference/configuration/sinks/splunk_hec_metrics/#indexer-acknowledgements
[Discord chat]: https://discord.com/invite/dX3bdkF
[Twitter]: https://twitter.com/vectordotdev
