#encoding: utf-8

module PostProcessors
  # Turns references to options into links.
  #
  # While we should do our best to link to options we are not always consistent
  # with it. This processor ensure that we are.
  class OptionLinker
    class << self
      def link!(content)
        content.scan(/[^\[]`([a-z][a-z_.]*)`/).collect do |matches|
          option = matches.first

          if content.include?("## #{option}")
            content.gsub!(/[^\[]`#{option}`/, "[`#{option}`](##{option.parameterize})")
          end
        end

        content
      end
    end
  end
end