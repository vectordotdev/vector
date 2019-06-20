require_relative "generator"

class GlobalGenerator < Generator
  attr_reader :options_table_generator,
    :sections_generator

  def initialize(schema)
    options = schema.options.to_h.values.sort
    @options_table_generator = OptionsTableGenerator.new(options, schema.sections)
    @sections_generator = SectionsGenerator.new(schema.sections.sort)
  end

  def generate
    <<~EOF
    ---
    description: Vector configuration
    ---

    #{warning}

    # Configuration

    ![](../../assets/configure.svg)

    This section covers configuring Vector and creating [pipelines](../../about/concepts.md#pipelines) like the one shown above. Vector requires only a _single_ [TOML](https://github.com/toml-lang/toml) configurable file, which you can specify via the [`--config` flag](../administration/starting.md#options) when [starting](../administration/starting.md) vector:

    ```bash
    vector --config /etc/vector/vector.toml
    ```

    ## Example

    {% code-tabs %}
    {% code-tabs-item title="vector.toml" %}
    ```coffeescript
    data_dir = "/var/lib/vector"

    # Ingest data by tailing one or more files
    [sources.apache_logs]
        type         = "file"
        path         = "/var/log/apache2/*.log"
        ignore_older = 86400 # 1 day

    # Structure and parse the data
    [transforms.apache_parser]
        inputs        = ["apache_logs"]
      type            = "regex_parser"
      regex           = '^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[(?P<timestamp>.*)\] "(?P<method>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$'

    # Sample the data to save on cost
    [transforms.apache_sampler]
        inputs       = ["apache_parser"]
        type         = "sampler"
        hash_field   = "request_id" # sample _entire_ requests
        rate         = 10 # only keep 10%

    # Send structured data to a short-term storage
    [sinks.es_cluster]
        inputs       = ["apache_sampler"]
        type         = "elasticsearch"
        host         = "79.12.221.222:9200"

    # Send structured data to a cost-effective long-term storage
    [sinks.s3_archives]
        inputs       = ["apache_parser"] # don't sample
        type         = "s3"
        region       = "us-east-1"
        bucket       = "my_log_archives"
        batch_size   = 10000000 # 10mb uncompressed
        gzip         = true
        encoding     = "ndjson"
    ```
    {% endcode-tabs-item %}
    {% endcode-tabs %}

    ## Global Options

    #{options_table_generator.generate}

    ## How It Works

    #{sections_generator.generate}
    EOF
  end
end