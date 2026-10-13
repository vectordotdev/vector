The `windows_event_log` source now recovers when the Windows Event Log service is restarted while Vector runs. All
subscription handles then return `ERROR_INVALID_HANDLE` from `EvtNext`, which matched no recovery branch, so the source
stayed silent until Vector was restarted. Such a handle is now rebuilt from the bookmark, which survives the service
restart, so collection resumes without gap or duplicate.

authors: pos-ei-don
