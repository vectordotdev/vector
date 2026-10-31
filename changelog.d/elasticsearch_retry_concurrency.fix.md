The `elasticsearch` sink now applies fixed `request.concurrency` limits across all endpoints, including requests waiting to retry. This restores backpressure during downstream failures. Adaptive concurrency is unchanged.

authors: Volevanius
