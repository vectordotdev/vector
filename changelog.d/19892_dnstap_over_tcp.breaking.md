Added support for TCP mode for DNSTAP source. As the `dnstap` source now supports multiple socket types, you will need to update your configuration to specify which type - either `mode: unix` for the existing unix sockets mode or `mode: tcp` for the new tcp mode.

authors: esensar
