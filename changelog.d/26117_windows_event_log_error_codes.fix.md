Fix the `windows_event_log` source going permanently silent shortly after startup. Four of the six
Win32 error constants held wrong values, so `EvtNext` returning `ERROR_INVALID_OPERATION` on a healthy
channel was mistaken for a stale query result and swallowed at debug level, and the only
re-subscription branch was gated on an error code that cannot occur. The values are corrected against
winerror.h, and a subscription handle that can no longer serve results is now rebuilt instead of being
abandoned. Direct (analytic/debug) channels continue to be skipped rather than aborting startup, and a
failed re-subscription now leaves the channel in a state that is retried on the next cycle instead of
going permanently silent, and the channel health summary reports that channel as inactive instead of
healthy while it is being retried.

authors: pos-ei-don
