Modified behavior of histograms (ddsketch) submitted by the Datadog metrics sink.  Previously the ddsketch field average was interpolated from bucket boundaries, which could cause slight inaccuracies.  Now the average will be calculated directly from the incoming histogram fields sum / count.

Users with alerts, dashboards, or SLOs based on histogram averages should review and adjust thresholds after upgrading, as average values will settle to their correct levels.

authors: tony-resendiz
