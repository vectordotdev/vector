module Vector
  class ConventionalCommit
    class << self
      def parse(message)
        hash = parse_commit_message(message)
        new(hash)
      end

      def parse!(message)
        hash = parse_commit_message!(message)
        new(hash)
      end

      private
        def parse_commit_message(message)
          begin
            parse_commit_message!(message)
          rescue Exception => e
            if message.include?("Use `namespace` field in metric sources")
              raise e
            end

            {
              "breaking_change" => nil,
              "description" => message,
              "pr_number" => nil,
              "scopes" => [],
              "type" => nil
            }
          end
        end

        def parse_commit_message!(message)
          match = message.match(/^(?<type>[a-z]*)(\((?<scope>[a-z0-9_, ]*)\))?(?<breaking_change>!)?: (?<description>.*?)( \(#(?<pr_number>[0-9]*)\))?$/)

          if match.nil?
            raise <<~EOF
            Commit message does not conform to the conventional commit format.

            Unable to parse at all!

              #{message}

            Please correct in the release /.meta file and retry.
            EOF
          end

          attributes =
            {
              "type" => match[:type],
              "breaking_change" => !match[:breaking_change].nil?,
              "description" => match[:description]
            }

          attributes["scopes"] =
            if match[:scope]
              match[:scope].split(",").collect(&:strip)
            else
              []
            end

          attributes["pr_number"] =
            if match[:pr_number]
              match[:pr_number].to_i
            else
              nil
            end

          attributes
        end
    end

    attr_reader :breaking_change,
      :description,
      :pr_number,
      :type,
      :scopes

    def initialize(hash)
      @breaking_change = hash.fetch("breaking_change")
      @description = hash.fetch("description")
      @pr_number = hash.fetch("pr_number")
      @type = hash.fetch("type")
      @scopes = hash.fetch("scopes")
    end

    def to_h
      {
          "breaking_change" => breaking_change,
          "description" => description,
          "pr_number" => pr_number,
          "type" => type,
          "scopes" => scopes
      }
    end
  end
end
