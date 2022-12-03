# Fluent -> Remap -> AWS Firehose

This soak tests fluent source feeding into a very simple VRL transform through
to AWS Firehose sink. It is a straight pipe.

## Method

Lading `tcp_gen` is used to generate log load into vector, `http_blackhole`
acts as an AWS Firehose.
