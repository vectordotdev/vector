#[cfg(test)]
mod tests;

use std::collections::{btree_map, BTreeMap};

pub type OrdMap<K, V> = BTreeMap<K, V>;
pub type IntoIter<K, V> = btree_map::IntoIter<K, V>;
pub type Iter<'a, K, V> = btree_map::Iter<'a, K, V>;
pub type IterMut<'a, K, V> = btree_map::IterMut<'a, K, V>;
