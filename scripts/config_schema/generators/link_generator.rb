require_relative "generator"

class LinkGenerator < Generator
  attr_reader :content, :links, :file_path, :root_path

  def initialize(content, file_path, links)
    @content = content
    @links = links
    @file_path = file_path

    parts = file_path.split("/")
    parts.pop
    parts.pop

    @root_path = parts.collect { |_part| ".." }.join("/")
  end

  def generate
    parts = content.partition(/\[([a-zA-Z0-9_\. ]*)\]:/)
    content = parts.first.strip

    direct_links = content.scan(/\]\(([a-zA-Z0-9_\. ]*)\)/).flatten.uniq

    if direct_links.any?
      raise <<~EOF
      You used a direct link in your the #{file_path.inspect} file:

      #{direct_links.first.inspect}

      This is not allowed in the Vector documentation for validation purposes.
      Please:

      1. Update your links to use a short link.
      2. Add the short link to the metadata.toml file.
      EOF
    end

    link_names = content.scan(/\]\[([a-zA-Z0-9_\. ]*)\]/).flatten.uniq
    footer_links = []

    link_names.each do |link_name|
      value = links.fetch(link_name)

      # We adjust the path accordingly.
      # 1. If the path starts with "http" then we can leave it alone.
      # 2. If the path starts with "docs/" we know it's an internal link,
      #    and we must update the path to be relative to the current document.
      #    This ensures that the link works both on Github as well as Gitbook.
      value =
        if value.start_with?("http")
          value
        else
          if file_path.start_with?("docs/")
            "#{root_path}#{value}"
          else
            path = value.gsub(/^docs\//, "/").gsub(".md", "").gsub("/README.md", "")
            "https://docs.vector.dev#{path}"
          end
        end 

      footer_links << "[#{link_name}]: #{value}"
    end

    <<~EOF
    #{content}


    #{footer_links.sort.join("\n")}
    EOF
  end

end