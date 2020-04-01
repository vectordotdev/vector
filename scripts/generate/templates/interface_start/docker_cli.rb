class Templates
  module InterfaceStart
    class DockerCLI
      attr_reader :interface, :requirements

      def initialize(interface, requirements)
        @interface = interface
        @requirements = requirements
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
                flag: "-v $PWD/vector.toml:#{interface.config_path}:ro",
                explanation: "passes your custom configuration to Vector"
              }.to_struct
            ]

          if requirements
            if requirements.docker_api
              hashes << {
                flag: "-v /var/run/docker.sock:/var/run/docker.sock",
                explanation: "ensures that Vector has access to the Docker API"
              }.to_struct
            end

            if requirements.file_system
              hashes << {
                flag: "-v /var/log",
                explanation: "ensures that Vector has access to your app's logging directory, adjust as necessary"
              }.to_struct
            end

            if requirements.network_port
              hashes << {
                flag: "-p #{requirements.network_port}:#{requirements.network_port}",
                explanation: "ensures that port #{requirements.network_port} is exposed for network communication"
              }.to_struct
            end
          end

          hashes
        end
    end
  end
end
