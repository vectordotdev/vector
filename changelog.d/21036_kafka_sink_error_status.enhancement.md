The `kafka` sink now retries sending events that failed to be sent for transient reasons. Previously
it would reject these events.

authors: frankh
