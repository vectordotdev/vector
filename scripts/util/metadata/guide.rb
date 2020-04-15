require 'front_matter_parser'

class Guide
  include Comparable

  attr_reader :author_github,
    :description,
    :id,
    :last_modified_on,
    :path,
    :series_position,
    :title

  def initialize(path)
    parsed = FrontMatterParser::Parser.parse_file(path)
    front_matter = parsed.front_matter

    @author_github = front_matter["author_github"]
    @id = path.sub(GUIDES_ROOT, '').sub(/\.md$/, "")
    @path = Pathname.new(path).relative_path_from(ROOT_DIR).to_s
    @permalink = "#{GUIDES_BASE_PATH}/#{@id}/"
    @title = front_matter.fetch("title")
  end

  def <=>(other)
    id <=> other.id
  end

  def eql?(other)
    self.<=>(other) == 0
  end

  def to_h
    {
      author_github: author_github,
      description: description,
      id: id,
      last_modified_on: last_modified_on,
      path: path,
      series_position: series_position,
      title: title
    }
  end
end
