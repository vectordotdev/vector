The `datadog_metrics` sink now converts a counter's `interval_ms` to seconds without integer
division. Counters with an interval under one second were sent as rates with infinite or `NaN`
values, and intervals that are not whole seconds overstated the rate. The rate value now uses
the exact interval, and the interval field is rounded to the nearest whole second (at least 1).
When the sink combines counters with an interval that share a series and second, it now also
combines their intervals, so the rate is not overstated.

authors: gremlinops
