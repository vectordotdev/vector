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
for Splunk HEC; Vector now supports Splunk HEC indexer acknowledgements and
token passthrough routing.

## Indexer Acknowledgements

With the new support for Splunk HEC indexer acknowledgement, Vector now provides a
better end-to-end acknowledgement to ensure that no data is lost. As you may be
aware, Splunk HEC does not guarantee that a data is successfully written and saved
if it receives a 200. Vector will now check [Splunk HEC indexer acknowledgements][Splunk indexer]
protocol to verify that data has been successfully delivered. To learn more about
how it works, check out the short explanation in the Vector [sinks reference page][indexer how it works].

## Passthrough Token Routing

Vector now also supports new token passthrough routing. You can now use these tokens,
which can be found in `splunk_hec` source requests, downstream `splunk_hec` sink
requests. With the ability to record Splunk token that the events came in with, you
can now use that token in the `splunk_hec` sink to forward logs. With this update,
you can now partition event batches in the `splunk_hec` sinks by token.


We hope that these improvements can make your experience using Vector with Splunk
better! If you any feedback for us, let us know on our [Discord chat] or on [Twitter].

[Splunk indexer]: https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/AboutHECIDXAck
[indexer how it works]: https://master.vector.dev/docs/reference/configuration/sinks/splunk_hec_metrics/#indexer-acknowledgements
[Discord chat]: https://discord.com/invite/dX3bdkF
[Twitter]: https://twitter.com/vectordotdev
