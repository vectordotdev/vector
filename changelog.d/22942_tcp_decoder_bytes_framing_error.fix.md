Fixed recurrent "Failed framing bytes" produced by TCP sources such as fluent and logstash by making the TCP read loop
lenient on connection resets.

authors: gwenaskell
