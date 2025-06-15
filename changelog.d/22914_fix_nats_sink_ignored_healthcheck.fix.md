The `nats` sink now does not return an error when an unresolvable or unavailable URL is provided. Note that if `--require-healthy` is set then Vector will stop on startup.

authors: rdwr-tomers
