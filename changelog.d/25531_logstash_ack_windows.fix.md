Fixed the `logstash` source to preserve writer window boundaries when generating ACKs. This prevents batched reads from producing ACK sequences that advance past the current window, which could lead to "invalid sequence number received" errors and duplicate retransmits under load.

authors: bruceg
