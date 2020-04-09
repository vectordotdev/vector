#encoding: utf-8

require 'net/http'

# Links
#
# This class implements reader methods for statically and dynamically defined
# links.
#
# == Link categories
#
# Links must be nested under 1 of 3 categories:
#
# 1. `assets` - signals a link to a file in the docs/assets folder.
# 2. `docs` - signals an internal documentation link.
# 3. `url` - signals an external URL.
#
# == Statically defined linked
#
# Links can be statically defined in the ./meta/links.toml file.
#
# == Dynamically defined linked
#
# To reduce the burden of having to manually define every link this class
# implement dynamic readers that can be found in the `#fetch_dynamic_url`
# method.
class Links
  CATEGORIES = ["assets", "docs", "guides", "pages", "urls"].freeze
  VECTOR_ROOT = "https://github.com/timberio/vector".freeze
  VECTOR_COMMIT_ROOT = "#{VECTOR_ROOT}/commit".freeze
  VECTOR_ISSUES_ROOT = "#{VECTOR_ROOT}/issues".freeze
  VECTOR_MILESTONES_ROOT = "#{VECTOR_ROOT}/milestone".freeze
  VECTOR_PRS_ROOT = "#{VECTOR_ROOT}/pull".freeze
  TEST_HARNESS_ROOT = "https://github.com/timberio/vector-test-harness".freeze

  attr_reader :values

  def initialize(links, docs_root, guides_root, pages_root)
    @links = links
    @values = {}

    @docs =
      Dir.glob("#{docs_root}/**/*.md").
      to_a.
      reject { |p| File.directory?(p) }.
      collect { |f| f.gsub(docs_root, "").split(".").first }

    @guides =
      Dir.glob("#{guides_root}/**/*.md").
      to_a.
      reject { |p| File.directory?(p) }.
      collect { |f| f.gsub(guides_root, "").split(".").first }

    @pages =
      Dir.glob("#{pages_root}/**/*.js").
      to_a.
      reject { |p| File.directory?(p) }.
      collect { |f| f.gsub(pages_root, "").split(".").first }
  end

  def []=(id)
    fetch(id)
  rescue KeyError
    nil
  end

  def exists?(id)
    fetch(id)
    true
  rescue KeyError
    false
  end

  def fetch(id)
    id_parts = id.split(".", 2)

    if id_parts.length != 2
      raise ArgumentError.new("Link id is invalid! #{id}")
    end

    category = id_parts[0]
    suffix = id_parts[1]
    hash_parts = suffix.split("#", 2)
    name = hash_parts[0]
    hash = hash_parts[1]
    query_parts = name.split("?", 2)
    name = query_parts[0]
    query = query_parts[1]

    value =
      case category
      when "assets"
        fetch_asset_path(name)
      when "docs"
        fetch_doc_path(name)
      when "guides"
        fetch_guide_path(name)
      when "pages"
        fetch_page_path(name)
      when "urls"
        fetch_url(name)
      else
        raise ArgumentError.new(
          <<~EOF
          Invalid link category!

            #{category.inspect}

          Links must start with `assets.`, `docs.`, `guides.`, `.pages`, or `urls.`
          EOF
        )
      end

    value = [value, query].compact.join("?")
    value = [value, hash].compact.join("#")
    @values[id] ||= value
    value
  end

  def fetch_id(id)
    # Docusaurus does not allow a leading or trailing `/`
    fetch(id).gsub(/^#{DOCS_BASE_PATH}\//, "").gsub(/\/$/, "")
  end

  private
    def fetch!(namespace, items, name)
      if @links[namespace] && @links[namespace][name]
        return @links[namespace][name]
      end

      normalized_name = name.downcase.gsub(".", "/").gsub("-", "_").split("#", 2).first

      found_items =
        items.select do |item|
          item.downcase.gsub("-", "_").end_with?(normalized_name)
        end

      if found_items.length == 1
        found_items.first
      elsif found_items.length == 0
        raise KeyError.new(
          <<~EOF
          Unknown link name!

            #{namespace}.#{name}

          This link does not match any documents.
          EOF
        )
      else
        raise KeyError.new(
          <<~EOF
          Ambiguous link name!

            #{namespace}.#{name}

          This link matches more than 1 doc:

            * #{found_items.join("\n  * ")}

          Please use something more specific that will match only a single document.
          EOF
        )
      end
    end

    def fetch_asset_path(name)
      assets =
        @docs.
          select { |doc| doc.start_with?("/assets/") }.
          select do |doc|
            basename = File.basename(doc, ".*").downcase.gsub("-", "_")
            basename == normalized_name
          end

      fetch!("assets", assets, name)
    end

    def fetch_doc_path(name)
      available_docs =
        if name.end_with?(".readme")
          @docs
        else
          @docs.select { |doc| !doc.end_with?("/README.md") }
        end

      DOCS_BASE_PATH + fetch!("docs", available_docs, name) + "/"
    end

    def fetch_guide_path(name)
      case name
      when "advanced"
        return "#{GUIDES_BASE_PATH}/advanced/"
      when "getting-started"
        return "#{GUIDES_BASE_PATH}/getting-started/"
      when "index"
        return GUIDES_BASE_PATH
      end

      available_guides =
        if name.end_with?(".readme")
          @guides
        else
          @guides.select { |guide| !guide.end_with?("/README.md") }
        end

      GUIDES_BASE_PATH + fetch!("guides", available_guides, name) + "/"
    end

    def fetch_page_path(name)
      if name == "index"
        "/"
      else
        fetch!("pages", @pages, name) + "/"
      end
    end

    def fetch_dynamic_url(name)
      case name
      when /^(.*)_(sink|source|transform)_issues$/
        name = $1
        type = $2
        query = "is:open is:issue label:\"#{type}: #{name}\""
        VECTOR_ISSUES_ROOT + "?" + {"q" => query}.to_query

      when /^(.*)_(sink|source|transform)_(bugs|enhancements)$/
        name = $1
        type = $2
        issue_type = $3.singularize
        query = "is:open is:issue label:\"#{type}: #{name}\" label:\"Type: #{issue_type}\""
        VECTOR_ISSUES_ROOT + "?" + {"q" => query}.to_query

      when /^(.*)_(sink|source|transform)_source$/
        name = $1
        name_parts = name.split("_")
        name_prefix = name_parts.first
        suffixed_name = name_parts[1..-1].join("_")
        type = $2

        variations =
          [
            "#{name}.rs",
            "#{name_prefix}/#{suffixed_name}.rs",
            "#{name}",
            "#{name_prefix}"
          ]

        paths =
          variations.collect do |variation|
            "#{VECTOR_ROOT}/tree/master/src/#{type.pluralize}/#{variation}"
          end

        variations.each do |variation|
          path = "#{ROOT_DIR}/src/#{type.pluralize}/#{variation}"
          if File.exists?(path) || File.directory?(path)
            return "#{VECTOR_ROOT}/tree/master/src/#{type.pluralize}/#{variation}"
          end
        end

        raise KeyError.new(
          <<~EOF
          Unknown link!

            urls.#{name}_source

          We tried the following paths:

            * #{paths.join("\n  * ")}

          If the path to the source file is unique, please add it to the
          links.toml file.
          EOF
        )

      when /^(.*)_test$/
        "#{TEST_HARNESS_ROOT}/tree/master/cases/#{$1}"

      when /^commit_([a-z0-9]+)$/
        "#{VECTOR_COMMIT_ROOT}/#{$1}"

      when /^compare_([a-z0-9_\.]*)\.\.\.([a-z0-9_\.]*)$/
        "https://github.com/timberio/vector/compare/#{$1}...#{$2}"

      when /^issue_([0-9]+)$/
        "#{VECTOR_ISSUES_ROOT}/#{$1}"

      when /^milestone_([0-9]+)$/
        "#{VECTOR_MILESTONES_ROOT}/#{$1}"

      when /^new_(.*)_(sink|source|transform)_issue$/
        name = $1
        type = $2
        label = "#{type}: #{name}"
        VECTOR_ISSUES_ROOT + "/new?" + {"labels" => [label]}.to_query

      when /^new_(.*)_(sink|source|transform)_(bug|enhancement)$/
        name = $1
        type = $2
        issue_type = $3.singularize
        component_label = "#{type}: #{name}"
        type_label = "Type: #{issue_type}"
        VECTOR_ISSUES_ROOT + "/new?" + {"labels" => [component_label, type_label]}.to_query

      when /^pr_([0-9]+)$/
        "#{VECTOR_PRS_ROOT}/#{$1}"

      when /^release_notes_([a-z0-9_\.]*)$/
        "#{HOST}/releases/#{$1}"

      when /^v([a-z0-9\-\.]+)$/
        "#{HOST}/releases/#{$1}/download"

      when /^v([a-z0-9\-\.]+)_branch$/
        "#{VECTOR_ROOT}/tree/v#{$1}"

      when /^vector_downloads\.?(.*)$/
        path = $1 == "" ? nil : $1
        ["https://packages.timber.io/vector", path].compact.join("/")
      else
        raise KeyError.new(
          <<~EOF
          Unknown link!

            urls.#{name}

          URL links must match a link defined in ./meta/links.toml or it
          must match a supported dynamic link, such as `urls.issue_541`.
          EOF
        )
      end
    end

    def fetch_url(name)
      @links.fetch("urls")[name] || fetch_dynamic_url(name)
    end
end
