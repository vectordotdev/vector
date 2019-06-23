require_relative "generator"

class OptionsTableGenerator < Generator
  attr_reader :options, :sections

  def initialize(options, sections, opts = {})
    @options = options
    @sections = sections
    @opts = opts
    @opts[:header] = true if !@opts.key?(:header)
    @opts[:titles] = true if !@opts.key?(:titles)
  end

  def generate
    content = ""

    if @opts[:header]
      content << <<~EOF
        | Key  | Type  | Description |
        | :--- | :---: | :---------- |
      EOF
    end

    categories = options.collect(&:category).uniq

    grouped_options =
      options.
        select { |option| option.name != "type" || !option.enum.nil? }.
        group_by do |option|
          title = "**#{option.required? ? "REQUIRED" : "OPTIONAL"}**"

          if categories.length > 1
           "#{title} - #{option.category}"
          else
            title
          end
        end

    if grouped_options.keys.length <= 1
      @opts[:titles] = false
    end

    grouped_options.each do |title, category_options|
        if @opts[:titles]
          content << "| #{title} | | |\n"
        end

        category_options.each do |option|
          tags = []

          if option.table? && !option.options.nil? && option.options.length > 0
            sub_generator = self.class.new(
              option.options,
              sections,
              header: false,
              path: option.name,
              titles: false
            )
            content << (sub_generator.generate + "\n")
          else
            if option.required?
              tags << "`required`"
            end

            if !option.default.nil?
              tags << "`default: #{option.default.inspect}`"
            elsif option.optional?
              tags << "`no default`"
            end

            if option.default.nil? && option.enum.nil? && option.examples.any?
              value = option.examples.first.inspect

              if value.length > 30
                tags << "`example: (see above)`"
              else
                tags << "`example: #{value}`"
              end
            end

            if option.enum
              tags << "`enum: #{option.enum.collect(&:inspect).join(", ")}`"
            end

            if !option.unit.nil?
              tags << "`unit: #{option.unit}`"
            end

            description = option.description.clone

            section_links =
              option.get_relevant_sections(sections).collect do |section|
                "[#{section.title}](##{section.slug})" 
              end

            if section_links.length > 0
              description << " See #{section_links.to_sentence} for more info."
            end

            if tags.any?
              description << "<br />#{tags.join(" ")}"
            end

            name = [@opts[:path], option.name].compact.join(".")

            content << "| `#{name}` | `#{option.type}` | #{description} |\n"
          end
        end
      end

    content.strip
  end
end