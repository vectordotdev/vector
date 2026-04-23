Modified behavior of histograms (ddsketch) submitted by the Datadog metrics sink.  Previously the ddsketch sum, count, and average fields were interpolated from bucket boundaries, which could cause slight inaccuracies.  Now those fields will be calculated directly from the corresponding fields on the incoming histogram.  Min and max fields remain interpolated.

Users with alerts, dashboards, or SLOs based on histogram averages should review and adjust thresholds after upgrading, as values will settle to their correct levels.

authors: tony-resendiz
