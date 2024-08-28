The `kafka` sink no longer emits warnings due to applying rdkafka options to a consumer used for the health check. Now it uses the producer client for the health check.

authors: belltoy
