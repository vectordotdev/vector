#encoding: utf-8

require 'net/http'

# Makes links available through methods
#
# This class implements reader methods for statically and dynamically defined
# links.
#
# == Validation
#
# All links are validated as a post-processing step via the `LinkPostProcessor`.
#
# == Link categories
#
# Links must be nested under 1 of 3 categories:
#
# 1. `docs` - signals an internal documentation link.
# 2. `url` - signals an external URL.
# 3. `images` - signals a link to an image asset.
#
# == Statically defined linked
#
# Links can be statically defined in the ./metadata.toml file. At the bottom
# of the file is a `[links]` table comprosied of the categories above.
#
# == Dynamically defined linked
#
# To reduce the burden of having to manually define every link this class
# implement dynamic readers:
#
# === /^docs\.(.*)_(sink|source|transform)$/
#
# Links to the documentation file for the specific component.
#
# === /^docs\.(.*)$/
#
# Links to the documentation with the matching name. For example, `regex-parser`
# will match the `docs/usage/configuration/transforms/regex-parser.md` file.
#
# A few things to note about this logic:
#
# 1. It is case insensitive.
# 2. It can match directories, this is useful when you want to link to an
#    entire section.
#
# === /^images\.(.*)$/
#
# Links to the image in the docs/assets folder.
#
# === /^url\.issue_([0-9]+)$/
#
# Links to the specified issue.
#
# === /^url\.(.*)_(sink|source|transform)_issues$/
#
# Links to component issues.
#
# === /^url\.(.*)_(sink|source|transform)_(bugs|enhancements)$/
#
# Links to either bug or enhancement issues for the component.
#
# === /^url\.(.*)_(sink|source|transform)_source$/
#
# Links to the source file for the component.
#
# === /^url\.new_(.*)_(sink|source|transform)_issue$/
#
# Links to the form to create a new issue for the component.
#
# === /^url\.(.*)_test$/
#
# Links to a test in the https://github.com/timberio/vector-test-harness/cases
# directory.=
class Links
  VECTOR_ROOT = "https://github.com/timberio/vector"
  VECTOR_ISSUES_ROOT = "#{VECTOR_ROOT}/issues"
  TEST_HARNESS_ROOT = "https://github.com/timberio/vector-test-harness"

  def initialize(links)    
    @links = links
    @checked_urls = {}

    @docs_files =
      Dir.glob("#{DOCS_ROOT}/**/*").
      to_a.
      collect { |f| f.gsub(DOCS_ROOT, "") }.
      select { |f| !f.start_with?("/assets/") }

    @image_files =
      Dir.glob("#{DOCS_ROOT}/assets/*").
      to_a.
      collect { |f| f.gsub(DOCS_ROOT, "") }
  end

  def fetch(id, opts = {})
    parts = id.split(".")
    category = parts[0]
    name = parts[1]
    section = parts[2]
    full_name = "#{category}.#{name}"

    if parts.length < 2
      raise KeyError.new(
        <<~EOF
        #{full_name.inspect} is not a valid link. Links must start with
        `docs.`, `images.`, or `url.` to signal if the link is internal or
        external. Please fix this link.

        For example: `docs.platforms` or `url.vector_repo` are both valid.
        EOF
      )
    end

    category_links = @links[category] || {}
    path_or_url = category_links[name] || parse(full_name)

    if path_or_url.nil?
      raise KeyError.new("#{full_name} link is not defined")
    end

    if !path_or_url.start_with?("http")
      path_or_url = normalize_path(path_or_url, opts[:current_file])
    end

    if CHECK_URLS
      check!(path_or_url)
    end

    if section
      "#{path_or_url}##{section}"
    else
      path_or_url
    end
  end

  private
    def check!(path_or_url)
      parts = path_or_url.split("#")
      raw_path_or_url = parts.first
      section = parts.last

      if raw_path_or_url.start_with?("/")
        check_file!(raw_path_or_url, section)
      else
        check_url!(raw_path_or_url)
      end
    end

    def check_file!(path, section)
      if !File.exists?("#{DOCS_ROOT}/#{path}")
        raise <<~EOF
        #{path.inspect} references a documentation file that does not exist!

        #{path.inspect}
        EOF
      elsif section
        content = File.read("#{DOCS_ROOT}/#{path}")
        normalized_content = content.gsub("-", " ")
        normalized_section = section.gsub("-", " ")
        if !normalized_content.match(/# #{normalized_section}/i)
          raise <<~EOF
          #{path.inspect} references a section that does not exist!

          #{path.inspect}##{section}
          EOF
        end
      end
    end

    def check_url(url)
      return @checked_urls[url] if @checked_urls.key?(url)

      uri = URI.parse(url)
      req = Net::HTTP.new(uri.host, uri.port)
      req.use_ssl = true if uri.scheme == 'https'
      path = uri.path == "" ? "/" : uri.path
      res = req.request_head(path)
      result = res.code.to_i != 404
      @checked_urls[url] = result
      result
    end

    def check_url!(url)
      if !check_url(url)
        raise <<~EOF
        #{url.inspect} references a dead link!

        #{url.inspect}
        EOF
      else
        true
      end
    end

    def find!(files, name)
      matched_files =
        files.select do |file|
          normalized_file = file.downcase
          
          [normalized_file, normalized_file.gsub("-", "_")].any? do |file_name|
            !(file_name =~ /#{name}(\..*)?$/).nil?
          end
        end

      if matched_files.length == 1
        matched_files.first
      elsif matched_files.length >= 2
        raise <<~EOF
        #{name.inspect} is ambiguous. Please be more specific or
        define the link the /.metadata.toml file. These files matched:

        * #{matched_files.join("\n* ")}
        EOF
      else
        raise <<~EOF
        #{name.inspect} could not be found. Please check the spelling
        or add it to the /.metadata.toml file.
        EOF
      end
    end

    def normalize_path(path, current_file)
      if current_file && current_file.start_with?(DOCS_ROOT)
        relative_root =
          current_file.
            gsub(/^#{DOCS_ROOT}/, "").
            split("/")[0..-3].
            collect { |_| ".." }.
            join("/")

        "#{relative_root}#{path}"
      else
        path = path.gsub(/^docs\//, "/").gsub(".md", "").gsub("/README.md", "")
        "#{VECTOR_DOCS_HOST}#{path}"
      end
    end

    def parse(full_name)
      case full_name
      when /^docs\.(.*)_(sink|source|transform)$/
        sink = $1
        type = $2.pluralize
        "/usage/configuration/#{type}/#{sink}.md"

      when /^docs\.(.*)$/
        name = $1
        find!(@docs_files, name)

      when /^images\.(.*)$/
        name = $1
        find!(@image_files, name)

      when /^url\.issue_([0-9]+)$/
        "#{VECTOR_ISSUES_ROOT}/#{$1}"

      when /^url\.(.*)_(sink|source|transform)_issues$/
        name = $1
        type = $2
        query = "is:open is:issue label:\"#{type.titleize}: #{name}\""
        VECTOR_ISSUES_ROOT + "?" + {"q" => query}.to_query

      when /^url\.(.*)_(sink|source|transform)_(bugs|enhancements)$/
        name = $1
        type = $2
        issue_type = $3.singularize
        query = "is:open is:issue label:\"#{type.titleize}: #{name}\" label:\"Type: #{issue_type.titleize}\""
        VECTOR_ISSUES_ROOT + "?" + {"q" => query}.to_query

      when /^url\.(.*)_(sink|source|transform)_source$/
        name = $1
        type = $2.pluralize
        source_file_url =
          if ["statsd"].include?(name)
            "#{VECTOR_ROOT}/tree/master/src/#{type}/#{name}/mod.rs"
          else
            "#{VECTOR_ROOT}/tree/master/src/#{type}/#{name}.rs"
          end

      when /^url\.new_(.*)_(sink|source|transform)_issue$/
        name = $1
        type = $2
        label = "#{type.titleize}: #{name}"
        VECTOR_ISSUES_ROOT + "/new?" + {"labels" => [label]}.to_query

      when /^url\.(.*)_test$/
        name = $1
        "#{TEST_HARNESS_ROOT}/tree/master/cases/#{name}"

      end
    end
end