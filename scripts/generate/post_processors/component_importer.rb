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
      'CodeExplanation' => "import CodeExplanation from '@site/src/components/CodeExplanation';",
      'CodeHeader' => "import CodeHeader from '@site/src/components/CodeHeader';",
      'Diagram' => "import Diagram from '@site/src/components/Diagram';",
      'Fields' => "import Fields from '@site/src/components/Fields';",
      'Field' => "import Field from '@site/src/components/Field';",
      'InstallationCommand' => "import InstallationCommand from '@site/src/components/InstallationCommand';",
      'Jump' => "import Jump from '@site/src/components/Jump';",
      'Step' => "import Step from '@site/src/components/Step';",
      'Steps' => "import Steps from '@site/src/components/Steps';",
      'SVG' => "import SVG from 'react-inlinesvg';",
      'Tabs' => "import Tabs from '@theme/Tabs';",
      'TabItem' => "import TabItem from '@theme/TabItem';",
      'VectorComponents' => "import VectorComponents from '@site/src/components/VectorComponents';"
    }

    class << self
      def import!(content)
        IMPORTS.each do |tag, import|
          if content.include?("<#{tag}") && !content.include?(import)
            content.sub!(/( *)<#{tag}/, "\\1#{import}\n\n\\1<#{tag}")
          end
        end

        content
      end
    end
  end
end
