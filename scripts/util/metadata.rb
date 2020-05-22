# encoding: utf-8
require "erb"

require "ostruct"
require "toml-rb"

require_relative "metadata/batching_sink"
require_relative "metadata/data_model"
require_relative "metadata/exposing_sink"
require_relative "metadata/field"
require_relative "metadata/guides"
require_relative "metadata/highlight"
require_relative "metadata/installation"
require_relative "metadata/links"
require_relative "metadata/post"
require_relative "metadata/release"
require_relative "metadata/source"
require_relative "metadata/streaming_sink"
require_relative "metadata/transform"

# Object representation of the /.meta directory
#
# This represents the /.meta directory in object form. Sub-classes represent
# each sub-component.
class Metadata
  module Template
    extend self

    def render(path, args = {})
      context = binding

      args.each do |key, value|
        context.local_variable_set("#{key}", value)
      end

      full_path = path.start_with?("/") ? path : "#{META_ROOT}/#{path}"

      if !File.exists?(full_path) && File.exists?("#{full_path}.erb")
        full_path = "#{full_path}.erb"
      end

      body = File.read(full_path)
      renderer = ERB.new(body, nil, '-')

      renderer.result(context)
    end
  end

  class << self
    def load!(meta_dir, docs_root, guides_root, pages_root)
      metadata = load_metadata!(meta_dir)
      validate_schema!(metadata)
      new(metadata, docs_root, guides_root, pages_root)
    end

    private
      def load_metadata!(meta_dir)
        metadata = {}

        contents =
          Dir.glob("#{meta_dir}/**/[^_]*.{toml,toml.erb}").
            sort.
            unshift("#{meta_dir}/root.toml"). # move to the front
            uniq.
            collect do |file|
              begin
                Template.render(file)
              rescue Exception => e
                Printer.error!(
                  <<~EOF
                  The follow metadata file failed to load:

                    #{file}

                  The error received was:

                    #{e.message}
                    #{e.backtrace.join("\n  ")}
                  EOF
                )
              end
            end

        content = contents.join("\n")
        TomlRB.parse(content)
      end

      def validate_schema!(metadata)
        errors = metadata.validate_schema

        if errors.any?
          Printer.error!(
            <<~EOF
            The resulting hash from the `/.meta/**/*.toml` files failed
            validation against the following schema:

                /.meta/schema/meta.json

            The errors include:

                * #{errors[0..50].join("\n*    ")}
            EOF
          )
        end
      end
  end

  attr_reader :blog_posts,
    :data_model,
    :domains,
    :env_vars,
    :guides,
    :highlights,
    :installation,
    :links,
    :options,
    :tests,
    :posts,
    :releases,
    :sinks,
    :sources,
    :team,
    :transforms

  def initialize(hash, docs_root, guides_root, pages_root)
    @data_model = DataModel.new(hash.fetch("data_model"))
    @guides = hash.fetch("guides").to_struct_with_name(constructor: Guides)
    @installation = Installation.new(hash.fetch("installation"))
    @options = hash.fetch("options").to_struct_with_name(constructor: Field)
    @releases = OpenStruct.new()
    @sinks = OpenStruct.new()
    @sources = OpenStruct.new()
    @transforms = OpenStruct.new()
    @tests = Field.new(hash.fetch("tests").merge({"name" => "tests"}))

    # domains

    @domains = hash.fetch("domains").collect { |h| OpenStruct.new(h) }

    # highlights

    @highlights ||=
      Dir.
        glob("#{HIGHLIGHTS_ROOT}/**/*.md").
        filter do |path|
          content = File.read(path)
          content.start_with?("---\n")
        end.
        collect do |path|
          Highlight.new(path)
        end.
        sort_by do |highlight|
          [ highlight.date, highlight.id ]
        end

    # posts

    @posts ||=
      Dir.
        glob("#{POSTS_ROOT}/**/*.md").
        filter do |path|
          content = File.read(path)
          content.start_with?("---\n")
        end.
        collect do |path|
          Post.new(path)
        end.
        sort_by do |post|
          [ post.date, post.id ]
        end

    # releases

    release_versions =
      hash.fetch("releases").collect do |version_string, _release_hash|
        Version.new(version_string)
      end

    hash.fetch("releases").collect do |version_string, release_hash|
      version = Version.new(version_string)

      last_version =
        release_versions.
          select { |other_version| other_version < version }.
          sort.
          last

      release_hash["version"] = version_string
      release = Release.new(release_hash, last_version, @highlights)
      @releases.send("#{version_string}=", release)
    end

    # sources

    hash["sources"].collect do |source_name, source_hash|
      source_hash["name"] = source_name
      source_hash["posts"] = posts.select { |post| post.source?(source_name) }
      source = Source.new(source_hash)
      @sources.send("#{source_name}=", source)
    end

    # transforms

    hash["transforms"].collect do |transform_name, transform_hash|
      transform_hash["name"] = transform_name
      transform_hash["posts"] = posts.select { |post| post.transform?(transform_name) }
      transform = Transform.new(transform_hash)
      @transforms.send("#{transform_name}=", transform)
    end

    # sinks

    hash["sinks"].collect do |sink_name, sink_hash|
      sink_hash["name"] = sink_name
      sink_hash["posts"] = posts.select { |post| post.sink?(sink_name) }

      (sink_hash["service_providers"] || []).each do |service_provider|
        provider_hash = (hash["service_providers"] || {})[service_provider.downcase] || {}
        sink_hash["env_vars"] = (sink_hash["env_vars"] || {}).merge((provider_hash["env_vars"] || {}).clone)
        sink_hash["options"] = sink_hash["options"].merge((provider_hash["options"] || {}).clone)
      end

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

    # links

    @links = Links.new(hash.fetch("links"), docs_root, guides_root, pages_root)

    # env vars

    @env_vars = (hash["env_vars"] || {}).to_struct_with_name(constructor: Field)

    components.each do |component|
      component.env_vars.to_h.each do |key, val|
        @env_vars["#{key}"] = val
      end
    end

    # team

    @team =
      hash.fetch("team").collect do |member|
        OpenStruct.new(member)
      end
  end

  def components
    @components ||= sources_list + transforms_list + sinks_list
  end

  def env_vars_list
    @env_vars_list ||= env_vars.to_h.values.sort
  end

  def event_types
    @event_types ||= data_model.types
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

  def new_post
    return @new_post if defined?(@new_post)

    @new_post ||=
      begin
        last_post = posts.last

        if (Date.today - last_post.date) <= 30
          last_post
        else
          nil
        end
      end
  end

  def new_release
    return @new_post if defined?(@new_post)

    @new_post ||=
      begin
        last_release = releases.releases_list.last

        if (Date.today - last_release.date) <= 30
          last_release
        else
          nil
        end
      end
  end

  def post_tags
    @post_tags ||= posts.collect(&:tags).flatten.uniq
  end

  def platform_names
    @platforms ||=
      begin
        (
          installation.operating_systems_list.collect(&:name) +
          installation.package_managers_list.collect(&:name) +
          installation.package_managers_list.collect(&:archs).flatten.uniq +
          installation.platforms_list.collect(&:name)
        ).sort
      end
  end

  def previous_minor_releases(release)
    releases_list.select do |other_release|
      other_release.version < release.version &&
        other_release.version.major != release.version.major &&
        other_release.version.minor != release.version.minor
    end
  end

  def releases_list
    @releases_list ||= @releases.to_h.values.sort
  end

  def relesed_versions
    releases
  end

  def service_providers
    @service_providers ||= components.collect(&:service_providers).flatten.uniq
  end

  def sinks_list
    @sinks_list ||= sinks.to_h.values.sort
  end

  def sources_list
    @sources_list ||= sources.to_h.values.sort
  end

  def to_h
    {
      event_types: event_types,
      guides: guides.deep_to_h,
      installation: installation.deep_to_h,
      latest_highlight: highlights.last.deep_to_h,
      latest_post: posts.last.deep_to_h,
      latest_release: latest_release.deep_to_h,
      highlights: highlights.deep_to_h,
      posts: posts.deep_to_h,
      post_tags: post_tags,
      releases: releases.deep_to_h,
      sources: sources.deep_to_h,
      team: team.deep_to_h,
      transforms: transforms.deep_to_h,
      sinks: sinks.deep_to_h
    }
  end

  def transforms_list
    @transforms_list ||= transforms.to_h.values.sort
  end
end
