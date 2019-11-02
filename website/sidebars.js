module.exports = {
  docs: [
    {
      type: 'category',
      label: 'Introduction',
      items: [
        'what_is_vector',
        'about/concepts',
        {
          type: 'category',
          label: 'Data Model',
          items: [
            'about/data-model/README',
            'about/data-model/log',
            'about/data-model/metric',
          ],
        },
        'about/guarantees',
      ],
    },
    {
      type: 'category',
      label: 'Setup',
      items: [
        {
          type: 'category',
          label: 'Installation',
          items: ['setup/installation/README'],
        },
      ],
    },
    {
      type: 'category',
      label: 'Components',
      items: [
        {
          type: 'category',
          label: 'Sources',
          items: ["usage/configuration/sources/docker","usage/configuration/sources/file","usage/configuration/sources/journald","usage/configuration/sources/kafka","usage/configuration/sources/statsd","usage/configuration/sources/stdin","usage/configuration/sources/syslog","usage/configuration/sources/tcp","usage/configuration/sources/udp","usage/configuration/sources/vector"],
        },
        {
          type: 'category',
          label: 'Transforms',
          items: ["usage/configuration/transforms/add_fields","usage/configuration/transforms/add_tags","usage/configuration/transforms/coercer","usage/configuration/transforms/field_filter","usage/configuration/transforms/grok_parser","usage/configuration/transforms/json_parser","usage/configuration/transforms/log_to_metric","usage/configuration/transforms/lua","usage/configuration/transforms/regex_parser","usage/configuration/transforms/remove_fields","usage/configuration/transforms/remove_tags","usage/configuration/transforms/sampler","usage/configuration/transforms/split","usage/configuration/transforms/tokenizer"],
        },
        {
          type: 'category',
          label: 'Sinks',
          items: ["usage/configuration/sinks/aws_cloudwatch_logs","usage/configuration/sinks/aws_cloudwatch_metrics","usage/configuration/sinks/aws_kinesis_streams","usage/configuration/sinks/aws_s3","usage/configuration/sinks/blackhole","usage/configuration/sinks/clickhouse","usage/configuration/sinks/console","usage/configuration/sinks/datadog_metrics","usage/configuration/sinks/elasticsearch","usage/configuration/sinks/file","usage/configuration/sinks/http","usage/configuration/sinks/kafka","usage/configuration/sinks/prometheus","usage/configuration/sinks/splunk_hec","usage/configuration/sinks/statsd","usage/configuration/sinks/tcp","usage/configuration/sinks/vector"],
        },
      ],
    },
    {
      type: 'category',
      label: 'Administration',
      items: ['about/guarantees'],
    },
    {
      type: 'category',
      label: 'Meta',
      items: ['about/guarantees'],
    },
  ]
};



