#[cfg(test)]
mod tests;

use std::collections::{hash_map, HashMap};

#[macro_export]
macro_rules! hashmap {
    () => (::structures::map::hash::HashMap::new());

    // trailing comma case
    ($($key:expr => $value:expr,)+) => (ordmap!($($key => $value),+));

    ($($key:expr => $value:expr),*) => {
        {
            let mut _map = ::structures::map::hash::HashMap::new();
            $(
                let _ = _map.insert($key.into(), $value.into());
            )*
            _map
        }
    };
}
pub type OrdMap<K, V> = HashMap<K, V>;
pub type IntoIter<K, V> = hash_map::IntoIter<K, V>;
pub type Iter<'a, K, V> = hash_map::Iter<'a, K, V>;
pub type IterMut<'a, K, V> = hash_map::IterMut<'a, K, V>;
