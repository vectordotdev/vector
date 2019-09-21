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
  VECTOR_ROOT = "https://github.com/timberio/vector"
  VECTOR_COMMIT_ROOT = "#{VECTOR_ROOT}/commit"
  VECTOR_ISSUES_ROOT = "#{VECTOR_ROOT}/issues"
  VECTOR_PRS_ROOT = "#{VECTOR_ROOT}/pull"
  VECTOR_RELEASE_ROOT = "#{VECTOR_ROOT}/releases/tag"
  TEST_HARNESS_ROOT = "https://github.com/timberio/vector-test-harness"

  attr_reader :values

  def initialize(links)    
    @links = links
    @values = {}

    @docs =
      Dir.glob("#{DOCS_ROOT}/**/*").
      to_a.
      collect { |f| f.gsub(DOCS_ROOT, "") }.
      select { |f| !f.end_with?("README.md") }
  end

  def []=(id)
    fetch(id)
  rescue KeyError
    nil
  end

  def fetch(id)
    id_parts = id.split(".", 2)
    category = id_parts[0]
    suffix = id_parts[1]
    suffix_parts = suffix.split("#", 2)
    name = suffix_parts[0]
    section = suffix_parts[1]

    base_value =
      case category
      when "assets"
        fetch_asset(name)
      when "docs"
        fetch_doc(name)
      when "urls"
        fetch_url(name)
      else
        raise ArgumentError.new(
          <<~EOF
          Invalid link category!

            #{category.inspect}

          Links must start with `docs.`, `images.`, or `urls.`
          EOF
        )
      end

    value = [base_value, section].compact.join("#")
    @values[id] ||= value
    value
  end

  private
    def fetch_asset(name)
      normalized_name = name.downcase.gsub(".", "/").gsub("-", "_")

      assets =
        @docs.
          select { |doc| doc.start_with?("/assets/") }.
          select do |doc|
            basename = File.basename(doc, ".*").downcase.gsub("-", "_")
            basename == normalized_name
          end

      if assets.length == 1
        assets.first
      elsif assets.length == 0
        raise KeyError.new(
          <<~EOF
          Unkknown asset name!

            assets.#{name}

          This link does not match any assets.
          EOF
        )
      else
        raise KeyError.new(
          <<~EOF
          Ambiguous asset name!

            assets.#{name}

          This link matches more than 1 asset:

            * #{assets.join("\n  * ")}

          Please use something more specific that will match only a single asset.
          EOF
        )
      end 
    end

    def fetch_doc(name)
      normalized_name = name.downcase.gsub(".", "/").gsub("-", "_").split("#", 2).first

      docs =
        @docs.
          select { |doc| !doc.start_with?("/assets/") }.
          select do |doc|
            doc.downcase.gsub(/\.md$/, "").gsub("-", "_").end_with?(normalized_name)
          end

      if docs.length == 1
        docs.first
      elsif docs.length == 0
        raise KeyError.new(
          <<~EOF
          Unknown link name!

            docs.#{name}

          This link does not match any documents.
          EOF
        )
      else
        raise KeyError.new(
          <<~EOF
          Ambiguous link name!

            docs.#{name}

          This link matches more than 1 doc:

            * #{docs.join("\n  * ")}

          Please use something more specific that will match only a single document.
          EOF
        )
      end
    end

    def fetch_url(name)
      @links.fetch("urls")[name] || fetch_dynamic_url(name)
    end

    def fetch_dynamic_url(name)
      case name
      when /^commit_([a-z0-9]+)$/
        "#{VECTOR_COMMIT_ROOT}/#{$1}"

      when /^issue_([0-9]+)$/
        "#{VECTOR_ISSUES_ROOT}/#{$1}"

      when /^pr_([0-9]+)$/
        "#{VECTOR_PRS_ROOT}/#{$1}"

      when /^v([a-z0-9-]+)$/
        version = $1.gsub("-", ".")
        "#{VECTOR_RELEASE_ROOT}/#{version}"

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
        type = $2.pluralize
        source_file_url =
          if ["statsd"].include?(name)
            "#{VECTOR_ROOT}/tree/master/src/#{type}/#{name}/mod.rs"
          else
            "#{VECTOR_ROOT}/tree/master/src/#{type}/#{name}.rs"
          end

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

      when /^vector_latest_(release|nightly)_(.*)/
        channel = $1 == "release" ? "latest" : $1
        target = $2
        "https://packages.timber.io/vector/#{channel}/vector-#{channel}-#{target}.tar.gz"
        
      when /^(.*)_test$/
        name = $1
        "#{TEST_HARNESS_ROOT}/tree/master/cases/#{name}"

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
end