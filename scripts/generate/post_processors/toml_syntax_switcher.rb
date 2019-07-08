#encoding: utf-8

module PostProcessors
  # Switch the `toml` syntax to `coffeescript`
  #
  # This is necessary since Gitbook does not offer TOML syntax highlighting.
  # Coffee script is the closest we've found.
  class TOMLSyntaxSwitcher
    class << self
      def switch!(content)
        content.gsub(/```toml/, "```coffeescript")
      end
    end
  end
end