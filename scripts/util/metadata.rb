require "ostruct"
require "toml-rb"

require_relative "metadata/batching_sink"
require_relative "metadata/exposing_sink"
require_relative "metadata/links"
require_relative "metadata/release"
require_relative "metadata/source"
require_relative "metadata/streaming_sink"
require_relative "metadata/transform"

# Object representation of the /.meta directory
#
# This represents the /.meta directory in object form. Sub-classes represent
# each sub-component.
class Metadata
  class << self
    def load!(meta_dir, docs_root)
      metadata = {}

      Dir.glob("#{meta_dir}/**/*.toml").each do |file|
        hash = TomlRB.load_file(file)
        metadata.deep_merge!(hash)
      end

      new(metadata, docs_root)
    end
  end

  attr_reader :companies,
    :installation,
    :links,
    :options,
    :releases,
    :sinks,
    :sources,
    :transforms

  def initialize(hash, docs_root)
    @companies = hash.fetch("companies")
    @installation = OpenStruct.new()
    @options = OpenStruct.new()
    @releases = OpenStruct.new()
    @sinks = OpenStruct.new()
    @sources = OpenStruct.new()
    @transforms = OpenStruct.new()

    # installation

    installation_hash = hash.fetch("installation")
    @installation.platforms = installation_hash.fetch("platforms").collect { |h| OpenStruct.new(h) }
    @installation.operating_systems = installation_hash.fetch("operating_systems").collect { |h| OpenStruct.new(h) }
    @installation.package_managers = installation_hash.fetch("package_managers").collect { |h| OpenStruct.new(h) }

    # releases

    release_versions =
      hash.fetch("releases").collect do |version_string, _release_hash|
        Version.new(version_string)
      end

    # Seed the list of releases with the first version
    release_versions << Version.new("0.3.0")

    hash.fetch("releases").collect do |version_string, release_hash|
      version = Version.new(version_string)

      last_version =
        release_versions.
          select { |other_version| other_version < version }.
          sort.
          last

      release_hash["version"] = version_string
      release = Release.new(release_hash, last_version)
      @releases.send("#{version_string}=", release)
    end

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

    @links = Links.new(hash.fetch("links"), docs_root)
  end

  def components
    @components ||= sources.to_h.values + transforms.to_h.values + sinks.to_h.values
  end

  def latest_patch_releases
    version = Version.new("#{latest_version.major}.#{latest_version.minor}.0")

    releases_list.select do |release|
      release.version >= version
    end
  end

  def latest_release
    @latest_release ||= releases_list.last
  end

  def latest_version
    @latest_version ||= latest_release.version
  end

  def newer_releases(release)
    releases_list.select do |other_release|
      other_release > release
    end
  end

  def previous_minor_releases(release)
    releases_list.select do |other_release|
      other_release.version < release.version &&
        other_release.major != release.major &&
        other_release.minor != release.minor
    end
  end

  def releases_list
    @releases_list ||= @releases.to_h.values.sort
  end

  def relesed_versions
    releases
  end
end