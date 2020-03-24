#encoding: utf-8

require 'date'

module PostProcessors
  # Adds a `last_modified` attribute to the front matter.
  class LastModifiedSetter
    class << self
      def set!(content, target_path)
        if !target_path.start_with?("#{ROOT_DIR}/website/")
          return content
        end

        old_content = File.read(target_path).lstrip

        if !old_content.start_with?("---\nlast_modified_on: ") || content != old_content
          content = content.lstrip
          today = Date.today.iso8601

          if content.start_with?("---last_modified_on: \n")
            content.sub(/^---\nlast_modified_on: ["0-9\-]*?\n/, "---\nlast_modified_on: \"#{today}\"\n")
          elsif content.start_with?("---\n")
            content.sub(/^---\n/, "---\nlast_modified_on: \"#{today}\"\n")
          else
            "---\nlast_modified_on: #{today}\n---\n#{content}"
          end
        else
          content
        end
      end
    end
  end
end
