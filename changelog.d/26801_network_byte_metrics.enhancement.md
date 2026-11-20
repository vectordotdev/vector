Added `component_sent_network_bytes_total` to the shared HTTP client and socket sink utilities, and `component_received_network_bytes_total` to shared TCP and HTTP request-body readers and the UDP socket source. These counters measure network payload sizes independently of event delivery, including retries and received payloads rejected during processing. They exclude transport headers and TLS overhead. Existing serialized-byte counters retain their current behavior; deprecation is deferred until the remaining transports have replacement coverage.

authors: titaneric
