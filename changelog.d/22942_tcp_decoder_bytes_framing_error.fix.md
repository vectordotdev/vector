Fixed recurrent "Failed framing bytes" produced by TCP sources such as fluent and logstash by ignoring connection
resets that occur after complete frames. Connection resets with partial frame data are still reported as errors.

authors: gwenaskell
