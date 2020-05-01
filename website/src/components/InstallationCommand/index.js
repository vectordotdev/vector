import React from "react";

import CodeBlock from "@theme/CodeBlock";
import CodeExplanation from "@site/src/components/CodeExplanation";
import TabItem from "@theme/TabItem";
import Tabs from "@theme/Tabs";

function InstallationCommand() {
  return (
    <Tabs
      className="mini"
      defaultValue="humans"
      values={[
        {
          label: (
            <>
              <i className="feather icon-user-check"></i>&nbsp;For Humans
            </>
          ),
          value: "humans",
        },
        {
          label: (
            <>
              <i className="feather icon-cpu"></i>&nbsp;For Machines
            </>
          ),
          value: "machines",
        },
      ]}
    >
      <TabItem value="humans">
        <CodeBlock className="language-bash">
          curl --proto '=https' --tlsv1.2 -sSf https://sh.vector.dev | sh
        </CodeBlock>
        <CodeExplanation>
          <ul>
            <li>Downloads and runs a simple script that installs Vector.</li>
            <li>Prompts are enabled, suitable for humans.</li>
          </ul>
        </CodeExplanation>
      </TabItem>
      <TabItem value="machines">
        <CodeBlock className="language-bash">
          curl --proto '=https' --tlsv1.2 -sSf https://sh.vector.dev | sh -s --
          -y
        </CodeBlock>
        <CodeExplanation>
          <ul>
            <li>Downloads and runs a simple script that installs Vector.</li>
            <li>Prompts are disabled, suitable for machine installations.</li>
          </ul>
        </CodeExplanation>
      </TabItem>
    </Tabs>
  );
}

export default InstallationCommand;
