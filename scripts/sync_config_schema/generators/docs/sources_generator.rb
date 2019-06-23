require_relative "../generator"

module Docs
  class SourcesGenerator < Generator
    attr_reader :sources

    def initialize(sources)
      @sources = sources
    end

    def generate
      content = <<~EOF
        ---
        description: Receive and pull log and metric events into Vector
        ---

        #{warning}

        # Sources

        ![](../../../assets/sources.svg)

        Sources are responsible for ingesting [events](../../../about/data-model.md#event) into Vector, they can both receive and pull in data. If you're deploying Vector in an [agent role](../../../setup/deployment/roles/agent.md), you'll want to look at local data sources like a [`file`](file.md) and [`stdin`](stdin.md). If you're deploying Vector in a [service role](../../../setup/deployment/roles/service.md), you'll want to look at sources that receive data over the network, like the [`vector`](vector.md), [`tcp`](tcp.md), and [`syslog`](syslog.md) sources.

        | Name | Description |
        | :--- | :---------- |
        #{source_rows}

        [+ request a new source](#{new_source_url()})
      EOF
      content.strip
    end

    private
      def source_rows
        links = sources.collect do |source|
          "| [**`#{source.name}`**](#{source.name}.md) | Ingests data through #{source.through_description} and outputs #{event_type_links(source.output_types)} events.<br />`guarantee: #{source.delivery_guarantee}` |"
        end

        links.join("\n")
      end
  end
end