require "json"

require "active_support/core_ext/string/filters"

require_relative "commit_scope"

class Commit
  TYPES = ["chore", "docs", "enhancement", "feat", "fix", "perf"].freeze
  TYPES_THAT_REQUIRE_SCOPES = ["enhancement", "feat", "fix"].freeze

  attr_reader :author,
    :breaking_change,
    :date,
    :deletions_count,
    :description,
    :files_count,
    :group,
    :insertions_count,
    :message,
    :pr_number,
    :scope,
    :sha,
    :type

  def initialize(attributes)
    @author = attributes.fetch("author")
    @deletions_count = attributes["deletions_count"] || 0
    @files_count = attributes.fetch("files_count")
    @date = attributes.fetch("date")
    @insertions_count = attributes["insertions_count"] || 0
    @message = attributes.fetch("message")
    @sha = attributes.fetch("sha")

    message_attributes = parse_commit_message!(@message)
    @breaking_change = message_attributes.fetch("breaking_change")
    @description = message_attributes.fetch("description")
    @pr_number = message_attributes["pr_number"]
    @scope = CommitScope.new(message_attributes["scope"] || "core")
    @type = message_attributes.fetch("type")
    @group = @breaking_change ? "breaking change" : @type
  end

  def breaking_change?
    @breaking_change == true
  end

  def bug_fix?
    type == "fix"
  end

  def category
    scope.category
  end

  def chore?
    type == "chore"
  end

  def component?
    !component_name.nil? && !component_type.nil?
  end

  def component_name
    return @component_name if defined?(@component_name)

    @component_name =
      if new_feature?
        match =  description.match(/`?(?<name>[a-zA-Z_]*)`? (source|transform|sink)/)

        if !match.nil? && !match[:name].nil?
          match[:name].downcase
        else
          nil
        end
      else
        scope.component_name
      end
  end

  def component_name!
    if component_name.nil?
      raise "Component name could not be found in commit: #{message}"
    end

    component_name
  end

  def component_type
    scope.component_type
  end

  def component_type!
    if component_type.nil?
      raise "Component type could not be found in commit: #{message}"
    end

    component_type
  end

  def doc_update?
    type == "docs"
  end

  def enhancement?
    type == "enhancement"
  end

  def new_component?
    new_feature? && !component_name.nil?
  end

  def new_feature?
    type == "feat"
  end

  def performance_improvement?
    type == "perf"
  end

  def sha_short
    @sha_short ||= sha.truncate(7, omission: "")
  end

  def sink?
    component_type == "sink"
  end

  def source?
    component_type == "source"
  end

  def to_h
    {
      author: author,
      breaking_change: breaking_change,
      date: date,
      deletions_count: deletions_count,
      description: description,
      files_count: files_count,
      group: group,
      insertions_count: insertions_count,
      message: message,
      pr_number: pr_number,
      scope: scope.deep_to_h,
      sha: sha,
      type: type,
    }
  end

  def transform?
    component_type == "transform"
  end

  private
    def parse_commit_message!(message)
      match = message.match(/^(?<type>[a-z]*)(\((?<scope>[a-z0-9_ ]*)\))?(?<breaking_change>!)?: (?<description>.*?)( \(#(?<pr_number>[0-9]*)\))?$/)

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

      if match[:scope]
        attributes["scope"] = match[:scope]
      end

      if match[:pr_number]
        attributes["pr_number"] = match[:pr_number].to_i
      end

      type = attributes.fetch("type")
      scope = attributes["scope"]

      if !type.nil? && !TYPES.include?(type)
        raise <<~EOF
        Commit has an invalid type!
        The type must be one of #{TYPES.inspect}.

          #{type.inspect}

        Please correct in the release /.meta file and retry.
        EOF
      end

      if TYPES_THAT_REQUIRE_SCOPES.include?(type) && scope.nil?
        raise <<~EOF
        Commit does not have a scope!

        A scope is required for commits of type #{TYPES_THAT_REQUIRE_SCOPES.inspect}.

          #{description}

        Please correct in the release /.meta file and retry.
        EOF
      end

      attributes
    end
end
