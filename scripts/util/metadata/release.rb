require_relative "commit"
require_relative "../../util/version"

class Release
  include Comparable

  attr_reader :codename,
    :commits,
    :description,
    :date,
    :highlights,
    :last_version,
    :permalink,
    :version,
    :whats_next

  def initialize(release_hash, last_version, all_highlights)
    @codename = release_hash["codename"] || ""
    @description = release_hash["description"] || ""
    @date = release_hash.fetch("date").to_date
    @last_version = last_version
    @version = Version.new(release_hash.fetch("version"))
    @permalink = "#{RELEASES_BASE_PATH}/#{@version}/"
    @whats_next = (release_hash["whats_next"] || []).collect(&:to_struct)

    # highlights

    @highlights =
      all_highlights.select do |h|
        h.release == version.to_s
      end

    # commits

    @commits =
      release_hash.fetch("commits").collect do |commit_hash|
        commit = Commit.new(commit_hash)
        highlight = @highlights.find { |h| h.pr_numbers.include?(commit.pr_number) }
        commit.highlight_permalink = highlight ? highlight.permalink : nil
        commit
      end

    # requirements

    @commits.each do |commit|
      if commit.breaking_change?
        if !@highlights.any? { |h| h.type?("breaking change") && h.pr_numbers.include?(commit.pr_number) }
          tags = ["type: breaking change"]

          commit.scopes.each do |scope|
            if scope.component
              tags << "domain: #{scope.component.type.pluralize}"
              tags << "#{scope.component.type}: #{scope.component.name}"
            end
          end

          raise ArgumentError.new(
            <<~EOF
            Release #{@version} contains breaking commits without an upgrade guide!

              * Commiit #{commit.sha_short} - #{commit.description}

            Please add the following breaking change at:

              website/highlights/#{commit.date.to_date.to_s}-#{commit.description.parameterize}.md

            With the following content:

            ---
            $schema: "/.meta/.schemas/highlights.json"
            title: "#{commit.description}"
            description: "<fill-in>"
            author_github: "https://github.com/binarylogic"
            hide_on_release_notes: false
            pr_numbers: [#{commit.pr_number}]
            release: "#{@version}"
            tags: #{tags.to_json}
            ---

            Explain the change and the reasoning here.

            ## Upgrade Guide

            Make the following changes in your `vector.toml` file:

            ```diff title="vector.toml"
             [sinks.example]
               type = "example"
            -  remove = "me"
            +  add = "me"
            ```

            That's it!

            EOF
          )
        end
      end
    end
  end

  def <=>(other)
    if other.is_a?(self.class)
      version <=> other.version
    else
      nil
    end
  end

  def authors
    @authors ||= commits.collect(&:author).uniq.sort
  end

  def breaking_changes
    @breaking_changes ||= commits.select(&:breaking_change?)
  end

  def bug_fixes
    @bug_fixes ||= commits.select(&:bug_fix?)
  end

  def compare_short_link
    @compare_short_link ||= "urls.compare_v#{last_version}...v#{version}"
  end

  def compare_url
    @compare_url ||= "https://github.com/timberio/vector/compare/v#{last_version}...v#{version}"
  end

  def deletions_count
    @deletions_count ||= countable_commits.sum(&:deletions_count)
  end

  def doc_updates
    @doc_updates ||= commits.select(&:doc_update?)
  end

  def enhancements
    @enhancements ||= commits.select(&:enhancement?)
  end

  def eql?(other)
    self.<=>(other) == 0
  end

  def files_count
    @files_count ||= countable_commits.sum(&:files_count)
  end

  def hash
    version.hash
  end

  def human_date
    date.strftime("%b %-d, %Y")
  end

  def insertions_count
    @insertions_count ||= countable_commits.sum(&:insertions_count)
  end

  def new_features
    @new_features ||= commits.select(&:new_feature?)
  end

  def major?
    type == "major"
  end

  def minor?
    type == "minor"
  end

  def patch?
    type == "patch"
  end

  def performance_improvements
    @performance_improvements ||= commits.select(&:performance_improvement?)
  end

  def pre?
    type == "pre"
  end

  def title
    @title ||= "Vector v#{version}"
  end

  def to_h
    {
      codename: codename,
      commits: commits.deep_to_h,
      description: description,
      compare_url: compare_url,
      deletions_count: deletions_count,
      date: date,
      insertions_count: insertions_count,
      last_version: last_version,
      permalink: permalink,
      highlights: highlights.deep_to_h,
      title: title,
      type: type,
      type_url: type_url,
      version: version,
      whats_next: whats_next.deep_to_h
    }
  end

  def type
    @type ||=
      if version.major == 0
        "initial dev"
      else
        last_version.bump_type(version)
      end
  end

  def type_url
    case type
    when "initial dev"
      "https://semver.org/#spec-item-4"
    when "patch"
      "https://semver.org/#spec-item-6"
    when "minor"
      "https://semver.org/#spec-item-7"
    when "major"
      "https://semver.org/#spec-item-8"
    else
      raise "Unknown release type #{type.inspect}!"
    end
  end

  private
    def countable_commits
      @countable_commits ||= commits.select do |commit|
        !commit.doc_update?
      end
    end
end
