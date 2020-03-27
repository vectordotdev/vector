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

        if content_changed?(old_content, content)
          content = content.lstrip
          today = Date.today.iso8601
          add_last_modified_on(content)
        else
          old_content
        end
      end

      private
        def content_changed?(old_content, new_content)
          old_content = old_content.sub(/---\nlast_modified_on: .*?\n/, "---\n")
          old_content != new_content
        end

        def add_last_modified_on(content)
          content = content.lstrip
          today = Date.today.iso8601

          if content.start_with?("---\nlast_modified_on:")
            content.sub(/^---\nlast_modified_on: ["0-9\-]*?\n/, "---\nlast_modified_on: \"#{today}\"\n")
          elsif content.start_with?("---\n")
            content.sub(/^---\n/, "---\nlast_modified_on: \"#{today}\"\n")
          else
            "---\nlast_modified_on: #{today}\n---\n#{content}"
          end
        end
    end
  end
end
