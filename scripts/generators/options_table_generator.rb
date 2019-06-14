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

    options.
      select { |option| option.name != "type" }.
      group_by do |option|
        "**#{option.required? ? "REQUIRED" : "OPTIONAL"}** - #{option.category}"
      end.
      each do |title, category_options|
        if @opts[:titles]
          content << "| #{title} | | |\n"
        end

        category_options.each do |option|
          tags = []

          if option.table?
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

            if option.default.nil? && option.enum.nil? && !option.example.nil?
              value = if option.example_key.nil?
                option.example.inspect
              else
                "#{option.example_key} = #{option.example.inspect}"
              end

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

            description = option.description

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