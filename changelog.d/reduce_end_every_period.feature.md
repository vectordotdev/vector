A new configuration option `end_every_period_ms` is available on reduce transforms
If supplied, every time this interval elapses for a given grouping, the reduced value
for that grouping is flushed. Checked every `flush_period_ms`.

authors: charlesconnell
