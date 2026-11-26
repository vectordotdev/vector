A disk buffer used as the overflow stage of a memory buffer is now drained while the memory stage keeps receiving events. Previously, records larger than the disk reader's internal read buffer could be lost or stall the reader when the memory stage won the race for the next event, so spilled events were only delivered after a restart, or not at all.

authors: wafgo
