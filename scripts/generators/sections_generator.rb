require_relative "generator"

class SectionsGenerator < Generator
  attr_reader :sections

  def initialize(sections)
    @sections = sections
  end

  def generate
    content = ""

    sections.each do |section|
      section_content = <<~EOF
      ### #{section.title}

      #{section.body}

      EOF

      section_content.strip!

      issue_links = section.issues.collect do |issue_num|
        "[Issue ##{issue_num}][issue_#{issue_num}]"
      end

      if issue_links.length > 0
        section_content << " See #{issue_links.to_sentence} for more info."
      end

      section_content << "\n\n"

      content << section_content
    end

    content.strip
  end
end