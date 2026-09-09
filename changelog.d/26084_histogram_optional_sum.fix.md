The sketch conversion used by the `datadog_metrics` sink no longer reports a sum and average of zero for a histogram that reports no sum, keeping its bucket-derived estimate instead. Previously such a histogram arrived with a fabricated sum of zero, which the conversion then trusted as exact.

authors: gwenaskell
