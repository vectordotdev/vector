Vector now reports the underlying error when a source or sink task fails at runtime; previously, it only said `the task completed with an error`. That includes failures to bind a listening socket (for example `Address already in use (os error 48)`), broker subscribe and consume errors, checkpoint file errors, sink runtime errors, and sources that stop because their downstream channel closed. Vector now includes the panic payload in its logs.

authors: gwenaskell
