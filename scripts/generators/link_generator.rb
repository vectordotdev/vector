require_relative "generator"

class LinkGenerator < Generator
  attr_reader :content, :links, :root_path

  def initialize(content, root_path, links)
    @content = content
    @links = links
    @root_path = root_path
  end

  def generate
    link_names = content.scan(/\]\[([^\s]*)\]/).flatten.uniq

    links_footer = ""

    link_names.each do |link_name|
      value = begin
        links.send(link_name)
      rescue KeyError
        raise "The link #{link_name.inspect} is not defined, please add it to the [links] table"
      end

      value = if value.start_with?("/")
        "#{root_path}#{value}"
      else
        value
      end 

      links_footer << "[#{link_name}]: #{value.inspect}\n"
    end

    <<~EOF
    #{content}

    #{links_footer}
    EOF
  end
end