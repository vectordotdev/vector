The `webhdfs` sink now retries temporary write failures instead of immediately
dropping the affected batch. Retry behavior and other outbound request settings
can be configured through the `request` options.

authors: zaugust4891
