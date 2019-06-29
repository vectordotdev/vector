require_relative "../generator"

module Docs
  class GuaranteesGenerator < Generator
    attr_reader :sources, :sinks

    def initialize(sources, sinks)
      @sources = sources
      @sinks = sinks
    end

    def support_matrix_table
      <<~EOF
      | Name | Description |
      | :--- | :---------- |
      #{support_matrix}
      EOF
    end

    private
      def support_matrix
        links = (sources + sinks).collect do |component|
          "| #{component_link(component)} | `#{component.delivery_guarantee}` |"
        end

        links.sort.join("\n")
      end
  end
end