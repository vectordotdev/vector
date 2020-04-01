require_relative "interface_start/docker_cli"

class Templates
  module InterfaceStart
    def load(interface, requirement: nil)
      case interface.name
      when "docker-cli"
        DockerCLI.new(source)
      when "docker-compose"
        DockerCLI.new(source)
      end
    end
  end
end
