The `datadog_metrics` sink now subdivides an oversized metrics payload until each piece fits,
instead of splitting it once and dropping any piece that is still too large.

Previously the sink asked the encoder how many chunks to split an oversized payload into, which
is derived from a byte-size ratio, but then partitioned the metrics by count. A batch of unevenly
sized metrics could therefore leave a chunk still over the size limit, and that whole chunk was
dropped, so a single oversized metric could take healthy metrics down with it. The sink now halves
an oversized batch and retries each half, matching the Datadog Agent's behavior, and only drops a
metric that is too large to be sent on its own.

authors: stephenwakely
