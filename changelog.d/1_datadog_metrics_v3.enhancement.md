Adds the a new encoder to the Datadog metrics sink to encode metrics with v3 of
the payload protocol. An additional option `dual_write` will make Vector send
duplicate payloads to the given endpoint encoded with the configured protocol.
This allows the Datadog backend to validate that the metrics send via both
protocols specify the exact same metrics.
