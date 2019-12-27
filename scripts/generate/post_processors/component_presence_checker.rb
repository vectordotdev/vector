module PostProcessors
  # Checks that all components defined in the /.meta directory are
  # present in the docs folder and that there are no superflous documents.
  class ComponentPresenceChecker
    class << self
      def check!(*args)
        new(*args).check!
      end
    end

    def initialize(type, docs, components)
      @type = type
      @doc_names = docs.collect { |s| File.basename(s).gsub(/\.md$/, "") } - ["README"]
      @component_names = components.to_h.keys.collect(&:to_s)
    end

    def check!
      if (missing = @component_names - @doc_names).any?
        raise <<~EOF
        The following #{@type} do not have documentation files.
        Please add them to:

        /scripts/generate/templates/docs/usage/configuration/#{@type}/*

        * #{missing.join("\n* ")}
        EOF
      end

      if (extra = @doc_names - @component_names).any?
        raise <<~EOF
        The following #{@type} have documentation files but are not
        defined in the /.meta directory. Please remove them from

        /docs/usage/configuration/#{@type}/*

        * #{extra.join("\n* ")}
        EOF
      end
    end
  end
end
