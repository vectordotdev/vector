Fixed a `disk_v2` buffer stall where a reader could wait indefinitely for another write even though the buffer already contained a published record.

authors: fernandol-nvidia
