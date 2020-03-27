require_relative "guide"

class Guides
  attr_reader :children, :description, :guides, :name, :series, :title

  def initialize(hash)
    @children = (hash["children"] || {}).to_struct_with_name(constructor: self.class)
    @description = hash.fetch("description")
    @name = hash.fetch("name")
    @series = hash.fetch("series")
    @title = hash.fetch("title")

    @guides ||=
      Dir.
        glob("#{GUIDES_ROOT}/#{@name}/**/*.md").
        filter do |path|
          content = File.read(path)
          content.start_with?("---\n")
        end.
        collect do |path|
          Guide.new(path)
        end.
        sort_by { |guide| [ guide.series_position, guide.title ] }
  end

  def to_h
    {
      children: children.deep_to_h,
      description: description,
      guides: guides.deep_to_h,
      name: name,
      series: series,
      title: title
    }
  end
end
