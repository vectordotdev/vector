class Templates
  module InterfaceTutorials
    class DockerCLI
      attr_reader :config_path, :source

      def initialize(source)
        if source.nil?
          raise ArgumentError.new("source is required")
        end

        @config_path = "vector.toml"
        @source = source
      end

      def start_command_lines
        flags.collect(&:flag) + ["timberio/vector:latest-alpine"]
      end

      def start_command_explanations
        explanations =
          flags.collect do |flag|
            "The `#{flag.flag.truncate(20)}` flag #{flag.explanation}."
          end

        explanations << "The `timberio/vector:latest-alpine` is the default image we've chosen, you are welcome to use [other image variants][docs.platforms.docker#variants]."
        explanations
      end

      private
        def flags
          hashes =
            [
              {
                flag: "-v $PWD/#{config_path}:/etc/vector/vector.toml:ro",
                explanation: "passes your custom configuration to Vector"
              }.to_struct
            ]


          if source.requirements.docker_api
            hashes << {
              flag: "-v /var/run/docker.sock:/var/run/docker.sock",
              explanation: "ensures that Vector has access to the Docker API"
            }.to_struct
          end

          if source.requirements.file_system
            hashes << {
              flag: "-v /var/log",
              explanation: "ensures that Vector has access to your app's logging directory, adjust as necessary"
            }.to_struct
          end

          if source.requirements.network_port
            hashes << {
              flag: "-p #{source.requirements.network_port}:#{source.requirements.network_port}",
              explanation: "ensures that port #{source.requirements.network_port} is exposed for network communication"
            }.to_struct
          end

          hashes
        end
    end
  end
end
