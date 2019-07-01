require_relative "../generator"

module Docs
  class SinksGenerator < Generator
    attr_reader :sinks

    def initialize(sinks)
      @sinks = sinks
    end

    def sinks_table
      <<~EOF
      | Name | Description |
      | :--- | :---------- |
      #{sink_rows}
      EOF
    end

    private
      def sink_rows
        links = sinks.collect do |sink|
          "| [**`#{sink.name}`**][docs.#{sink.name}_sink] | #{sink.plural_write_verb.humanize} #{event_type_links(sink.input_types).to_sentence} events to #{sink.write_to_description}. |"
        end

        links.join("\n")
      end
  end
end