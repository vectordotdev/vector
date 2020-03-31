module PostProcessors
  # Validates front matter
  #
  # If front matter contains a $schema attribute thenit will be validated here.
  class FrontMatterValidator
    class << self
      def validate!(content, target_path)
        loader = FrontMatterParser::Loader::Yaml.new
        parser = FrontMatterParser::Parser.new(:md, loader: loader)
        parsed = parser.call(content)
        front_matter  = parsed.front_matter
        errors = front_matter.validate_schema

        if  errors.any?
          schema = front_matter.fetch("$schema")

          Printer.error!(
            <<~EOF
            The front matter in the the following file:

                #{target_path.sub(/^${ROOT_DIR}/, "")}

            Fail validation against the following schema:

                #{schema}

            The errors include:

                * #{errors[0..50].join("\n    * ")}

            The front matter is:

                #{JSON.pretty_generate(front_matter)}

            Please fix these errors and try again.
            EOF
          )
        end
      rescue Exception => e
        Printer.error!(
          <<~EOF
          Unable to parse front matter for:

              #{target_path}

          Error:

            #{e.message}

          Stacktrace:

            #{e.backtrace.join("\n  ")}

          EOF
        )
      end
    end
  end
end
