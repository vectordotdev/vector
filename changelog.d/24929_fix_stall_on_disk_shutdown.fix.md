Fixed issue during in place reload of a sink with a disk buffer configured, where
the component would stall for batch.timeout_sec before gracefully reloading.
This fix also remediates issues Vector had where it would ignore SIGINT during
cases where the pipeline stall was occuring.

authors: graphcareful
