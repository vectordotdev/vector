require 'front_matter_parser'

class Post
  include Comparable

  attr_reader :author,
    :date,
    :id,
    :path,
    :permalink,
    :tags,
    :title

  def initialize(path)
    path_parts = path.split("-", 3)

    @date = Date.parse("#{path_parts.fetch(0)}-#{path_parts.fetch(1)}-#{path_parts.fetch(2)}")
    @path = Pathname.new(path).relative_path_from(ROOT_DIR).to_s

    front_matter = FrontMatterParser::Parser.parse_file(path).front_matter

    @author = front_matter.fetch("author")
    @id = front_matter.fetch("id")
    @permalink = "#{BLOG_HOST}/#{id}"
    @tags = front_matter.fetch("tags")
    @title = front_matter.fetch("title")
  end

  def <=>(other)
    date <=> other.date
  end

  def eql?(other)
    self.<=>(other) == 0
  end

  def to_h
    {
      author: author,
      date: date,
      id: id,
      path: path,
      permalink: permalink,
      tags: tags,
      title: title
    }
  end
end