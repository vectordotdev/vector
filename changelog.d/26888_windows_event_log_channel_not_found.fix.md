Fixed the `windows_event_log` source remaining silent after a subscribed channel is temporarily
unregistered, such as during Microsoft Defender platform updates: `EvtNext` then returns
`ERROR_EVT_CHANNEL_NOT_FOUND` on every pull, and the source only recovered when Vector was
restarted. The source now retries the subscription (every 5 seconds while the channel is missing)
and resumes from its bookmark; a missing channel no longer causes a fallback to a subscription
without the bookmark.

authors: pos-ei-don
