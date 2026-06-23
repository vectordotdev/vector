Fixed the `logstash` source to close the connection on a malformed frame instead of attempting to continue. A failed JSON decode or decompression previously left the decoder desynchronized but still running, which could busy-loop and emit ACKs for bogus sequence numbers (surfacing as `invalid sequence number received` on the client). The source now treats any decode error as fatal and closes the connection — matching the upstream `logstash-input-beats` server — so the client reconnects and retransmits the unacknowledged window.

authors: graphcareful
