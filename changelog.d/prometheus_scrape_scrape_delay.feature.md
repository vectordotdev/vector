The `prometheus_scrape` source has a new `scrape_delay` option that controls when scrapes happen relative to the configured `scrape_interval_secs`. Sources sharing an interval otherwise all scrape at the same instant, so an instance running many of them raises load in a single spike each interval.

`none`, the default, keeps the existing behavior unchanged. A delay in seconds, such as `30s`, holds the first scrape back by that much and then keeps the same fixed cadence, so sources can be staggered by hand. `auto` instead picks a position inside each interval, derived from a hash of the host name, the component ID and the scrape number, which reduces persistent alignment with other periodic work. See the option docs for the trade-offs `auto` makes.

authors: taloric
