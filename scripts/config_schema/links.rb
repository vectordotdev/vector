require 'net/http'

class Links
  def initialize(links, sources, transforms, sinks, correctness_tests, performance_tests)    
    @links = links
    @sinks = sinks
    @sources = sources
    @transforms = transforms
    @correctness_tests = correctness_tests
    @performance_tests = performance_tests
    @checked = []
    @docs_files = Dir.glob('docs/**/*').to_a
  end

  def fetch(full_name_with_section)
    parts = full_name_with_section.split(".")
    category = parts[0]
    name = parts[1]
    section = parts[2]
    full_name = "#{category}.#{name}"

    if parts.length < 2
      raise KeyError.new(
        <<~EOF
        #{full_name.inspect} is not a valid link. Links must start with `docs.` or `url.`
        to signal if the link is internal or external. Please fix this link.

        For example: `docs.platforms` or `url.vector_repo` are both valid.
        EOF
      )
    end

    category_links =
      begin
        @links.fetch(category)
      rescue KeyError
        raise KeyError.new("The #{category} category is not a valid link category")
      end

    path_or_url =
      if category_links[name].nil?
        case full_name
        when /^url\.issue_([0-9]+)$/
          "#{REPO_ISSUES_ROOT}/#{$1}"

        when /^docs\.(.*)_sink$/
          sink = $1
          if @sinks.to_h.key?(sink.to_sym)
            "/usage/configuration/sinks/#{sink}.md"
          end

        when /^docs\.(.*)_source$/
          source = $1
          if @sources.to_h.key?(source.to_sym)
            "/usage/configuration/sources/#{source}.md"
          end

        when /^url\.(.*_correctness)_test$/
          name = $1
          if @correctness_tests.include?(name)
            "https://github.com/timberio/vector-test-harness/tree/master/cases/#{name}"
          end

        when /^url\.(.*_performance)_test$/
          name = $1
          if @performance_tests.include?(name)
            "https://github.com/timberio/vector-test-harness/tree/master/cases/#{name}"
          end

        when /^docs\.(.*)_transform$/
          transform = $1
          if @transforms.to_h.key?(transform.to_sym)
            "/usage/configuration/transforms/#{transform}.md"
          end

        when /^docs\.(.*)$/
          name = $1
          files = @docs_files.select do |file|
            [file, file.gsub("-", "_")].any? do |file_name|
              file_name.end_with?("#{name}.md") || file_name.end_with?("/#{name}")
            end
          end

          if files.length == 1
            files.first
          elsif files.length >= 2
            raise <<~EOF
            #{full_name.inspect} is ambiguous. Please be more specific or
            define the link the metadata.toml file.
            EOF
          else
            raise <<~EOF
            #{full_name.inspect} could not be found. Please check the spelling
            or add it to the metadata.toml file.
            EOF
          end
        end
      else
        category_links.fetch(name)
      end

    if path_or_url.nil?
      raise KeyError.new("#{full_name} link is not defined")
    end

    if !@checked.include?(full_name)
      check!(full_name, section)
    end

    @checked << full_name

    if section
      "#{path_or_url}##{section}"
    else
      path_or_url
    end
  end

  private
    def check!(path_or_url, section)
      parts = path_or_url.split("#")
      raw_path_or_url = parts.first

      if raw_path_or_url.start_with?("/")
        if !File.exists?("docs/#{raw_path_or_url}")
          raise <<~EOF
          #{full_name.inspect} references a documentation file that does not exist!

          #{raw_path_or_url.inspect}
          EOF
        elsif section
          content = File.read("docs/#{raw_path_or_url}")
          text_section = section.gsub("-", " ")
          if !content.match(/# #{text_section}/i)
            raise <<~EOF
            #{full_name.inspect} references a section that does not exist!

            #{raw_path_or_url.inspect}##{section}
            EOF
          end
        end
      else
        if !working_url?(raw_path_or_url)
          raise <<~EOF
          #{full_name.inspect} references a dead link!

          #{raw_path_or_url.inspect}
          EOF
        end
      end
    end

    def working_url?(url_str)
      url = URI.parse("http://www.google.com/")
      req = Net::HTTP.new(url.host, url.port)
      req.use_ssl = true if url.scheme == 'https'
      res = req.request_head(url.path)
      res.code.to_i
    end
end