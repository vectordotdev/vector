The `logstash` source no longer runs out of memory and crashes on rare problematic compressed frames, such as one containing an extremely large number of events. Additionally, events received before a malformed frame inside a compressed payload are now delivered instead of being silently discarded along with the malformed frame.

authors: pront
