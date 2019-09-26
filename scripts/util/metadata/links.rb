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
  VECTOR_BRANCH_ROOT = "https://github.com/timberio/vector/tree/v0.3"
  VECTOR_COMMIT_ROOT = "#{VECTOR_ROOT}/commit"
  VECTOR_ISSUES_ROOT = "#{VECTOR_ROOT}/issues"
  VECTOR_PRS_ROOT = "#{VECTOR_ROOT}/pull"
  TEST_HARNESS_ROOT = "https://github.com/timberio/vector-test-harness"

  attr_reader :values

  def initialize(links, docs_root)    
    @links = links
    @values = {}

    @docs =
      Dir.glob("#{docs_root}/**/*").
      to_a.
      collect { |f| f.gsub(docs_root, "") }
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
      available_docs = @docs.select { |doc| !doc.start_with?("/assets/") }

      available_docs =
        if name.end_with?(".readme")
          available_docs
        else
          available_docs.select { |doc| !doc.end_with?("/README.md") }
        end

      found_docs =
        available_docs.select do |doc|
          doc.downcase.gsub(/\.md$/, "").gsub("-", "_").end_with?(normalized_name)
        end

      if found_docs.length == 1
        found_docs.first
      elsif found_docs.length == 0
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

            * #{found_docs.join("\n  * ")}

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
        type = $2

        source_file_url =
          case "#{name}_#{type}"
          when "statsd_source"
            "#{VECTOR_ROOT}/tree/master/src/#{type.pluralize}/#{name}/mod.rs"
          else
            "#{VECTOR_ROOT}/tree/master/src/#{type.pluralize}/#{name}.rs"
          end

      when /^(.*)_test$/
        "#{TEST_HARNESS_ROOT}/tree/master/cases/#{$1}"

      when /^commit_([a-z0-9]+)$/
        "#{VECTOR_COMMIT_ROOT}/#{$1}"

      when /^compare_([a-z0-9_\.]*)\.\.\.([a-z0-9_\.]*)$/
        "https://github.com/timberio/vector/compare/#{$1}...#{$2}"

      when /^issue_([0-9]+)$/
        "#{VECTOR_ISSUES_ROOT}/#{$1}"

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

      when /^v([a-z0-9\-\.]+)$/
        "#{VECTOR_ROOT}/releases/tag/v#{$1}"

      when /^v([a-z0-9\-\.]+)_branch$/
        "#{VECTOR_ROOT}/tree/v#{$1}"

      when /^vector_downloads\.?(.*)$/
        path = $1 == "" ? nil : $1
        ["https://packages.timber.io/vector", path].compact.join("")
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