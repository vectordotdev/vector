Fixed the `logstash` source to preserve ACK domains when a fresh Lumberjack `WindowSize` arrives before a previous partial writer window has been acknowledged. This prevents ACKs for later windows from advancing past the current Filebeat window and triggering `invalid sequence number received` retransmits.

authors: emilychendd
