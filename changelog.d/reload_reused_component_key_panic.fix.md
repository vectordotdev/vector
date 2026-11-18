Fixed a panic (`Replacing unknown sink from fanout`) when a config reload reuses a component ID
for a different kind of component (e.g. a transform replaced by a sink), or when a failed reload
restores removed components.

authors: armleth
