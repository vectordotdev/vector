Fixed issue during in-situ reload of a sink with disk buffer configured where
component would stall for batch.timeout_sec before fully gracefully reloading.

authors: graphcareful
