require 'front_matter_parser'

class Highlight
  include Comparable

  attr_reader :author_github,
    :date,
    :description,
    :hide_on_release_notes,
    :id,
    :path,
    :permalink,
    :pr_numbers,
    :release,
    :tags,
    :title

  def initialize(path)
    path_parts = File.basename(path).split("-", 4)

    @date = Date.parse("#{path_parts.fetch(0)}-#{path_parts.fetch(1)}-#{path_parts.fetch(2)}")
    @path = Pathname.new(path).relative_path_from(ROOT_DIR).to_s

    parsed = FrontMatterParser::Parser.parse_file(path)
    front_matter = parsed.front_matter

    @author_github = front_matter.fetch("author_github")
    @description = front_matter.fetch("description")
    @hide_on_release_notes = front_matter.fetch("hide_on_release_notes")
    @id = front_matter["id"] || @path.split("/").last.gsub(/\.md$/, '')
    @permalink = "#{HIGHLIGHTS_BASE_PATH}/#{id}/"
    @pr_numbers = front_matter.fetch("pr_numbers")
    @release = front_matter.fetch("release")
    @tags = front_matter.fetch("tags")
    @title = front_matter.fetch("title")

    # Requirements

    if breaking_change? && !File.read(path).include?("## Upgrade Guide")
      raise Exception.new(
        <<~EOF
        The following "breaking change" highlight does not have an "Upgrade
        Guide" section:

            #{path}

        This is required for all "breaking change" highlights to ensure
        we provide a good, consistent UX for upgrading users. To fix this,
        please add a "Upgrade Guide" section:

            ## Upgrade Guide

            Make the following changes in your `vector.toml` file:

            ```diff title="vector.toml"
             [sinks.example]
               type = "example"
            -  remove = "me"
            +  add = "me"
            ```

            That's it!

        EOF
      )
    end
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

  def hide_on_release_notes?
    @hide_on_release_notes == true
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
      hide_on_release_notes: hide_on_release_notes,
      id: id,
      path: path,
      permalink: permalink,
      tags: tags,
      title: title
    }
  end
end
