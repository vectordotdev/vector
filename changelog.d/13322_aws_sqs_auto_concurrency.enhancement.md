The `aws_sqs` source now dynamically scales its polling tasks between 1 and `max_concurrency` (defaulting to the number of available CPUs) based on queue activity, reducing unnecessary `ReceiveMessage` API requests when the queue is idle. The `client_concurrency` option has been deprecated in favor of `max_concurrency`.

authors: jimmystewpot
