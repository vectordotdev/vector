#encoding: utf-8

module PostProcessors
  # Automatically imports components.
  #
  # Imports can only happen once, which is not easy when we use templates.
  # It's also easy to forget an import, which does not throw an error. This
  # post processor ensures imports are always present.
  class ComponentImporter
    IMPORTS = {
      'Accordion' => "import Accordion from '@site/src/components/Accordion';",
      'Alert' => "import Alert from '@site/src/components/Alert';",
      'Assumptions' => "import Assumptions from '@site/src/components/Assumptions';",
      'Changelog' => "import Changelog from '@site/src/components/Changelog';",
      'CodeExplanation' => "import CodeExplanation from '@site/src/components/CodeExplanation';",
      'ConfigExample' => "import ConfigExample from '@site/src/components/ConfigExample';",
      'DaemonDiagram' => "import DaemonDiagram from '@site/src/components/DaemonDiagram';",
      'Diagram' => "import Diagram from '@site/src/components/Diagram';",
      'Fields' => "import Fields from '@site/src/components/Fields';",
      'Field' => "import Field from '@site/src/components/Field';",
      'HighlightItems' => "import HighlightItems from '@theme/HighlightItems';",
      'InstallationCommand' => "import InstallationCommand from '@site/src/components/InstallationCommand';",
      'Jump' => "import Jump from '@site/src/components/Jump';",
      'ServiceDiagram' => "import ServiceDiagram from '@site/src/components/ServiceDiagram';",
      'SidecarDiagram' => "import SidecarDiagram from '@site/src/components/SidecarDiagram';",
      'Steps' => "import Steps from '@site/src/components/Steps';",
      'SVG' => "import SVG from 'react-inlinesvg';",
      'Tabs' => "import Tabs from '@theme/Tabs';",
      'TabItem' => "import TabItem from '@theme/TabItem';",
      'VectorComponents' => "import VectorComponents from '@site/src/components/VectorComponents';",
      'Vic' => "import Vic from '@site/src/components/Vic';"
    }

    FRONTMATTER_FENCE = "---".freeze

    class << self
      def import!(content)
        statements = []

        IMPORTS.each do |tag, import|
          if content.include?("<#{tag}") && !content.include?(import)
            statements << import
          end
        end

        if statements.any?
          imports = statements.join("\n")

          content = content.lstrip

          if content.start_with?(FRONTMATTER_FENCE)
            parts = content.split(FRONTMATTER_FENCE, 3)

            if parts.size < 3
              raise <<~EOF
              Unable to parse

              #{content.inspect}
              EOF
            end

            front_matter = parts[1]
            body = parts[2].lstrip

            FRONTMATTER_FENCE +
              front_matter +
              FRONTMATTER_FENCE +
              "\n\n" +
              imports +
              "\n\n" +
              body
          else
            imports + content
          end
        else
          content
        end
      end
    end
  end
end
