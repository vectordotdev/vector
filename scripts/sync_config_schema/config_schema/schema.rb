require_relative "batching_sink"
require_relative "links"
require_relative "section"
require_relative "source"
require_relative "streaming_sink"
require_relative "transform"

class Schema
  attr_reader :enums,
    :guides,
    :links,
    :options,
    :sections,
    :sinks,
    :sources,
    :transforms

  def initialize(hash)
    @enums = OpenStruct.new(hash.fetch("enums"))
    @options = OpenStruct.new()
    @sinks = OpenStruct.new()
    @sections = hash.fetch("sections").collect { |h| Section.new(h) }
    @sources = OpenStruct.new()
    @transforms = OpenStruct.new()

    # sources

    hash["sources"].collect do |source_name, source_hash|
      source_hash["name"] = source_name
      source = Source.new(source_hash)
      @sources.send("#{source_name}=", source)
    end

    # transforms

    hash["transforms"].collect do |transform_name, transform_hash|
      transform_hash["name"] = transform_name
      transform = Transform.new(transform_hash)
      @transforms.send("#{transform_name}=", transform)
    end

    # sinks

    hash["sinks"].collect do |sink_name, sink_hash|
      sink_hash["name"] = sink_name
      sink = sink_hash.fetch("write_style") == "batching" ?
        BatchingSink.new(sink_hash) : 
        StreamingSink.new(sink_hash)
      @sinks.send("#{sink_name}=", sink)
    end

    transforms_list = @transforms.to_h.values
    transforms_list.each do |transform|
      alternatives = transforms_list.select do |alternative|
        function_diff = alternative.function_categories - transform.function_categories
        alternative != transform && function_diff != alternative.function_categories
      end

      transform.alternatives = alternatives.sort
    end

    # options

    hash.fetch("options").each do |option_name, option_hash|
      option = Option.new(
        option_hash.merge({"name" => option_name}
      ))

      @options.send("#{option_name}=", option)
    end

    # guides

    @guides =
      Dir["docs/usage/guides/*.md"].
        select { |file| file != "README.md" }.
        collect { |file| Guide.new(file) }

    # links

    @links = Links.new(
      hash.fetch("links"),
      sources,
      transforms,
      sinks,
      @enums.correctness_tests,
      @enums.performance_tests
    )
  end
end