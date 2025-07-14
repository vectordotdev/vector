Add a new `incremental_to_absolute` transform which converts incremental metrics to absolute metrics. This is useful for
use cases when sending metrics to a sink is lossy or you want to get a historical record of metrics, in which case
incremental metrics may be inaccurate since any gaps in metrics sent will result in an inaccurate reading of the ending
value.

authors: GreyLilac09