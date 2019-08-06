#encoding: utf-8

module PostProcessors
  # Adds section references to option descriptions.
  #
  # When documenting options we'll list them in a table with their name, type,
  # and description. Below we'll expand on behavior in a "How It Works" section.
  # In many of the sections we'll reference options that dictate that behavior.
  # Within the options table above it helps to add text like "See the XXX
  # section for more info". This processor adds that text automatically.
  class OptionReferencer
    class << self
      def reference!(content)
        content.scan(/\[\[references:(.*)\]\]/).collect do |matches|
          option = matches.first
          how_it_works = content.split("\n## How It Works").last.split("\n## ").first

          sections_with_references = how_it_works.split("`#{option}`")[0..-2]
          titles =
            sections_with_references.collect do |section|
              match = section.scan(/\n### (.*)\n/).last
              if match.nil?
                nil
              else
                match.first
              end
            end.compact.uniq

          if titles.any?
            links = titles.collect { |title| "[#{title}](##{title.parameterize})" }
            content.sub!("[[references:#{option}]]", " See #{links.to_sentence} for more info.")
          else
            content.sub!("[[references:#{option}]]", "")
          end
        end

        content
      end
    end
  end
end