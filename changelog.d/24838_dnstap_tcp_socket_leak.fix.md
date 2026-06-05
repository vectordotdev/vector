Fix TCP socket leak in `dnstap` source where sockets would accumulate in `CLOSE_WAIT` state
indefinitely. After sending a FrameStream `FINISH` frame in response to a client `STOP`, Vector
now explicitly closes the write side of the TCP connection as required by the FrameStream protocol,
preventing `CLOSE_WAIT` accumulation that previously exhausted the connection limit after extended
operation.

authors: jpds
