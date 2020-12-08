module Util
  class Version < Gem::Version
    def bump_type(other_version)
      # Return nil if the other version is not greater than the current version
      if other_version <= self
        return nil
      end

      bumped_version = bump
      next_major = segments.first + 1

      if other_version.prerelease?
        "pre"
      elsif other_version < bumped_version
        "patch"
      elsif other_version == bumped_version
        "minor"
      elsif other_version.segments.first == next_major
        "major"
      else
        nil
      end
    end

    def major
      segments[0]
    end

    def major_x
      "#{segments[0]}.X"
    end

    def minor
      segments[1]
    end

    def minor_x
      "#{segments[0]}.#{segments[1]}.X"
    end

    def patch
      segments[2]
    end
  end
end
