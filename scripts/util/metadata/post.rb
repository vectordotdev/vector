require 'front_matter_parser'

class Post
  include Comparable

  attr_reader :author_github,
    :date,
    :description,
    :id,
    :path,
    :permalink,
    :tags,
    :title

  def initialize(path)
    path_parts = path.split("-", 3)

    @date = Date.parse("#{path_parts.fetch(0)}-#{path_parts.fetch(1)}-#{path_parts.fetch(2)}")
    @path = Pathname.new(path).relative_path_from(ROOT_DIR).to_s

    parsed = FrontMatterParser::Parser.parse_file(path)
    front_matter = parsed.front_matter

    @author_github = front_matter.fetch("author_github")
    @description = parsed.content.split("\n\n").first.remove_markdown_links
    @id = front_matter.fetch("id")
    @permalink = "#{POSTS_BASE_PATH}/#{id}/"
    @tags = front_matter.fetch("tags")
    @title = front_matter.fetch("title")
  end

  def <=>(other)
    date <=> other.date
  end

  def eql?(other)
    self.<=>(other) == 0
  end

  def sink?(name)
    tag?("sink: #{name}")
  end

  def source?(name)
    tag?("source: #{name}")
  end

  def tag?(name)
    tags.any? { |tag| tag == name }
  end

  def transform?(name)
    tag?("transform: #{name}")
  end

  def type?(name)
    tag?("type: announcement")
  end

  def to_h
    {
      author_github: author_github,
      date: date,
      description: description,
      id: id,
      path: path,
      permalink: permalink,
      tags: tags,
      title: title
    }
  end
end
