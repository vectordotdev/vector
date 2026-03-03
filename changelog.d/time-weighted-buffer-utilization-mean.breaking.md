The `*buffer_utilization_mean` metrics have been enhanced to use time-weighted
averaging which make them more representative of the actual buffer utilization
over time.

This change is breaking due to the replacement of the existing
`buffer_utilization_ewma_alpha` config option with
`buffer_utilization_ewma_half_life_seconds`.

authors: bruceg
