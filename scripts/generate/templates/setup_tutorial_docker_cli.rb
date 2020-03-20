class Templates
  class SetupTutorialDockerCLI
    attr_reader :source

    def initialize(source)
      if source.nil?
        raise ArgumentError.new("source is required")
      end

      @source = source
    end

    def start_command_flags
      flags.collect(&:flag)
    end

    def start_explanation
      flags.collect do |flag|
        "The `#{flag.flag.truncate(5)}` flag #{flag.explanation}."
      end
    end

    private
      def flags
        hashes = []

        if source.requirements.docker_api
          hashes << {
            flag: "-v /var/run/docker.sock:/var/run/docker.sock",
            explanation: "ensures that Vector has access to the Docker API"
          }.to_struct
        end

        hashes
      end
  end
end
