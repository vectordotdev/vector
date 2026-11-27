Iterating over all fields of a log event, as the `reduce` transform does for every event, no longer runs a regex on each field name to decide whether it needs quotes. This lowers CPU usage and removes regex cache-pool contention when several threads iterate fields at once.

authors: gwenaskell
