Fix the `windows_event_log` source going permanently silent shortly after startup. Four of the six
Win32 error constants held wrong values, so `EvtNext` returning `ERROR_INVALID_OPERATION` on a healthy
channel was mistaken for a stale query result and swallowed at debug level, and the only
re-subscription branch was gated on an error code that cannot occur. The values are corrected against
winerror.h, and a subscription handle that can no longer serve results is now rebuilt instead of being
abandoned. Direct (analytic/debug) channels continue to be skipped rather than aborting startup, and a
failed re-subscription now leaves the channel in a state that is retried on the next cycle instead of
going permanently silent, and the channel health summary reports that channel as inactive instead of
healthy while it is being retried.

On a disabled or empty channel `ERROR_INVALID_OPERATION` only means there is nothing to read; the
subscription is now kept there instead of being rebuilt every `event_timeout_ms` (checked via the
channel's enabled flag and record count). Rebuilds of a channel that delivers nothing back off (5 s,
doubling, up to 10 minutes) and are logged at `warn` only on the first and every power-of-two attempt; a
signalled channel is still handled immediately.

The source also recovers when the Windows Event Log service is restarted while Vector runs. All
subscription handles then return `ERROR_INVALID_HANDLE` from `EvtNext`, which matched no recovery
branch, so the source stayed silent until Vector itself was restarted. Such a handle is now rebuilt from
the bookmark, which survives the service restart, so collection resumes without gap or duplicate. If the
service is still down when the rebuild is attempted, it is retried every 5 seconds rather than on the
idle-channel backoff, so recovery is not delayed once the service is back.

authors: pos-ei-don
