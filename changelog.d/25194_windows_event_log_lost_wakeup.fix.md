The `windows_event_log` source no longer freezes after periods of inactivity. A race between the signal-handle reset and incoming event notifications could cause new events to be silently dropped from the wakeup path; the signal is now reset before draining so notifications arriving mid-drain are preserved.

authors: tot19
