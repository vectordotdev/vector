Fixed log message ordering on shutdown where `Vector has stopped.` was logged before components had finished draining, causing confusing output interleaved with `Waiting on running components` messages.

A new `VectorStopping` event was added in the place of the `VectorStopped` event.

authors: tronboto
