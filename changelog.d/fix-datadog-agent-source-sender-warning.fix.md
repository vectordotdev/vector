Eliminated the "Source send cancelled." error and corresponding metric for the
`datadog_agent` source, as Datadog Agent will always resend events when the
connection is dropped without an HTTP response code.

author: bruceg
