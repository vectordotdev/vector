use crate::value;
use std::collections::{btree_map::Entry, BTreeMap};
use std::ops::{BitAnd, BitOr};

/// Properties for a given expression that express the expected outcome of the
/// expression.
///
/// This includes whether the expression is fallible, whether it can return
/// "nothing", and a list of values the expression can resolve to.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TypeDef {
    /// True, if an expression can return an error.
    ///
    /// Some expressions are infallible (e.g. the [`Literal`] expression, or any
    /// custom function designed to be infallible).
    pub fallible: bool,

    /// The [`value::Kind`]s this definition represents.
    pub kind: value::Kind,

    /// Some types contain a collection of other types. If they do, this value
    /// is set to `Some`, and returns the [`TypeDef`] of the collected inner
    /// types.
    ///
    /// For example, given a [`Value::Array`]:
    ///
    /// ```rust
    /// # use remap_lang::{expression::Array, Value, Expression, state, InnerTypeDef, TypeDef, value::Kind};
    ///
    /// let vec = vec![Value::Null, Value::Boolean(true)];
    /// let expression = Array::from(vec);
    /// let state = state::Compiler::default();
    ///
    /// assert_eq!(
    ///     expression.type_def(&state),
    ///     TypeDef {
    ///         fallible: false,
    ///         kind: Kind::Array,
    ///         inner_type_def: Some(InnerTypeDef::Array(TypeDef {
    ///             fallible: false,
    ///             kind: Kind::Null | Kind::Boolean,
    ///             inner_type_def: None,
    ///         }.boxed())),
    ///     },
    /// );
    /// ```
    pub inner_type_def: Option<InnerTypeDef>,
}

impl Default for TypeDef {
    fn default() -> Self {
        Self {
            fallible: false,
            kind: value::Kind::all(),
            inner_type_def: None,
        }
    }
}

impl BitOr for TypeDef {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        let inner_type_def = match (self.inner_type_def, rhs.inner_type_def) {
            (None, None) => None,
            (None, Some(rhs)) => Some(rhs),
            (Some(lhs), None) => Some(lhs),
            (Some(lhs), Some(rhs)) => Some(lhs | rhs),
        };

        Self {
            fallible: self.fallible | rhs.fallible,
            kind: self.kind | rhs.kind,
            inner_type_def,
        }
    }
}

impl BitAnd for TypeDef {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        let maps = |lhs: BTreeMap<String, TypeDef>, rhs: BTreeMap<String, TypeDef>| {
            // Calculate the intersection of the two maps
            let mut map = BTreeMap::new();
            for (key, value1) in lhs.into_iter() {
                if let Some(value2) = rhs.get(&key) {
                    map.insert(key, value1 & value2.clone());
                }
            }
            map
        };

        let inner_type_def = match (self.inner_type_def, rhs.inner_type_def) {
            (
                Some(InnerTypeDef::Both {
                    map: lhsm,
                    array: lhsa,
                }),
                Some(InnerTypeDef::Both {
                    map: rhsm,
                    array: rhsa,
                }),
            ) => Some(InnerTypeDef::Both {
                map: maps(lhsm, rhsm),
                array: (*lhsa & *rhsa).boxed(),
            }),

            (Some(InnerTypeDef::Map(lhs)), Some(InnerTypeDef::Map(rhs)))
            | (Some(InnerTypeDef::Map(lhs)), Some(InnerTypeDef::Both { map: rhs, .. }))
            | (Some(InnerTypeDef::Both { map: lhs, .. }), Some(InnerTypeDef::Map(rhs))) => {
                Some(InnerTypeDef::Map(maps(lhs, rhs)))
            }

            (Some(InnerTypeDef::Array(lhs)), Some(InnerTypeDef::Array(rhs)))
            | (Some(InnerTypeDef::Both { array: lhs, .. }), Some(InnerTypeDef::Array(rhs)))
            | (Some(InnerTypeDef::Array(lhs)), Some(InnerTypeDef::Both { array: rhs, .. })) => {
                Some(InnerTypeDef::Array((*lhs & *rhs).boxed()))
            }

            _ => None,
        };

        Self {
            fallible: self.fallible & rhs.fallible,
            kind: self.kind & rhs.kind,
            inner_type_def,
        }
    }
}

impl From<value::Kind> for TypeDef {
    fn from(kind: value::Kind) -> Self {
        Self {
            kind,
            ..Default::default()
        }
    }
}

impl TypeDef {
    pub fn boxed(self) -> Box<Self> {
        Box::new(self)
    }

    /// Returns the set of scalar kinds associated with this type definition.
    ///
    /// If a type definition includes an `inner_type_def`, this method will
    /// recursively resolve those until the final scalar kinds are known.
    pub fn scalar_kind(&self) -> value::Kind {
        let mut kind = self.kind.scalar();
        let mut type_def = self.inner_type_def.clone();

        while let Some(InnerTypeDef::Array(td)) = type_def {
            kind |= td.kind.scalar();
            type_def = td.inner_type_def;
        }

        kind
    }

    pub fn is_fallible(&self) -> bool {
        self.fallible
    }

    pub fn into_fallible(mut self, fallible: bool) -> Self {
        self.fallible = fallible;
        self
    }

    /// Returns `true` if the _other_ [`TypeDef`] is contained within the
    /// current one.
    ///
    /// That is to say, its constraints must be more strict or equal to the
    /// constraints of the current one.
    pub fn contains(&self, other: &Self) -> bool {
        // If we don't expect fallible, but the other does, the other's
        // requirement is less strict than ours.
        if !self.is_fallible() && other.is_fallible() {
            return false;
        }

        self.kind.contains(other.kind)
    }

    pub fn fallible_unless(mut self, kind: impl Into<value::Kind>) -> Self {
        if !kind.into().contains(self.kind) {
            self.fallible = true
        }

        self
    }

    pub fn with_constraint(mut self, kind: impl Into<value::Kind>) -> Self {
        self.kind = kind.into();

        if self.kind.is_scalar() {
            self.inner_type_def = None;
        }

        self
    }

    pub fn with_inner_type(mut self, inner_type: Option<InnerTypeDef>) -> Self {
        self.inner_type_def = inner_type;
        self
    }

    /// Applies a type constraint to the items in an array. If you need all items in the array to
    /// be integers, for example, set `Kind::Integer`; if items can be either integers or Booleans,
    /// set `Kind::Integer | Kind::Boolean`; and so on.
    pub fn fallible_unless_array_has_inner_type(mut self, kind: impl Into<value::Kind>) -> Self {
        match &self.inner_type_def {
            Some(InnerTypeDef::Array(inner_kind)) if kind.into() == inner_kind.kind => (),
            _ => {
                self.fallible = true;
            }
        }

        self
    }

    pub fn merge(self, other: Self) -> Self {
        self | other
    }

    pub fn merge_optional(self, other: Option<Self>) -> Self {
        match other {
            Some(other) => self.merge(other),
            None => self,
        }
    }

    /// Similar to `merge_optional`, except that the optional `TypeDef` is
    /// considered to be the "default" for the `self` `TypeDef`.
    ///
    /// The implication of this is that the resulting `TypeDef` will be equal to
    /// `self` or `other`, if either of the two is infallible.
    ///
    /// If neither are, the two type definitions are merged as usual.
    pub fn merge_with_default_optional(self, other: Option<Self>) -> Self {
        if !self.is_fallible() {
            return self;
        }

        match other {
            None => self,

            // If `self` isn't exact, see if `other` is.
            Some(other) if !other.is_fallible() => other,

            // Otherwise merge the optional as usual.
            Some(other) => self.merge(other),
        }
    }
}

/// The inner type defnition for a type.
///
/// Maps will have an inner type definition that represents the typedefs of the
/// fields contained by the map.
///
/// Arrays have a single inner type definition representing the type for each element
/// contained within the array.
///
/// All other type just have an inner typedef of `None`.
///
/// Some expressions can potentially evaluate to either a Map and an Array. eg.
///
/// `if .foo { [1, 2, 3] } else { {"foo": "bar" } }
///
/// These expressions can be represented using `Both`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum InnerTypeDef {
    Array(Box<TypeDef>),
    Map(BTreeMap<String, TypeDef>),
    Both {
        map: BTreeMap<String, TypeDef>,
        array: Box<TypeDef>,
    },
}

impl BitOr for InnerTypeDef {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        let maps = |lhs: BTreeMap<String, TypeDef>, rhs: BTreeMap<String, TypeDef>| {
            // Calculate the union of the two maps.
            let mut map = BTreeMap::new();
            for (key, value) in lhs.into_iter().chain(rhs.into_iter()) {
                // Using match here rather than `and_modify` and `or_insert` to avoid having to clone `value`.
                match map.entry(key) {
                    Entry::Occupied(mut l) => {
                        let l: &mut TypeDef = l.get_mut();
                        *l = l.clone() | value;
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(value);
                    }
                }
            }
            map
        };

        match (self, rhs) {
            (InnerTypeDef::Array(lhs), InnerTypeDef::Array(rhs)) => {
                InnerTypeDef::Array((*lhs | *rhs).boxed())
            }
            (InnerTypeDef::Map(lhs), InnerTypeDef::Map(rhs)) => InnerTypeDef::Map(maps(lhs, rhs)),
            (InnerTypeDef::Array(array), InnerTypeDef::Map(map))
            | (InnerTypeDef::Map(map), InnerTypeDef::Array(array)) => {
                InnerTypeDef::Both { map, array }
            }
            (InnerTypeDef::Both { map: map1, array }, InnerTypeDef::Map(map2))
            | (InnerTypeDef::Map(map1), InnerTypeDef::Both { map: map2, array }) => {
                InnerTypeDef::Both {
                    map: maps(map1, map2),
                    array,
                }
            }
            (InnerTypeDef::Both { map, array: array1 }, InnerTypeDef::Array(array2))
            | (InnerTypeDef::Array(array1), InnerTypeDef::Both { map, array: array2 }) => {
                InnerTypeDef::Both {
                    map,
                    array: (*array1 | *array2).boxed(),
                }
            }
            (
                InnerTypeDef::Both {
                    map: map1,
                    array: array1,
                },
                InnerTypeDef::Both {
                    map: map2,
                    array: array2,
                },
            ) => InnerTypeDef::Both {
                map: maps(map1, map2),
                array: (*array1 | *array2).boxed(),
            },
        }
    }
}

/// Utility macro to make defining inner type def maps easier.
/// For example, to specify an inner type def for an array of booleans:
///
/// ```rust
/// # use remap_lang::{inner_type_def, value::Kind};
/// inner_type_def!([Kind::Boolean]);
/// ```
///
/// For a map with two fields:
///
/// ```rust
/// # use remap_lang::{inner_type_def, value::Kind};
/// inner_type_def!({ "field1": Kind::Boolean,
///                   "field2": Kind::Integer });
/// ```
///
///
#[macro_export]
macro_rules! inner_type_def {
    ([$v:expr]) => ( $crate::InnerTypeDef::Array( Box::new( $v.into())) );

    ({}) => (
        $crate::InnerTypeDef::Map(::std::collections::BTreeMap::new())
    );

    ({$($k:tt: $v:expr),+ $(,)?}) => ({
        $crate::InnerTypeDef::Map(
            vec![$(($k.to_owned(), $v.into())),+]
                .into_iter()
                .collect::<::std::collections::BTreeMap<_, _>>()
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use value::Kind;

    #[test]
    fn scalar_kind() {
        let type_def = TypeDef {
            kind: Kind::Array,
            inner_type_def: Some(InnerTypeDef::Array(
                TypeDef {
                    kind: Kind::Boolean | Kind::Float,
                    inner_type_def: Some(InnerTypeDef::Array(
                        TypeDef {
                            kind: Kind::Bytes,
                            ..Default::default()
                        }
                        .boxed(),
                    )),
                    ..Default::default()
                }
                .boxed(),
            )),
            ..Default::default()
        };

        assert_eq!(
            type_def.scalar_kind(),
            Kind::Boolean | Kind::Float | Kind::Bytes
        );
    }

    #[test]
    fn inner_type_def_or() {
        let type_def_a = inner_type_def!([Kind::Boolean]);
        let type_def_b = inner_type_def!([Kind::Integer]);
        let expected = inner_type_def!([Kind::Integer | Kind::Boolean]);

        assert_eq!(expected, type_def_a | type_def_b);

        let type_def_a = inner_type_def!({ "a": Kind::Boolean, "b": Kind::Bytes });
        let type_def_b = inner_type_def!({ "a": Kind::Float, "c": Kind::Timestamp });
        let expected = inner_type_def!({
            "a": Kind::Boolean | Kind::Float,
            "b": Kind::Bytes,
            "c": Kind::Timestamp
        });

        assert_eq!(expected, type_def_a | type_def_b);
    }

    #[test]
    fn array_inner_type() {
        // All items are strings + all must be strings -> infallible
        let non_mixed_array = TypeDef {
            inner_type_def: Some(inner_type_def!([Kind::Bytes])),
            ..Default::default()
        }
        .fallible_unless_array_has_inner_type(Kind::Bytes);

        assert!(!non_mixed_array.is_fallible());

        // Items are strings or Booleans + all must be strings -> fallible
        let mixed_array_mismatched = TypeDef {
            inner_type_def: Some(inner_type_def!([Kind::Bytes | Kind::Boolean])),
            ..Default::default()
        }
        .fallible_unless_array_has_inner_type(Kind::Bytes);

        assert!(mixed_array_mismatched.is_fallible());

        // Items are integers or floats + all must be integers or floats -> infallible
        let mixed_array_matched = TypeDef {
            inner_type_def: Some(inner_type_def!([Kind::Integer | Kind::Float])),
            ..Default::default()
        }
        .fallible_unless_array_has_inner_type(Kind::Integer | Kind::Float);

        assert!(!mixed_array_matched.is_fallible());

        // Items are Booleans or maps + must be floats -> fallible
        let mismatched_array = TypeDef {
            inner_type_def: Some(inner_type_def!([Kind::Boolean | Kind::Map])),
            ..Default::default()
        }
        .fallible_unless_array_has_inner_type(Kind::Float);

        assert!(mismatched_array.is_fallible());

        // Setting a required array type on a map -> fallible
        let map_type = TypeDef {
            kind: Kind::Map,
            inner_type_def: Some(inner_type_def!([Kind::Map])),
            ..Default::default()
        }
        .fallible_unless_array_has_inner_type(Kind::Bytes);

        assert!(map_type.is_fallible());

        // Any non-array should be fallible if an inner type constraint is
        // applied
        let non_array = TypeDef {
            kind: Kind::Bytes | Kind::Float | Kind::Boolean,
            ..Default::default()
        }
        .fallible_unless_array_has_inner_type(Kind::Bytes);

        assert!(non_array.is_fallible());
    }
}
