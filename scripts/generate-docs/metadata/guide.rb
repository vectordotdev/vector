#encoding: utf-8

class Guide
  attr_reader :file_path,
    :sinks,
    :sources,
    :title,
    :transforms

  def initialize(file_path)
    parsed = FrontMatterParser::Parser.parse_file(file_path)
    front_matter = parsed.front_matter

    h1s = parsed.content.scan(/# (.*)\n/).flatten

    @file_path = file_path.gsub(/^docs/, "")
    @sinks = (front_matter["sinks"] || "").split(",").collect(&:strip)
    @sources = (front_matter["sources"] || "").split(",").collect(&:strip)
    @title = h1s.fetch(0)
    @transforms = (front_matter["transforms"] || "").split(",").collect(&:strip)
  end
end