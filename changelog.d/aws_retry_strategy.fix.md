Fixed the default `aws_s3` sink retry strategy.
The default configuration now correctly retries common transient errors instead of requiring manual configuration.

authors: pront
