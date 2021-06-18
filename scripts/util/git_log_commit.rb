module Vector
  class GitLogCommit
    class << self
      def fetch_since!(last_version)
        range = "v#{last_version}..."
        commit_log = `git log #{range} --cherry-pick --right-only --no-merges --pretty=format:'%H\t%s\t%aN\t%ad'`.chomp
        commit_lines = commit_log.split("\n").reverse

        commit_lines.collect do |commit_line|
          hash = parse_commit_line!(commit_line)
          new(hash)
        end
      end

      def from_file!(path)
        contents = File.read(path)
        contents.split("\n").collect do |line|
          hash = parse_commit_line!(line)
          new(hash)
        end
      end

      private
        # This is used for the `files_count`, `insertions_count`, and `deletions_count`
        # attributes. It helps to communicate stats and the depth of changes in our
        # release notes.
        def get_commit_stats(sha)
          `git show --shortstat --oneline #{sha}`.split("\n").last
        end

        def parse_commit_line!(commit_line)
          # Parse the full commit line
          line_parts = commit_line.split("\t")
          sha = line_parts.fetch(0)
          message = line_parts.fetch(1)
          author = line_parts.fetch(2)
          date = Time.parse(line_parts.fetch(3)).utc

          attributes =
            {
              "sha" =>  sha,
              "author" => author,
              "date" => date,
              "message" => message
            }

          # Parse the stats
          stats = get_commit_stats(attributes.fetch("sha"))
          if /^\W*\p{Digit}+ files? changed,/.match(stats)
            stats_attributes = parse_commit_stats!(stats)
            attributes.merge!(stats_attributes)
          end

          attributes
        end

        # Parses the data from `#get_commit_stats`.
        def parse_commit_stats!(stats)
          attributes = {}

          stats.split(", ").each do |stats_part|
            stats_part.strip!

            key =
              case stats_part
              when /insertions?/
                "insertions_count"
              when /deletions?/
                "deletions_count"
              when /files? changed/
                "files_count"
              else
                raise "Invalid commit stat: #{stats_part}"
              end

            count = stats_part.match(/^(?<count>[0-9]*) /)[:count].to_i
            attributes[key] = count
          end

          attributes["insertions_count"] ||= 0
          attributes["deletions_count"] ||= 0
          attributes["files_count"] ||= 0
          attributes
        end
    end

    attr_reader :author,
      :date,
      :deletions_count,
      :files_count,
      :insertions_count,
      :message,
      :raw,
      :sha

    def initialize(hash)
      @author = hash.fetch("author")
      @date = hash.fetch("date")
      @deletions_count = hash.fetch("deletions_count", 0)
      @files_count = hash.fetch("files_count", 0)
      @insertions_count = hash.fetch("insertions_count", 0)
      @message = hash.fetch("message")
      @sha = hash.fetch("sha")
    end

    def to_h
      {
        "author" => author,
        "date" => date,
        "deletions_count" => deletions_count,
        "files_count" => files_count,
        "insertions_count" => insertions_count,
        "message" => message,
        "sha" => sha
      }
    end

    def to_raw
      "#{sha}\t#{message}\t#{author}\t#{date.strftime("%a %b %d %H:%M:%S %Y %z")}"
    end
  end
end
