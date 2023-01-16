require_relative "conventional_commit"
require_relative "git_log_commit"

module Vector
  class Commit
    class << self
      def fetch_since(last_version)
        git_log = GitLogCommit.fetch_since!(last_version)
        git_log.collect do |git_log_commit|
          from_git_log_commit(git_log_commit)
        end
      end

      def fetch_since!(last_version)
        git_log = GitLogCommit.fetch_since!(last_version)
        git_log.collect do |git_log_commit|
          from_git_log_commit!(git_log_commit)
        end
      end

      def from_git_log!(git_log)
        git_log.collect do |git_log_commit|
          from_git_log_commit!(git_log_commit)
        end
      end

      private
        def from_git_log_commit(git_log_commit)
          conventional_commit = ConventionalCommit.parse(git_log_commit.message)
          hash = git_log_commit.to_h.merge(conventional_commit.to_h)
          new(hash)
        end

        def from_git_log_commit!(git_log_commit)
          conventional_commit = ConventionalCommit.parse!(git_log_commit.message)
          hash = git_log_commit.to_h.merge(conventional_commit.to_h)
          new(hash)
        end
    end

    attr_reader :author,
      :breaking_change,
      :date,
      :deletions_count,
      :description,
      :files_count,
      :insertions_count,
      :pr_number,
      :scopes,
      :sha,
      :type

    def initialize(hash)
      @author = hash.fetch("author")
      @breaking_change = hash.fetch("breaking_change")
      @date = hash.fetch("date")
      @deletions_count = hash.fetch("deletions_count")
      @description = hash.fetch("description")
      @files_count = hash.fetch("files_count")
      @insertions_count = hash.fetch("insertions_count")
      @pr_number = hash.fetch("pr_number")
      @scopes = hash.fetch("scopes")
      @sha = hash.fetch("sha")
      @type = hash.fetch("type")
    end

    def eql?(other)
      sha == other.sha || pr_number == other.pr_number
    end

    def breaking_change?
      breaking_change == true
    end

    def fix?
      type == "fix"
    end

    def new_feature?
      type == "feat"
    end

    def to_cue_struct
      "{" +
        "sha: #{sha.to_json}, " +
        "date: #{date.to_json}, " +
        "description: #{description.to_json}, " +
        "pr_number: #{pr_number.to_json}, " +
        "scopes: #{scopes.to_json}, " +
        "type: #{type.to_json}, " +
        "breaking_change: #{breaking_change.to_json}, " +
        "author: #{author.to_json}, " +
        "files_count: #{files_count.to_json}, " +
        "insertions_count: #{insertions_count.to_json}, " +
        "deletions_count: #{deletions_count.to_json}}"
    end

    def validate!
      if !type.nil? && !TYPES.include?(type)
        raise <<~EOF
        The following commit has an invalid type!

          #{to_s}

        The type must be one of #{TYPES.inspect}.

          #{type.inspect}

        Please correct in the release /.meta file and retry.
        EOF
      end

      if TYPES_THAT_REQUIRE_SCOPES.include?(type) && scopes.empty?
        raise <<~EOF
        The following commit does not have a scope

          #{to_s}

        A scope is required for commits of type #{TYPES_THAT_REQUIRE_SCOPES.inspect}.

          #{description}

        Please correct in the release /.meta file and retry.
        EOF
      end

      true
    end

    def to_git_log_commit
      message = ""

      if type
        message = "#{message}#{type.clone}"
      end

      if scopes.any?
        message = "#{message}(#{scopes.join(", ")})"
      end

      if breaking_change?
        message = "#{message}!"
      end

      message = "#{message}: #{description}"

      if pr_number
        message = "#{message} (##{pr_number})"
      end

      GitLogCommit.new({
        "author" => author,
        "date" => date,
        "deletions_count" => deletions_count,
        "files_count" => files_count,
        "insertions_count" => insertions_count,
        "message" => message,
        "sha" => sha
      })
    end

    def to_s
      "#{sha} #{type}(#{scopes.join(", ")})#{breaking_change? ? "!" : ""}: #{description} (##{pr_number})"
    end
  end
end
