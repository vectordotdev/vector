The `sample` transform now has a `sample_rate_key` configuration option, which default to `sample_rate`, that allows configuring which key is used to attach the sample rate to sampled events. If set to an empty string, the sample rate will not be attached to sampled events.

authors: dekelpilli
