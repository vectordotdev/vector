require 'front_matter_parser'

class Highlight
  include Comparable

  attr_reader :author_github,
    :date,
    :description,
    :id,
    :importance,
    :path,
    :permalink,
    :pr_numbers,
    :release,
    :tags,
    :title

  def initialize(path)
    path_parts = path.split("-", 4)

    @date = Date.parse("#{path_parts.fetch(0)}-#{path_parts.fetch(1)}-#{path_parts.fetch(2)}")
    @path = Pathname.new(path).relative_path_from(ROOT_DIR).to_s

    parsed = FrontMatterParser::Parser.parse_file(path)
    front_matter = parsed.front_matter

    @author_github = front_matter.fetch("author_github")
    @description = front_matter.fetch("description")
    @id = front_matter["id"] || @path.split("/").last.gsub(/\.md$/, '')
    @importance = front_matter.fetch("importance")
    @permalink = "#{HIGHLIGHTS_BASE_PATH}/#{id}/"
    @pr_numbers = front_matter.fetch("pr_numbers")
    @release = front_matter.fetch("release")
    @tags = front_matter.fetch("tags")
    @title = front_matter.fetch("title")
  end

  def <=>(other)
    date <=> other.date
  end

  def breaking_change?
    type?("breaking change")
  end

  def eql?(other)
    self.<=>(other) == 0
  end

  def importance?(name)
    importance == name
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

  def type
    @type ||=
      begin
        type_tag = tags.find { |tag| tag.start_with?("type: ") }
        type_tag.gsub(/^type: /, '')
      end
  end

  def type?(name)
    tag?("type: #{name}")
  end

  def to_h
    {
      author_github: author_github,
      date: date,
      description: description,
      id: id,
      importance: importance,
      path: path,
      permalink: permalink,
      tags: tags,
      title: title
    }
  end
end
