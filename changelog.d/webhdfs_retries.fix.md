The `webhdfs` sink now retries temporary write failures instead of immediately
dropping the affected batch. You can configure retry behavior and other
outbound request settings through the `request` options.

authors: zaugust4891
