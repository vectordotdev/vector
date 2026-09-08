Fix an issue where a reload signal (`SIGHUP` or a configuration file change) received while
Vector was draining events during graceful shutdown would immediately force-quit Vector
mid-drain, potentially dropping events that could have been flushed. Reload signals received
during the shutdown drain are now ignored so the drain completes; a second `SIGINT`, `SIGTERM`,
or `SIGQUIT` still forces an immediate exit.

authors: thomasqueirozb
