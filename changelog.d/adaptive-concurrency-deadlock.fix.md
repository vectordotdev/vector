Setting `adaptive_concurrency.decrease_ratio` in a sink to a value less than 0.5
could lead to a deadlock where no transmission slots are available. This has
been adjusted to ensure at least one slot is always available.
