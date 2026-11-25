Fixed a panic (`Replacing unknown sink from fanout`) on config reload when a component ID is reused
by a different kind of component, or when a failed reload restores a removed component.

authors: armleth
