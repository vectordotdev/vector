#encoding: utf-8

module PostProcessors
  # Responsible for checking and normalizing links in all of our docs.
  #
  # This classes parses all links out of our markdown files and checks that
  # they are valid and then returns content with the links normalized.
  # The purposes is to automate the tedious task of defining links and
  # ensuring they never become broken.
  #
  # ==== Validation
  #
  # * Local links are validated, ensuring the file exists
  # * External links are validated by making a HEAD request to the endpoint
  # * If link links to a specific section the document is searched to ensure
  #   that section actually exists.
  #
  # ==== "Magic" Short Links
  #
  # The keep the magic alive in Ruby :) Authors of markdown files can use
  # short links without defining their result. For example, you can use
  # [My link][docs.configuration] to link to the `/docs/configuration` page.
  # Auhtors can also use [My link][url.vector_repo] to link to
  # `https://github.com/timberio/vector`. "urls" links are defined in the root
  # `/.meta/links.toml` file and "docs" links are automatically inferred from the
  # documentation's file name. If the file name is ambiguous it can be defined
  # directly in the `/.meta/links.toml` file.
  class LinkChecker
    class << self
      def check!(*args)
        new(*args).check!
      end

      def remove_link_footers(content)
        parts = content.partition(/\n\n\[([a-zA-Z0-9_\-\. ]*)\]:/)
        parts.first.strip
      end
    end

    attr_reader :content, :links, :file_path, :opts, :root_path

    def initialize(content, file_path, links, opts = {})
      @content = self.class.remove_link_footers(content)
      @links = links
      @file_path = file_path
      @opts = opts

      parts = file_path.split("/")
      parts.pop
      parts.pop

      @root_path = parts.collect { |_part| ".." }.join("/")
    end

    def check!
      if !file_path.include?("SUMMARY.md")
        verify_no_direct_links!
      end

      link_names = content.scan(/\]\[([a-zA-Z0-9_\-\. ]*)\]/).flatten.uniq

      footer_links = []

      link_names.each do |link_name|
        value = links.fetch(link_name, current_file: file_path)
        footer_links << "[#{link_name}]: #{value}"
      end

      <<~EOF
      #{content}

      
      #{footer_links.sort.join("\n")}
      EOF
    end

    private
      def verify_no_direct_links!
        direct_links = content.scan(/\]\(([a-zA-Z0-9_\-\. ]*)\)/).flatten.uniq

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