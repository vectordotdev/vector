require_relative "../generator"

module Docs
  class GlobalGenerator < Generator
    attr_reader :options_table_generator,
      :sections_generator,
      :sources,
      :transforms,
      :sinks

    def initialize(schema)
      @sources = schema.sources.to_h.values.sort
      @transforms = schema.transforms.to_h.values.sort
      @sinks = schema.sinks.to_h.values.sort
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
          include      = ["/var/log/apache2/*.log"]
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
          doc_type     = "_doc"

      # Send structured data to a cost-effective long-term storage
      [sinks.s3_archives]
          inputs       = ["apache_parser"] # don't sample
          type         = "aws_s3"
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

      ## Sources

      | Name  | Description |
      | :---  | :---------- |
      #{source_rows}

      [+ request a new source](#{new_source_url()})

      ## Transforms

      | Name  | Description |
      | :---  | :---------- |
      #{transform_rows}

      [+ request a new transform](#{new_transform_url()})

      ## Sinks

      | Name  | Description |
      | :---  | :---------- |
      #{sink_rows}

      [+ request a new sink](#{new_sink_url()})

      ## How It Works

      #{sections_generator.generate}
      EOF
    end

    private
      def source_rows
        links = sources.collect do |source|
          "| [**`#{source.name}`**][#{component_short_link(source)}] | Ingests data through #{source.through_description}. |"
        end

        links.join("\n")
      end

      def transform_rows
        links = transforms.collect do |transform|
          "| [**`#{transform.name}`**][#{component_short_link(transform)}] | Allows you to #{transform.allow_you_to_description}. |"
        end

        links.join("\n")
      end

      def sink_rows
        links = sinks.collect do |sink|
          "| [**`#{sink.name}`**][#{component_short_link(sink)}] | #{sink.plural_write_verb.humanize} events to #{sink.write_to_description}. |"
        end

        links.join("\n")
      end
  end
end