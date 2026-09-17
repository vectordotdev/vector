Fix an issue where a reload signal (`SIGHUP` or a configuration file change) received while
Vector was draining events during graceful shutdown would immediately force-quit Vector
mid-drain, potentially dropping events that could have been flushed. Shutdown requests now
remain available throughout startup, validation, reload, and drain. Reload requests are coalesced
after a short quiet period, preserving component and enrichment-table updates. Reloads received
during drain do not force an exit; a second `SIGINT`, `SIGTERM`, or `SIGQUIT` still does.

authors: thomasqueirozb
