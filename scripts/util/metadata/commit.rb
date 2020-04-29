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
    :insertions_count,
    :message,
    :pr_number,
    :scopes,
    :sha,
    :type

  attr_accessor :highlight_permalink

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
    @scopes = (message_attributes["scopes"] || []).collect { |s| CommitScope.new(s) }
    @type = message_attributes.fetch("type")
  end

  def breaking_change?
    @breaking_change == true
  end

  def bug_fix?
    type == "fix"
  end

  def chore?
    type == "chore"
  end

  def components
    return @components if defined?(@components)

    @components =
      if new_feature?
        match =  description.match(/`?(?<name>[a-zA-Z_]*)`? (?<type>source|transform|sink)/i)

        if !match.nil?
          [
            {name: match.fetch(:name).downcase, type: match.fetch(:type).downcase}.to_struct
          ]
        else
          []
        end
      else
        scopes.collect(&:component)
      end
  end

  def doc_update?
    type == "docs"
  end

  def enhancement?
    type == "enhancement"
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
      highlight_permalink: highlight_permalink,
      insertions_count: insertions_count,
      message: message,
      pr_number: pr_number,
      scopes: scopes.deep_to_h,
      sha: sha,
      type: type,
    }
  end

  def transform?
    component_type == "transform"
  end

  private
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

      if match[:scope]
        attributes["scopes"] = match[:scope].split(",").collect(&:strip)
      end

      if match[:pr_number]
        attributes["pr_number"] = match[:pr_number].to_i
      end

      type = attributes.fetch("type")
      scopes = attributes["scopes"]

      if !type.nil? && !TYPES.include?(type)
        raise <<~EOF
        Commit has an invalid type!
        The type must be one of #{TYPES.inspect}.

          #{type.inspect}

        Please correct in the release /.meta file and retry.
        EOF
      end

      if TYPES_THAT_REQUIRE_SCOPES.include?(type) && scopes.empty?
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
