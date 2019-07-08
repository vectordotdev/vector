#encoding: utf-8

module PostProcessors
  # Sorts sections with a [[sort]] directive.
  #
  # There are times within the documentation where sections are out of order
  # due to the use of partials. For example, the `aws_s3` sink includes a
  # partial for common component sections but also defines it's own custom
  # sections below. This results in sections that are not alphabetically
  # orders, which is nice for the user since some pages can contain many
  # sections. This class automatically parses and sorts the sections to solve
  # this.
  class SectionSorter
    class << self
      def sort!(*args)
        new(*args).sort!
      end
    end

    attr_reader :content

    def initialize(content)
      @content = content
    end

    def sort!
      new_content = content.clone

      parts.each do |part|
        sorted_content = part[:content].split(/\n#{part[:depth]}# /).sort.join("\n#{part[:depth]}# ")
        new_content.gsub!(part[:content], sorted_content)
      end

      new_content.gsub!(/ \[\[sort\]\]/, "")

      new_content
    end

    private
      def parts
        @parts ||=
          content.scan(/\n(#*) (.*) (\[\[sort\]\])/).collect do |matches|
            depth = matches.fetch(0)
            title = matches.fetch(1)
            sort_flag = matches.fetch(2)
            
            part = 
              content.
                split("\n#{depth} #{title} #{sort_flag}").
                last.
                split("\n#{depth} ").
                first

            {title: title, depth: depth, content: part}
          end
      end
  end
end