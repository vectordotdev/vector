require_relative "setup_tutorial/docker_cli_setup_tutorial"

class Templates
  class SetupTutorial
    class Base
      attr_reader :platform, :sink, :source, :strategy

      def initialize(platform: platform, sink: sink, source: source, strategy: strategy)
        @platform = platform
        @sink = sink
        @source = source
        @strategy = strategy
      end
    end

    class << self
      def build(platform: platform, sink: sink, source: source, strategy: strategy)
        case platform.name
        when "docker"
          DockerCLISetupTutorial.new(platform: platform, sink: sink, source: source, strategy: strategy)
        else
          raise NotImplementedError.new(
            "The #{platform.name} platform does not have a setup tutorial implemented"
          )
        end
      end
    end
  end
end
