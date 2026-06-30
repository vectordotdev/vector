Fixed the `logstash` source to delimit ACK windows by `WindowSize` frames rather than by counting events down to the advertised window size. The previous approach could lose a window boundary when a sender flushed a window with fewer events than advertised (legal, since `WindowSize` is a maximum) and a later window was coalesced into the same read, causing Vector to ACK a sequence number beyond the window the sender was waiting on. Beats/Filebeat reported this as `invalid sequence number received (seq=N, expected=M)`, leading to reconnects and duplicate retransmits.

authors: graphcareful
