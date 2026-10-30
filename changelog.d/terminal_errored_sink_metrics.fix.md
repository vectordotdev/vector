Fixed missing error and discarded-event metrics for terminal errored sink requests.
Successful Redis sorted-set writes that update existing members no longer trigger retries or discarded-event metrics.
Redis Pub/Sub discarded-event metrics exclude events confirmed delivered on earlier retry attempts.

Elasticsearch partial retries retain permanent failures in discarded-event metrics and exclude confirmed deliveries, including on terminal transport errors.

authors: Shuricky
