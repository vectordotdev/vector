#encoding: utf-8

module PostProcessors
  # Responsible for resolving link definitions.
  #
  # This classes parses all links out of our markdown files and adds definitions
  # at the bottom of the file.
  #
  # See the `Links` class for more info on how links are resoolved.
  class LinkDefiner
    class << self
      def define!(*args)
        new(*args).define!
      end

      def remove_link_footers(content)
        parts = content.partition(/\n\n\[([a-zA-Z0-9_\-\.\/#\?= ]*)\]:/)
        parts.first.strip
      end
    end

    attr_reader :content, :links, :file_path, :opts

    def initialize(content, file_path, links, opts = {})
      @content = self.class.remove_link_footers(content)
      @links = links
      @file_path = file_path
      @opts = opts
    end

    def define!
      verify_no_direct_links!

      link_ids = content.scan(/\[\[\[([a-zA-Z0-9_\-\.\/#\?= ]*)\]\]\]/).flatten.uniq

      link_ids.each do |link_id|
        definition = get_path_or_url(link_id)
        content.gsub!("[[[#{link_id}]]]", definition)
      end

      link_ids = content.scan(/\]\[([a-zA-Z0-9_\-\.\/#\?= ]*)\]/).flatten.uniq

      footer_links = []

      link_ids.each do |link_id|
        definition = get_path_or_url(link_id)
        footer_links << "[#{link_id}]: #{definition}"
      end

      <<~EOF
      #{content}

      
      #{footer_links.sort.join("\n")}
      EOF
    end

    private
      def get_path_or_url(link_id)
        definition = links.fetch(link_id)

        if definition.start_with?("/")
          if !in_website?
            definition = HOST + definition.gsub(/\.md$/, "")
          end
        end

        definition
      end

      def in_website?
        @file_path.start_with?(WEBSITE_ROOT)
      end

      def verify_no_direct_links!
        direct_links = content.scan(/\]\([^#]([a-zA-Z0-9_\-\.\/?= ]*)\)/).flatten.uniq

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