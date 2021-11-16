---
date: "2021-11-16"
title: "Automatic namespacing for config files"
description: "A guide that addresses the new automatic namespacing functionality"
authors: ["barieom"]
pr_numbers: []
release: "0.18.0"
hide_on_release_notes: false
badges:
  type: new feature
---

# Automatic namespacing for configuration


We've released automatic namespacing for configuration files, which simplifies namespace configuration for your pipelines.

As Vector continues to evolves, releasing more configuration-heavy functionality such as an aggregator role and pipelines features, there's often a proliferation of configurations necessary to program Vector. Even more, the ability to organize Vector across multiple files was previously lacked clarity and includes a heavy amount of boilerplate, which further made configuration difficult for collaboration and navigation.

To further paint this pain point, Vector users may have had dozens of Vector configuration files, from multiple source files to countless sink files, in a single directory (let's assume Vector is loaded using the configuration `--config-dir /etc/vector`):
```
/etc/vector/
│   file001.toml
│   file002.toml    
│	file003.toml
│   ...
│   file022.toml
│   file023.toml
```

To solve this issue, _automatic namespacing_ provides Vector users the ability to organize their configuration into separate files based on Vector's configuration directory structure. This will make it easy for users like you to split up your configuration files and collaborate with others on their team. Vector will look in every subfolder for any component configuration files and use filenames as their component ID. 

The above example, now with _automatic namespacing_, enables you to transform the example directory structure above to:
```
/etc/vector/
└───souces/
│   │   file001.toml
│   │   ...
│   │   file005.toml
│   
└───transforms/
│   │   file006.toml
│   │   ...
│   │   file016.toml
│ 
└───sinks/
    │   file017.toml
    │   file022.toml
```

The configuration files become simplified from
``` toml
# /etc/vector/file017.toml
[sinks.foo]
type = "anything"
```

which becomes
``` toml
# /etc/vector/sinks/file017.toml
type = "anything"
```

This additionally serves as a way to provide more RBAC (role-based access control) to admins at, say, organizations with sophisticated structure. In projects or organizations where read-write access needs to be limited or restricted for different teams, _automatic namespacing_ can provide different teams access to specific pipelines, such as limiting the access to a security team to only the security pipeline.

If you any feedback for us, let us know on our [Discord chat][] or on [Twitter][].


[Discord chat]:https://discord.com/invite/dX3bdkF
[Twitter]:https://twitter.com/vectordotdev