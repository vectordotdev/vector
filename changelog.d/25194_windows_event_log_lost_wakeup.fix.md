The `windows_event_log` source no longer freezes after periods of inactivity. Two complementary fixes address the root cause and add a recovery path:

1. **Pre-drain signal reset**: the subscription's wait handle is now reset *before* draining events via `EvtNext`, not after. A signal that fires between the last `EvtNext` and the old post-drain `ResetEvent` was silently lost, leaving the source frozen until the next OS event arrived. Resetting first preserves any notification raised mid-drain.

2. **Speculative pull on timeout**: on each wait timeout, `pull_events` is called speculatively. `EvtNext` returns `ERROR_NO_MORE_ITEMS` immediately on an empty channel (near-zero cost), so this is safe every cycle. If events are recovered, a warning is logged. This self-heals within one timeout period regardless of why the wakeup signal was lost — covering both the drain-race path and any other lost-wakeup scenario.

authors: tot19
