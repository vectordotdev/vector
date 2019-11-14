require_relative "commit"
require_relative "../../util/version"

class Release
  include Comparable

  DOWNLOAD_NAMES = [
    "vector-amd64.deb",
    "vector-x86_64-apple-darwin.tar.gz",
    "vector-x86_64-unknown-linux-musl.tar.gz",
    "vector-x86_64.rpm"
  ]

  attr_reader :commits, :date, :last_version, :version

  def initialize(release_hash, last_version)
    @last_version = last_version
    @date = release_hash.fetch("date").to_date
    @version = Version.new(release_hash.fetch("version"))

    @commits =
      release_hash.fetch("commits").collect do |commit_hash|
        Commit.new(commit_hash)
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

  def deletions_count
    @deletions_count ||= countable_commits.sum(&:deletions_count)
  end

  def doc_updates
    @doc_updates ||= commits.select(&:doc_update?)
  end

  def downloads
    @downloads ||= DOWNLOAD_NAMES.inject({}) do |hash, name|
      hash[name] = "https://github.com/timberio/vector/releases/download/v#{version}/#{name}"
      hash
    end
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

  def to_h
    {
      date: date,
      downloads: downloads,
      last_version: last_version,
      version: version,
    }
  end

  def type
    @type ||= last_version.bump_type(version)
  end

  private
    def countable_commits
      @countable_commits ||= commits.select do |commit|
        !commit.doc_update?
      end
    end
end