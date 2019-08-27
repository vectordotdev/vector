require_relative "metadata/batching_sink"
require_relative "metadata/exposing_sink"
require_relative "metadata/links"
require_relative "metadata/source"
require_relative "metadata/streaming_sink"
require_relative "metadata/transform"

# Object representation of the /.metadata.toml file
#
# This represents the /.metadata.toml in object form. Sub-classes represent
# each sub-component.
class Metadata
  class << self
    def load()
      metadata_toml = TomlRB.load_file("#{DOCS_ROOT}/../.metadata.toml")
      companies_toml = TomlRB.load_file("#{DOCS_ROOT}/../.companies.toml")
      new(metadata_toml.merge(companies_toml))
    end
  end

  attr_reader :companies,
    :enums,
    :links,
    :options,
    :sinks,
    :sources,
    :transforms

  def initialize(hash)
    @companies = hash.fetch("companies")
    @enums = OpenStruct.new(hash.fetch("enums"))
    @options = OpenStruct.new()
    @sinks = OpenStruct.new()
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

      sink =
        case sink_hash.fetch("egress_method")
        when "batching"
          BatchingSink.new(sink_hash)
        when "exposing"
          ExposingSink.new(sink_hash)
        when "streaming"
          StreamingSink.new(sink_hash)
        end

      @sinks.send("#{sink_name}=", sink)
    end

    transforms_list = @transforms.to_h.values
    transforms_list.each do |transform|
      alternatives = transforms_list.select do |alternative|
        if transform.function_categories != ["convert_types"] && alternative.function_categories.include?("program")
          true
        else
          function_diff = alternative.function_categories - transform.function_categories
          alternative != transform && function_diff != alternative.function_categories
        end
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

    # links

    @links = Links.new(hash.fetch("links"))
  end
end