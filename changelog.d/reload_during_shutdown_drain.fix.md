Fix an issue where a reload signal (`SIGHUP` or a configuration file change) received while
Vector was draining events during graceful shutdown would immediately force-quit Vector
mid-drain, potentially dropping events that could have been flushed. Shutdown and reload
signals now travel on separate channels, so reload signals received during the shutdown drain
are ignored and a burst of reloads can no longer crowd a shutdown out of the control channel;
a second `SIGINT`, `SIGTERM`, or `SIGQUIT` still forces an immediate exit.

authors: thomasqueirozb
