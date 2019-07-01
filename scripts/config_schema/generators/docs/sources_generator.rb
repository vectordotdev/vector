require_relative "../generator"

module Docs
  class SourcesGenerator < Generator
    attr_reader :sources

    def initialize(sources)
      @sources = sources
    end

    def sources_table
      <<~EOF
      | Name | Description |
      | :--- | :---------- |
      #{source_rows}
      EOF
    end

    def generate
      content = <<~EOF
        ---
        description: Receive and pull log and metric events into Vector
        ---

        #{warning}

        # Sources

        ![](../../../assets/sources.svg)

        Sources are responsible for ingesting [events][docs.event] into Vector, they can both receive and pull in data. If you're deploying Vector in an [agent role][docs.agent_role], you'll want to look at local data sources like a [`file`][docs.file_source] and [`stdin`][docs.stdin_source]. If you're deploying Vector in a [service role][docs.service_role], you'll want to look at sources that receive data over the network, like the [`vector`][docs.vector_source], [`tcp`][docs.tcp_source], and [`syslog`][docs.syslog_source] sources.

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
          "| [**`#{source.name}`**][docs.#{source.name}_source] | Ingests data through #{source.through_description} and outputs #{event_type_links(source.output_types).to_sentence} events. |"
        end

        links.join("\n")
      end
  end
end