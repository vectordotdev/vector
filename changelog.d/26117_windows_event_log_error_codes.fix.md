Fix the `windows_event_log` source going permanently silent shortly after startup. Four of the six
Win32 error constants held wrong values, so `EvtNext` returning `ERROR_INVALID_OPERATION` on a healthy
channel was mistaken for a stale query result and swallowed at debug level, and the only
re-subscription branch was gated on an error code that cannot occur. The values are corrected against
winerror.h, and a subscription handle that can no longer serve results is now rebuilt instead of being
abandoned.

authors: pos-ei-don
