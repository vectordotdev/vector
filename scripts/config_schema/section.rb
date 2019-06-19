require_relative "field"
require_relative "section"

class Section
  include Comparable

  attr_reader :body,
    :issues,
    :referenced_options,
    :slug,
    :title

  def initialize(hash)
    @body = hash.fetch("body")
    @issues = hash["issues"] || []
    @title = hash.fetch("title")
    @slug = @title.parameterize
    @referenced_options = @body.scan(/`([a-zA-Z0-9_.]*)`/).flatten
  end

  def <=>(other)
    title <=> other.title
  end
end