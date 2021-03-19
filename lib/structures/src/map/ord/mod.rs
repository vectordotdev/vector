#[cfg(test)]
mod tests;

use std::collections::{btree_map, BTreeMap};

#[macro_export]
macro_rules! ordmap {
    () => (::structures::map::ord::OrdMap::new());

    // trailing comma case
    ($($key:expr => $value:expr,)+) => (ordmap!($($key => $value),+));

    ($($key:expr => $value:expr),*) => {
        {
            let mut _map = ::structures::map::ord::OrdMap::new();
            $(
                let _ = _map.insert($key.into(), $value.into());
            )*
            _map
        }
    };
}
pub type OrdMap<K, V> = BTreeMap<K, V>;
pub type IntoIter<K, V> = btree_map::IntoIter<K, V>;
pub type Iter<'a, K, V> = btree_map::Iter<'a, K, V>;
pub type IterMut<'a, K, V> = btree_map::IterMut<'a, K, V>;
