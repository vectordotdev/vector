require "erb"
require "json_schemer"
require "ostruct"
require "toml-rb"

require_relative "metadata/batching_sink"
require_relative "metadata/data_model"
require_relative "metadata/exposing_sink"
require_relative "metadata/field"
require_relative "metadata/links"
require_relative "metadata/post"
require_relative "metadata/release"
require_relative "metadata/source"
require_relative "metadata/streaming_sink"
require_relative "metadata/transform"
require_relative "metadata/tutorial"

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
      body = File.read(full_path)
      renderer = ERB.new(body, nil, '-')

      renderer.result(context)
    end
  end

  class << self
    def load!(meta_dir, docs_root, guides_root, pages_root)
      metadata = load_metadata!(meta_dir)
      json_schema = load_json_schema!(meta_dir)
      validate_schema!(json_schema, metadata)
      new(metadata, docs_root, guides_root, pages_root)
    end

    private
      def load_json_schema!(meta_dir)
        json_schema = read_json("#{meta_dir}/.schema.json")

        Dir.glob("#{meta_dir}/.schema/**/*.json").each do |file|
          hash = read_json("#{meta_dir}/.schema.json")
          json_schema.deep_merge!(hash)
        end

        json_schema
      end

      def load_metadata!(meta_dir)
        metadata = {}

        contents =
          Dir.glob("#{meta_dir}/**/[^_]*.toml").collect do |file|
            begin
              Template.render(file)
            rescue Exception => e
              error!(
                <<~EOF
                The follow metadata file failed to load:

                  #{file}

                The error received was:

                  #{e.message}
                  #{e.stacktrace.join("\n")}
                EOF
              )
            end
          end

        content = contents.join("\n")
        TomlRB.parse(content)
      end

      def posts
        @posts ||=
          Dir.glob("#{POSTS_ROOT}/**/*.md").collect do |path|
            Post.new(path)
          end.sort_by { |post| [ post.date, post.id ] }
      end

      def read_json(path)
        body = File.read(path)
        JSON.parse(body)
      end

      def validate_schema!(schema, metadata)
        schemer = JSONSchemer.schema(schema, ref_resolver: 'net/http')
        errors = schemer.validate(metadata).to_a
        limit = 50

        if errors.any?
          error_messages =
            errors[0..(limit - 1)].collect do |error|
              "The value at `#{error.fetch("data_pointer")}` failed validation for `#{error.fetch("schema_pointer")}`, reason: `#{error.fetch("type")}`"
            end

          if errors.size > limit
            error_messages << "+ #{errors.size} errors"
          end

          error!(
            <<~EOF
            The metadata schema is invalid. This means the the resulting
            hash from the `/.meta/**/*.toml` files violates the defined
            schema. Errors include:

            * #{error_messages.join("\n* ")}
            EOF
          )
        end
      end
  end

  attr_reader :blog_posts,
    :data_model,
    :domains,
    :env_vars,
    :installation,
    :links,
    :options,
    :tests,
    :posts,
    :releases,
    :sinks,
    :sources,
    :team,
    :transforms,
    :tutorials

  def initialize(hash, docs_root, guides_root, pages_root)
    @data_model = DataModel.new(hash.fetch("data_model"))
    @installation = OpenStruct.new()
    @options = hash.fetch("options").to_struct_with_name(Field)
    @releases = OpenStruct.new()
    @sinks = OpenStruct.new()
    @sources = OpenStruct.new()
    @transforms = OpenStruct.new()
    @tests = Field.new(hash.fetch("tests").merge({"name" => "tests"}))
    @tutorials = hash.fetch("tutorials").to_struct_with_name(Tutorial, should_have_keys: ["steps"])

    # domains

    @domains = hash.fetch("domains").collect { |h| OpenStruct.new(h) }

    # installation

    installation_hash = hash.fetch("installation")
    @installation.containers = installation_hash.fetch("containers").collect { |h| OpenStruct.new(h) }
    @installation.downloads = installation_hash.fetch("downloads").collect { |h| OpenStruct.new(h) }
    @installation.operating_systems = installation_hash.fetch("operating_systems").collect { |h| OpenStruct.new(h) }
    @installation.package_managers = installation_hash.fetch("package_managers").collect { |h| OpenStruct.new(h) }

    # posts

    @posts ||=
      Dir.glob("#{POSTS_ROOT}/**/*.md").collect do |path|
        Post.new(path)
      end.sort_by { |post| [ post.date, post.id ] }

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

      last_date = last_version && hash.fetch("releases").fetch(last_version.to_s).fetch("date").to_date

      release_hash["version"] = version_string
      release = Release.new(release_hash, last_version, last_date, @posts)
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

    @env_vars = (hash["env_vars"] || {}).to_struct_with_name(Field)

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

  def downloads(arch: nil, os: nil, package_manager: nil, type: nil)
    downloads = installation.downloads
    downloads = downloads.select { |d| d.arch && d.arch.downcase == arch.to_s.downcase } if arch
    downloads = downloads.select { |d| d.os && d.os.downcase == os.to_s.downcase } if os
    downloads = downloads.select { |d| d.package_manager && d.package_manager.downcase == package_manager.to_s.downcase } if package_manager
    downloads = downloads.select { |d| d.type && d.type.downcase == type.to_s.downcase } if type
    downloads
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

  def platforms
    @platforms ||= installation.containers +
      installation.operating_systems +
      installation.package_managers
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
      installation: installation.deep_to_h,
      latest_post: posts.last.deep_to_h,
      latest_release: latest_release.deep_to_h,
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
