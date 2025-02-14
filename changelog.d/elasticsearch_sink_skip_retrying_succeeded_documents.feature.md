request_retry_partial option of elasticsearch sink's behaviour changed. Now only retriable failed requests in a bulk will be retried (instead of all requests in the bulk).
authors: Serendo