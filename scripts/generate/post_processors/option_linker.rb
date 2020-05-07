#encoding: utf-8

module PostProcessors
  # Turns references to options into links.
  #
  # While we should do our best to link to options we are not always consistent
  # with it. This processor ensure that we are.
  class OptionLinker
    class << self
      def link!(content)
        content.scan(/ `([a-zA-Z][a-zA-Z_.\*]*)`/).collect do |matches|
          option = matches.first
          section_name = option.end_with?(".*") ? option.sub(/\.\*$/, '') : option

          if content.include?("## #{section_name}")
            content.gsub!(/ `#{option}`/, " [`#{option}`](##{section_name.slugify})")
          end
        end

        content
      end
    end
  end
end
