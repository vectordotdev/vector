HTTP-based sinks no longer hold an adaptive concurrency permit while a request sleeps between retry attempts. The retry layer now wraps the concurrency limiter, so a permit is acquired and released around each individual attempt. This prevents a few requests stuck in long retry backoffs from starving healthy traffic after a transient downstream error, and gives the adaptive concurrency controller per-attempt latency and outcome signals.

authors: valerypetrov
