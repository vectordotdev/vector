#encoding: utf-8

module PostProcessors
  # Responsible for resolving link definitions.
  #
  # This classes parses all links out of our markdown files and adds definitions
  # at the bottom of the file.
  #
  # See the `Links` class for more info on how links are resoolved.
  class LinkDefiner
    VECTOR_DOCS_HOST = "https://docs.vector.dev"
    
    class << self
      def define!(*args)
        new(*args).define!
      end

      def remove_link_footers(content)
        parts = content.partition(/\n\n\[([a-zA-Z0-9_\-\.\/# ]*)\]:/)
        parts.first.strip
      end
    end

    attr_reader :content, :docs_root, :links, :file_path, :opts

    def initialize(content, file_path, links, opts = {})
      @content = self.class.remove_link_footers(content)
      @links = links
      @file_path = file_path
      @opts = opts

      if in_docs?
        @docs_root = @file_path.gsub(DOCS_ROOT + "/", "").split("/")[1..-1].collect { |_| ".." }.join("/")

        if @docs_root == ""
          @docs_root = "."
        end
      end
    end

    def define!
      if !file_path.end_with?("SUMMARY.md") && !file_path.end_with?("conventions.md")
        verify_no_direct_links!
      end

      link_names = content.scan(/\]\[([a-zA-Z0-9_\-\.\/# ]*)\]/).flatten.uniq

      footer_links = []

      link_names.each do |link_name|
        definition = links.fetch(link_name)

        if definition.start_with?("/")
          if in_docs?
            definition = docs_root + definition
          else
            definition = VECTOR_DOCS_HOST + definition.gsub(/\.md$/, "")
          end
        end

        footer_links << "[#{link_name}]: #{definition}"
      end

      <<~EOF
      #{content}

      
      #{footer_links.sort.join("\n")}
      EOF
    end

    private
      def in_docs?
        @file_path.start_with?(DOCS_ROOT)
      end

      def verify_no_direct_links!
        direct_links = content.scan(/\]\([^#]([a-zA-Z0-9_\-\.\/# ]*)\)/).flatten.uniq

        if direct_links.any?
          raise <<~EOF
          You used a direct link in the #{file_path.inspect} file:

          #{direct_links.first.inspect}

          This is not allowed in the Vector documentation for validation purposes.
          Please:

          1. Update your links to use a short link.
          2. Add the short link to the /.meta/links.toml file.
          EOF
        end

        true
      end
  end
end