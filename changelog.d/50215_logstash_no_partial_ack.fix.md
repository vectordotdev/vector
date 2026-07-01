Fixed the `logstash` source to ACK only completed writer windows rather than
sometimes emitting a partial ACK before the window is complete. While partial
ACKs are permitted by the official protocol spec and cause no problems for the
reference `go-lumber` client in Beats, they appears to confuse proxies that
assume there will only be one ACK per window, causing errors on subsequent
batches.

authors: bruceg
