Fixed log message ordering on shutdown where `Vector has stopped.` was logged before
components had finished draining, causing confusing output interleaved with
`Waiting on running components` messages.
