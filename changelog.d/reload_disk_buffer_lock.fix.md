Fixed configuration reloads that change a sink's disk buffer settings failing with `failed to lock buffer.lock; is another Vector process running and using this buffer?`. This happened every time when the sink's input was an idle transform, and intermittently under load. When the rollback hit the same error, Vector shut down.

authors: thomasqueirozb
