require 'front_matter_parser'

class Guide
  include Comparable

  attr_reader :author_github,
    :description,
    :domain,
    :id,
    :last_modified_on,
    :path,
    :title

  def initialize(path)
    parsed = FrontMatterParser::Parser.parse_file(path)
    front_matter = parsed.front_matter

    @author_github = front_matter["author_github"]
    @id = path.sub(/\.md$/, "")
    @path = Pathname.new(path).relative_path_from(ROOT_DIR).to_s
    @permalink = "#{BLOG_HOST}/#{@id}"
    @title = front_matter.fetch("title")
  end

  def <=>(other)
    id <=> other.id
  end

  def eql?(other)
    self.<=>(other) == 0
  end
end
