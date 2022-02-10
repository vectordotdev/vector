//! TypeDefs
//!
//! The type definitions for typedefs record the various possible type definitions for the state
//! that can be passed through a VRL program.
//!
//! `TypeDef` contains a `KindInfo`.
//!
//! `KindInfo` can be:
//! `Unknown` - We don't know what type this is.
//! `Known` - A set of the possible known `TypeKind`s. There can be multiple possible types for a
//! path in scenarios such as `if .thing { .x = "hello" } else { .x = 42 }`. In that example after
//! that statement is run, `.x` could contain either an string or an integer, we won't know until
//! runtime exactly which.
//!
//! `TypeKind` is a concrete type for a path, `Bytes` (string), `Integer`, `Float`, `Boolean`,
//! `Timestamp`, `Regex`, `Null` or `Array` or `Object`.
//!
//! `Array` is a Map of `Index` -> `KindInfo`.
//! `Index` can be a specific index into that array, or `Any` which represents any index found within
//! that array.
//!
//! `Object` is a Map of `Field` -> `KindInfo`.
//! `Field` can be a specifix field name of the object, or `Any` which represents any element found
//! within that object.
//!
//!
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Sub,
};

use lookup::{FieldBuf, LookupBuf, SegmentBuf};

use crate::{map, value::Kind};

/// Properties for a given expression that express the expected outcome of the
/// expression.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TypeDef {
    /// True, if an expression can return an error.
    ///
    /// Some expressions are infallible (e.g. the [`Literal`][crate::expression::Literal] expression, or any
    /// custom function designed to be infallible).
    pub fallible: bool,

    /// The [`value::Kind`][crate::value::Kind]s this definition represents.
    ///
    /// This is wrapped in a [`TypeKind`] enum, such that we encode details
    /// about potential inner kinds for collections (arrays or objects).
    pub kind: KindInfo,
}

impl Sub<Kind> for TypeDef {
    type Output = Self;

    /// Removes the given kinds from this type definition.
    fn sub(mut self, other: Kind) -> Self::Output {
        self.kind = match self.kind {
            KindInfo::Unknown => KindInfo::Unknown,
            KindInfo::Known(kinds) => {
                KindInfo::Known(kinds.into_iter().filter(|k| k.to_kind() != other).collect())
            }
        };

        self
    }
}

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub enum KindInfo {
    Unknown,
    Known(BTreeSet<TypeKind>),
}

impl From<Kind> for KindInfo {
    fn from(kind: Kind) -> Self {
        let info = Self::Unknown;

        if kind.is_empty() || kind.is_all() {
            return info;
        }

        let mut set = BTreeSet::default();

        if kind.contains_bytes() {
            set.insert(TypeKind::Bytes);
        }
        if kind.contains_integer() {
            set.insert(TypeKind::Integer);
        }
        if kind.contains_float() {
            set.insert(TypeKind::Float);
        }
        if kind.contains_boolean() {
            set.insert(TypeKind::Boolean);
        }
        if kind.contains_timestamp() {
            set.insert(TypeKind::Timestamp);
        }
        if kind.contains_regex() {
            set.insert(TypeKind::Regex);
        }
        if kind.contains_null() {
            set.insert(TypeKind::Null);
        }
        if kind.contains_array() {
            let mut map = BTreeMap::default();
            map.insert(Index::Any, Self::Unknown);
            set.insert(TypeKind::Array(map));
        }
        if kind.contains_object() {
            let mut map = BTreeMap::default();
            map.insert(Field::Any, Self::Unknown);
            set.insert(TypeKind::Object(map));
        }

        Self::Known(set)
    }
}

impl KindInfo {
    pub fn or_null(self) -> Self {
        use KindInfo::*;

        match self {
            Unknown => Unknown,
            Known(mut set) => {
                set.insert(TypeKind::Null);
                Known(set)
            }
        }
    }

    fn object(&self) -> Option<&BTreeMap<Field, KindInfo>> {
        match self {
            KindInfo::Unknown => None,
            KindInfo::Known(set) => set.iter().find_map(|k| match k {
                TypeKind::Object(object) => Some(object),
                _ => None,
            }),
        }
    }

    fn array(&self) -> Option<&BTreeMap<Index, KindInfo>> {
        match self {
            KindInfo::Unknown => None,
            KindInfo::Known(set) => set.iter().find_map(|k| match k {
                TypeKind::Array(array) => Some(array),
                _ => None,
            }),
        }
    }

    /// Insert the given [`KindInfo`] into a provided path.
    ///
    /// For example, given kind info:
    ///
    /// KindInfo {
    ///   Object {
    ///     "bar": KindInfo {
    ///       Bytes
    ///     }
    ///   }
    /// }
    ///
    /// And a path `.foo`, This would return:
    ///
    ///
    /// KindInfo {
    ///   Object {
    ///     "foo": KindInfo {
    ///       Object {
    ///         "bar" : KindInfo {
    ///           Bytes
    ///         }
    ///       }
    ///     }
    ///   }
    /// }
    ///
    /// e.g., the existing [`KindInfo`] gets nested into the provided path.
    pub fn for_path(mut self, path: LookupBuf) -> Self {
        for segment in path.iter().rev() {
            match segment {
                SegmentBuf::Field(FieldBuf { name, .. }) => {
                    let mut map = BTreeMap::default();
                    map.insert(Field::Field(name.as_str().to_owned()), self);

                    let mut set = BTreeSet::new();
                    set.insert(TypeKind::Object(map));

                    self = KindInfo::Known(set);
                }
                SegmentBuf::Coalesce(fields) => {
                    // TODO: I'm not sure this is right - it should be the
                    // combined typedef of all the fields in the coalesce.
                    let field = fields.last().unwrap();
                    let mut map = BTreeMap::default();
                    map.insert(Field::Field(field.as_str().to_owned()), self);

                    let mut set = BTreeSet::new();
                    set.insert(TypeKind::Object(map));

                    self = KindInfo::Known(set);
                }
                SegmentBuf::Index(index) => {
                    // For negative indices, we have to mark the array contents
                    // as unknown.
                    let (index, info) = if index.is_negative() {
                        (Index::Any, KindInfo::Unknown)
                    } else {
                        (Index::Index(*index as usize), self)
                    };

                    let mut map = BTreeMap::default();
                    map.insert(index, info);

                    let mut set = BTreeSet::new();
                    set.insert(TypeKind::Array(map));

                    self = KindInfo::Known(set);
                }
            }
        }

        self
    }

    /// Given a [`KindInfo`], try to fetch the inner [`KindInfo`] based on the
    /// provided path.
    ///
    /// For example, Given kind info:
    ///
    /// KindInfo {
    ///   Object {
    ///     "foo": KindInfo {
    ///       Bytes
    ///     }
    ///   }
    /// }
    ///
    /// And a path `.foo`. This would return `KindInfo::Bytes`.
    pub fn at_path(&self, path: LookupBuf) -> Self {
        let mut iter = path.into_iter();

        let info = match self {
            kind @ KindInfo::Unknown => return kind.clone(),
            kind @ KindInfo::Known(_) => {
                let new = match iter.next() {
                    None => return kind.clone(),
                    Some(segment) => match segment {
                        SegmentBuf::Coalesce(fields) => match kind.object() {
                            None => KindInfo::Unknown,
                            Some(kind) => fields
                                .into_iter()
                                .find_map(|field| {
                                    let field = Field::Field(field.as_str().to_owned());
                                    kind.get(&field).cloned()
                                })
                                .unwrap_or_else(|| {
                                    if let Some(kind) = kind.get(&Field::Any) {
                                        kind.clone()
                                    } else {
                                        KindInfo::Unknown
                                    }
                                }),
                        },
                        SegmentBuf::Field(FieldBuf { name: field, .. }) => match kind.object() {
                            None => KindInfo::Unknown,
                            Some(kind) => {
                                let field = Field::Field(field.as_str().to_owned());

                                if let Some(kind) = kind.get(&field) {
                                    kind.clone()
                                } else if let Some(kind) = kind.get(&Field::Any) {
                                    kind.clone()
                                } else {
                                    KindInfo::Unknown
                                }
                            }
                        },
                        SegmentBuf::Index(index) => match kind.array() {
                            None => KindInfo::Unknown,
                            Some(kind) => {
                                let index = Index::Index(index as usize);

                                if let Some(kind) = kind.get(&index) {
                                    kind.clone()
                                } else if let Some(kind) = kind.get(&Index::Any) {
                                    kind.clone()
                                } else {
                                    KindInfo::Unknown
                                }
                            }
                        },
                    },
                };

                match kind {
                    KindInfo::Known(set) if set.len() > 1 => new.or_null(),
                    _ => new,
                }
            }
        };

        info.at_path(LookupBuf::from_segments(iter.collect()))
    }

    fn merge(self, rhs: Self, shallow: bool, overwrite: bool) -> Self {
        use KindInfo::*;

        match (self, rhs) {
            (KindInfo::Known(lhs), KindInfo::Known(rhs)) => {
                let (lhs_array, lhs): (Vec<_>, Vec<_>) = lhs
                    .into_iter()
                    .partition(|k| matches!(k, TypeKind::Array(_)));
                let lhs_array = lhs_array
                    .into_iter()
                    .filter_map(|k| match k {
                        TypeKind::Array(v) => Some(v),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .pop();

                let (rhs_array, rhs): (Vec<_>, Vec<_>) = rhs
                    .into_iter()
                    .partition(|k| matches!(k, TypeKind::Array(_)));
                let rhs_array = rhs_array
                    .into_iter()
                    .filter_map(|k| match k {
                        TypeKind::Array(v) => Some(v),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .pop();

                // If both the lhs and rhs contain an array, we need to merge
                // their definitions.
                //
                // We do this by taking the highest index of the lhs array, and
                // increase the indexes of the rhs index by that amount.
                #[allow(clippy::suspicious_arithmetic_impl)]
                let array = lhs_array
                    .clone()
                    .zip(rhs_array.clone())
                    .map(|(mut l, mut r)| {
                        if !overwrite {
                            let r_start = l
                                .keys()
                                .filter_map(|i| i.to_inner())
                                .max()
                                .map(|i| i + 1)
                                .unwrap_or_default();

                            r = r
                                .into_iter()
                                .map(|(i, v)| (i.shift(r_start), v))
                                .collect::<BTreeMap<_, _>>();
                        };

                        l.append(&mut r);
                        l
                    })
                    .or_else(|| lhs_array.or(rhs_array));

                let (lhs_object, lhs): (Vec<_>, Vec<_>) = lhs
                    .into_iter()
                    .partition(|k| matches!(k, TypeKind::Object(_)));
                let lhs_object = lhs_object
                    .into_iter()
                    .filter_map(|k| match k {
                        TypeKind::Object(v) => Some(v),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .pop();

                let (rhs_object, rhs): (Vec<_>, Vec<_>) = rhs
                    .into_iter()
                    .partition(|k| matches!(k, TypeKind::Object(_)));
                let rhs_object = rhs_object
                    .into_iter()
                    .filter_map(|k| match k {
                        TypeKind::Object(v) => Some(v),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .pop();

                // Similar to merging two arrays, but for objects.
                //
                // In this case, all we care about is merging the two objects,
                // with the rhs object taking precedence.
                let object = lhs_object
                    .clone()
                    .zip(rhs_object.clone())
                    .map(|(mut l, mut r)| {
                        // merge nested keys, if requested
                        if !shallow {
                            for (k1, v1) in l.iter_mut() {
                                for (k2, v2) in r.iter_mut() {
                                    if k1 == k2 {
                                        *v2 = v1.clone().merge(v2.clone(), false, false);
                                    }
                                }
                            }
                        }

                        l.append(&mut r);
                        l
                    })
                    .or_else(|| lhs_object.or(rhs_object));

                let mut lhs: BTreeSet<_> = lhs.into_iter().collect();
                let mut rhs = rhs.into_iter().collect();
                lhs.append(&mut rhs);

                if let Some(array) = array {
                    lhs.insert(TypeKind::Array(array));
                }

                if let Some(object) = object {
                    lhs.insert(TypeKind::Object(object));
                }
                Known(lhs)
            }
            (lhs @ Known(_), _) => lhs,
            (_, rhs @ Known(_)) => rhs,
            _ => Unknown,
        }
    }

    fn map<F>(&self, f: F) -> Self
    where
        F: Fn(&TypeKind) -> TypeKind,
    {
        match self {
            Self::Unknown => Self::Unknown,
            Self::Known(set) => Self::Known(set.iter().map(f).collect()),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub enum TypeKind {
    Bytes,
    Integer,
    Float,
    Boolean,
    Timestamp,
    Regex,
    Null,
    Array(BTreeMap<Index, KindInfo>),
    Object(BTreeMap<Field, KindInfo>),
}

impl TypeKind {
    /// Convert a given [`TypeKind`] into a [`Kind`].
    pub fn to_kind(&self) -> Kind {
        use TypeKind::*;

        match self {
            Bytes => Kind::Bytes,
            Integer => Kind::Integer,
            Float => Kind::Float,
            Boolean => Kind::Boolean,
            Timestamp => Kind::Timestamp,
            Regex => Kind::Regex,
            Null => Kind::Null,
            Array(_) => Kind::Array,
            Object(_) => Kind::Object,
        }
    }

    /// Collects the kinds into a single kind.
    /// Array and objects may have different kinds for each key/index, this collects those
    /// into a single kind.
    pub fn collect_kinds(self) -> TypeKind {
        match self {
            TypeKind::Array(kinds) => {
                let mut newkinds = BTreeMap::new();
                newkinds.insert(
                    Index::Any,
                    kinds
                        .into_iter()
                        .fold(KindInfo::Unknown, |acc, (_, k)| acc.merge(k, false, true)),
                );
                TypeKind::Array(newkinds)
            }

            TypeKind::Object(kinds) => {
                let mut newkinds = BTreeMap::new();
                newkinds.insert(
                    Field::Any,
                    kinds
                        .into_iter()
                        .fold(KindInfo::Unknown, |acc, (_, k)| acc.merge(k, false, true)),
                );
                TypeKind::Object(newkinds)
            }
            _ => self,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
pub enum Index {
    Any,
    Index(usize),
}

impl Index {
    fn to_inner(self) -> Option<usize> {
        match self {
            Index::Any => None,
            Index::Index(i) => Some(i),
        }
    }

    fn shift(self, count: usize) -> Self {
        match self {
            Index::Index(i) => Index::Index(i + count),
            v => v,
        }
    }
}

impl From<usize> for Index {
    fn from(i: usize) -> Self {
        Self::Index(i)
    }
}

impl From<i32> for Index {
    fn from(i: i32) -> Self {
        Self::Index(i as usize)
    }
}

impl From<()> for Index {
    fn from(_: ()) -> Self {
        Self::Any
    }
}

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub enum Field {
    Any,
    Field(String),
}

impl From<String> for Field {
    fn from(k: String) -> Self {
        Self::Field(k)
    }
}

impl From<&'static str> for Field {
    fn from(k: &'static str) -> Self {
        Self::Field(k.to_owned())
    }
}

impl From<()> for Field {
    fn from(_: ()) -> Self {
        Self::Any
    }
}

impl TypeDef {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn at_path(&self, path: LookupBuf) -> TypeDef {
        let fallible = self.fallible;
        let kind = self.kind.at_path(path);

        Self { fallible, kind }
    }

    pub fn for_path(self, path: LookupBuf) -> TypeDef {
        let fallible = self.fallible;
        let kind = self.kind.for_path(path);

        Self { fallible, kind }
    }

    pub fn kind(&self) -> Kind {
        match &self.kind {
            KindInfo::Unknown => Kind::all(),
            KindInfo::Known(set) => set.iter().fold(Kind::empty(), |acc, k| acc | k.to_kind()),
        }
    }

    #[inline]
    pub fn fallible(mut self) -> Self {
        self.fallible = true;
        self
    }

    #[inline]
    pub fn infallible(mut self) -> Self {
        self.fallible = false;
        self
    }

    #[inline]
    pub fn with_fallibility(mut self, fallible: bool) -> Self {
        self.fallible = fallible;
        self
    }

    #[inline]
    pub fn unknown(mut self) -> Self {
        self.kind = KindInfo::Unknown;
        self
    }

    #[inline]
    pub fn bytes(self) -> Self {
        self.scalar(Kind::Bytes)
    }

    #[inline]
    pub fn add_bytes(self) -> Self {
        self.add_scalar(Kind::Bytes)
    }

    #[inline]
    pub fn integer(self) -> Self {
        self.scalar(Kind::Integer)
    }

    #[inline]
    pub fn add_integer(self) -> Self {
        self.add_scalar(Kind::Integer)
    }

    #[inline]
    pub fn float(self) -> Self {
        self.scalar(Kind::Float)
    }

    #[inline]
    pub fn add_float(self) -> Self {
        self.add_scalar(Kind::Float)
    }

    #[inline]
    pub fn boolean(self) -> Self {
        self.scalar(Kind::Boolean)
    }

    #[inline]
    pub fn add_boolean(self) -> Self {
        self.add_scalar(Kind::Boolean)
    }

    #[inline]
    pub fn timestamp(self) -> Self {
        self.scalar(Kind::Timestamp)
    }

    #[inline]
    pub fn add_timestamp(self) -> Self {
        self.add_scalar(Kind::Timestamp)
    }

    #[inline]
    pub fn regex(self) -> Self {
        self.scalar(Kind::Regex)
    }

    #[inline]
    pub fn add_regex(self) -> Self {
        self.add_scalar(Kind::Regex)
    }

    #[inline]
    pub fn null(self) -> Self {
        self.scalar(Kind::Null)
    }

    #[inline]
    pub fn add_null(self) -> Self {
        self.add_scalar(Kind::Null)
    }

    /// Set the type definition kind to a scalar.
    ///
    /// This overwrites any existing kind information.
    #[inline]
    pub fn scalar(self, kind: Kind) -> Self {
        self.unknown().add_scalar(kind)
    }

    /// Add a new scalar kind to the type definition.
    ///
    /// This appends the new scalars to the existing kinds.
    pub fn add_scalar(mut self, kind: Kind) -> Self {
        debug_assert!(kind.is_scalar());

        self.kind = self.kind.merge(kind.into(), false, false);
        self
    }

    #[inline]
    pub fn array<V>(self, inner: Vec<V>) -> Self
    where
        V: Into<Self>,
    {
        self.unknown().add_array(inner)
    }

    #[inline]
    pub fn add_array<V>(self, inner: Vec<V>) -> Self
    where
        V: Into<Self>,
    {
        let map = inner.into_iter().enumerate().fold(
            BTreeMap::<Index, _>::default(),
            |mut acc, (i, td)| {
                acc.insert(i.into(), td.into());
                acc
            },
        );

        self.add_array_mapped(map)
    }

    #[inline]
    pub fn restrict_array(mut self) -> Self {
        match self.kind {
            KindInfo::Known(set) => {
                self.kind = KindInfo::Known(
                    set.into_iter()
                        .filter(|k| matches!(k, TypeKind::Array(_)))
                        .collect(),
                );
                self
            }
            KindInfo::Unknown => self.array_mapped::<(), Kind>(map! { (): Kind::all() }),
        }
    }

    #[inline]
    pub fn array_mapped<I, V>(self, map: BTreeMap<I, V>) -> Self
    where
        I: Into<Index>,
        V: Into<Self>,
    {
        self.unknown().add_array_mapped(map)
    }

    #[inline]
    pub fn add_array_mapped<I, V>(mut self, map: BTreeMap<I, V>) -> Self
    where
        I: Into<Index>,
        V: Into<Self>,
    {
        // must not have multiple arrays in list
        self = self.remove_array();

        let map = map
            .into_iter()
            .map(|(i, td)| (i.into(), td.into().kind))
            .collect::<BTreeMap<_, _>>();

        self.add_container(TypeKind::Array(map))
    }

    #[inline]
    pub fn object<K, V>(self, inner: BTreeMap<K, V>) -> Self
    where
        K: Into<Field>,
        V: Into<Self>,
    {
        self.unknown().add_object(inner)
    }

    #[inline]
    pub fn add_object<K, V>(mut self, inner: BTreeMap<K, V>) -> Self
    where
        K: Into<Field>,
        V: Into<Self>,
    {
        // must not have multiple objects in list
        self = self.remove_object();

        let map = inner
            .into_iter()
            .fold(BTreeMap::default(), |mut acc, (k, td)| {
                acc.insert(k.into(), td.into().kind);
                acc
            });

        self.add_container(TypeKind::Object(map))
    }

    #[inline]
    pub fn restrict_object(mut self) -> Self {
        match self.kind {
            KindInfo::Known(set) => {
                self.kind = KindInfo::Known(
                    set.into_iter()
                        .filter(|k| matches!(k, TypeKind::Object(_)))
                        .collect(),
                );
                self
            }
            KindInfo::Unknown => self.object::<(), Kind>(map! { (): Kind::all() }),
        }
    }

    fn add_container(mut self, kind: TypeKind) -> Self {
        debug_assert!(matches!(kind, TypeKind::Array(_) | TypeKind::Object(_)));

        let mut set = BTreeSet::default();
        set.insert(kind);

        self.kind = self.kind.merge(KindInfo::Known(set), false, false);
        self
    }

    fn remove_array(mut self) -> Self {
        self.kind = match self.kind {
            KindInfo::Known(set) => KindInfo::Known(
                set.into_iter()
                    .filter(|k| !matches!(k, TypeKind::Array(_)))
                    .collect(),
            ),
            v => v,
        };

        self
    }

    fn remove_object(mut self) -> Self {
        self.kind = match self.kind {
            KindInfo::Known(set) => KindInfo::Known(
                set.into_iter()
                    .filter(|k| !matches!(k, TypeKind::Object(_)))
                    .collect(),
            ),
            v => v,
        };

        self
    }

    /// Collects any subtypes that can contain multiple indexed types (array, object) and collects them into
    /// a single type for all indexes.
    /// Used for functions that cant determine which indexes of a collection have been used in the result.
    pub fn collect_subtypes(mut self) -> Self {
        self.kind = match self.kind {
            KindInfo::Known(set) => {
                KindInfo::Known(set.into_iter().map(|k| k.collect_kinds()).collect())
            }
            v => v,
        };

        self
    }

    #[inline]
    pub fn is_unknown(&self) -> bool {
        matches!(self.kind, KindInfo::Unknown)
    }

    #[inline]
    pub fn is_bytes(&self) -> bool {
        self.is("bytes")
    }

    #[inline]
    pub fn is_integer(&self) -> bool {
        self.is("integer")
    }

    #[inline]
    pub fn is_float(&self) -> bool {
        self.is("float")
    }

    #[inline]
    pub fn is_boolean(&self) -> bool {
        self.is("boolean")
    }

    #[inline]
    pub fn is_timestamp(&self) -> bool {
        self.is("timestamp")
    }

    #[inline]
    pub fn is_regex(&self) -> bool {
        self.is("regex")
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        self.is("null")
    }

    #[inline]
    pub fn is_array(&self) -> bool {
        self.is("array")
    }

    #[inline]
    pub fn is_object(&self) -> bool {
        self.is("object")
    }

    fn is(&self, kind: &'static str) -> bool {
        match &self.kind {
            KindInfo::Unknown => false,
            KindInfo::Known(set) if set.len() == 1 => {
                let v = set.iter().next().unwrap();
                match kind {
                    "bytes" => matches!(v, TypeKind::Bytes),
                    "integer" => matches!(v, TypeKind::Integer),
                    "float" => matches!(v, TypeKind::Float),
                    "boolean" => matches!(v, TypeKind::Boolean),
                    "timestamp" => matches!(v, TypeKind::Timestamp),
                    "regex" => matches!(v, TypeKind::Regex),
                    "null" => matches!(v, TypeKind::Null),
                    "array" => matches!(v, TypeKind::Array(_)),
                    "object" => matches!(v, TypeKind::Object(_)),
                    _ => unreachable!("implementation error"),
                }
            }
            KindInfo::Known(_) => false,
        }
    }

    // -------------------------------------------------------------------------

    pub fn has_kind(&self, kind: impl Into<Kind>) -> bool {
        self.kind().intersects(kind.into())
    }

    // -------------------------------------------------------------------------

    pub fn is_fallible(&self) -> bool {
        self.fallible
    }

    pub fn is_infallible(&self) -> bool {
        !self.is_fallible()
    }

    /// Set the type definition to be fallible if its kind is not contained
    /// within the provided kind.
    pub fn fallible_unless(mut self, kind: impl Into<Kind>) -> Self {
        let kind = kind.into();
        if !kind.contains(self.kind()) {
            self.fallible = true
        }

        self
    }

    fn update_segment<'a, I>(
        kind: &KindInfo,
        mut path: std::iter::Peekable<I>,
        newkind: &KindInfo,
    ) -> KindInfo
    where
        I: std::iter::Iterator<Item = &'a SegmentBuf> + Clone,
    {
        let next_segment = |path: &mut std::iter::Peekable<I>, kindinfo| {
            // Keep recursing until we reach the end of the path at which point we take the new
            // kind.
            if path.peek().is_some() {
                Self::update_segment(kindinfo, path.clone(), newkind)
            } else {
                newkind.clone()
            }
        };

        match kind {
            KindInfo::Unknown => {
                // The kind is unknown and thus there is nothing to remove.
                kind.clone()
            }
            KindInfo::Known(kinds) => KindInfo::Known(
                kinds
                    .iter()
                    .map(|kind| match (kind, path.next()) {
                        (TypeKind::Object(object), Some(SegmentBuf::Field(fieldname))) => {
                            // Is there an exact fieldname specified for the given path field?
                            let indexed = object.iter().any(|(field, _)| {
                                matches!(field,
                                    Field::Field(field) if field == fieldname.as_str())
                            });

                            TypeKind::Object(
                                object
                                    .iter()
                                    .flat_map(|(field, kindinfo)| match field {
                                        Field::Field(f) if f == fieldname.as_str() => {
                                            vec![(field.clone(), next_segment(&mut path, kindinfo))]
                                                .into_iter()
                                        }

                                        Field::Any if !indexed => {
                                            // If this specific field wasn't defined for the
                                            // object, we want to retain the original Any
                                            // definition and insert our new specific field
                                            // definition.
                                            vec![
                                                (field.clone(), kindinfo.clone()),
                                                (
                                                    Field::Field(fieldname.as_str().to_string()),
                                                    next_segment(&mut path, kindinfo),
                                                ),
                                            ]
                                            .into_iter()
                                        }

                                        _ => vec![(field.clone(), kindinfo.clone())].into_iter(),
                                    })
                                    .collect(),
                            )
                        }

                        (TypeKind::Object(object), Some(SegmentBuf::Coalesce(fieldnames))) => {
                            TypeKind::Object(
                                object
                                    .iter()
                                    .map(|(field, kindinfo)| {
                                        let kind = next_segment(&mut path, kindinfo);

                                        match field {
                                            Field::Field(ref name)
                                                if fieldnames.iter().any(|fieldname| {
                                                    name == fieldname.as_str()
                                                }) =>
                                            {
                                                // Coalesced fields can also be Null if the coalesced value
                                                // goes down the other branch.
                                                (field.clone(), kind.or_null())
                                            }
                                            _ => (field.clone(), kind),
                                        }
                                    })
                                    .collect(),
                            )
                        }

                        (TypeKind::Array(array), Some(SegmentBuf::Index(index))) => {
                            // Is an exact index type definition specified for the given path
                            // segment?
                            let indexed = array.iter().any(|(idx, _kindinfo)| {
                                matches!(idx,
                                    Index::Index(idx) if *index > 0 && *idx == *index as usize)
                            });

                            TypeKind::Array(
                                array
                                    .iter()
                                    .flat_map(|(idx, kindinfo)| match idx {
                                        Index::Index(idx)
                                            if *index >= 0 && *idx == *index as usize =>
                                        {
                                            vec![(
                                                Index::Index(*idx),
                                                next_segment(&mut path, kindinfo),
                                            )]
                                            .into_iter()
                                        }
                                        _ if *index < 0 => {
                                            // If we have specified a negative index we need to merge this
                                            // type since we aren't sure the precise index this is
                                            // specifying since it's dependant on the array length
                                            // at runtime.
                                            vec![(
                                                *idx,
                                                kindinfo.clone().merge(
                                                    next_segment(&mut path, kindinfo),
                                                    false,
                                                    true,
                                                ),
                                            )]
                                            .into_iter()
                                        }
                                        Index::Any if !indexed => {
                                            // If there is an Any type specified, and we know that
                                            // there isn't a type specified for the index provided
                                            // in the type, we need to add a specific typedef just
                                            // for the index.
                                            vec![
                                                (
                                                    Index::Index(*index as usize),
                                                    next_segment(&mut path, kindinfo),
                                                ),
                                                (*idx, kindinfo.clone()),
                                            ]
                                            .into_iter()
                                        }
                                        _ => vec![(*idx, kindinfo.clone())].into_iter(),
                                    })
                                    .collect(),
                            )
                        }

                        (kind, _) => kind.clone(),
                    })
                    .collect(),
            ),
        }
    }

    /// Updates the type definition at the given path with the provided kind.
    pub fn update_path(&self, path: &LookupBuf, kind: &KindInfo) -> Self {
        let segments = path.as_segments();
        let peekable = segments.iter().peekable();
        let kind = Self::update_segment(&self.kind, peekable, kind);

        TypeDef {
            fallible: self.fallible,
            kind,
        }
    }

    /// Performs a bitwise-or operation, and returns the resulting type definition.
    pub fn merge(self, rhs: Self) -> Self {
        Self {
            fallible: self.fallible | rhs.fallible,
            kind: self.kind.merge(rhs.kind, false, false),
        }
    }

    /// Performs a shallow bitwise-or operation, and returns the resulting type
    /// definition.
    pub fn merge_shallow(self, rhs: Self) -> Self {
        Self {
            fallible: self.fallible | rhs.fallible,
            kind: self.kind.merge(rhs.kind, true, false),
        }
    }

    /// Merge two type definitions, where the rhs type definition should
    /// overwrite any conflicting values in the lhs type definition.
    pub fn merge_overwrite(self, rhs: Self) -> Self {
        Self {
            fallible: self.fallible | rhs.fallible,
            kind: self.kind.merge(rhs.kind, false, true),
        }
    }

    /// For any array defined by this type def, allows you to map the indexed kind
    /// to a new kind.
    pub fn map_array<F>(&self, f: F) -> Self
    where
        F: Fn(&KindInfo) -> KindInfo,
    {
        let newkind = self.kind.map(|k| match k {
            TypeKind::Array(array) => TypeKind::Array(
                array
                    .iter()
                    .map(|(index, kind)| (*index, f(kind)))
                    .collect(),
            ),
            k => k.clone(),
        });

        Self {
            fallible: self.fallible,
            kind: newkind,
        }
    }

    fn remove_segment<'a, I>(kind: &KindInfo, mut path: std::iter::Peekable<I>) -> KindInfo
    where
        I: std::iter::Iterator<Item = &'a SegmentBuf> + Clone,
    {
        match kind {
            KindInfo::Unknown => {
                // The kind is unknown and thus there is nothing to remove.
                kind.clone()
            }
            KindInfo::Known(kinds) => KindInfo::Known(
                kinds
                    .iter()
                    .map(|kind| match (kind, path.next(), path.peek()) {
                        (TypeKind::Object(object), Some(SegmentBuf::Field(fieldname)), Some(_)) => {
                            TypeKind::Object(
                                object
                                    .iter()
                                    .map(|(field, kindinfo)| match field {
                                        Field::Field(name) if name == fieldname.as_str() => (
                                            field.clone(),
                                            Self::remove_segment(kindinfo, path.clone()),
                                        ),
                                        _ => (field.clone(), kindinfo.clone()),
                                    })
                                    .collect(),
                            )
                        }

                        (TypeKind::Object(object), Some(SegmentBuf::Field(fieldname)), None) => {
                            TypeKind::Object(
                                object
                                    .iter()
                                    .filter_map(|(field, kindinfo)| match field {
                                        Field::Field(name) if name == fieldname.as_str() => None,
                                        _ => Some((field.clone(), kindinfo.clone())),
                                    })
                                    .collect(),
                            )
                        }

                        (
                            TypeKind::Object(object),
                            Some(SegmentBuf::Coalesce(fieldnames)),
                            Some(_),
                        ) => TypeKind::Object(
                            object
                                .iter()
                                .map(|(field, kindinfo)| match field {
                                    Field::Field(name)
                                        if fieldnames
                                            .iter()
                                            .any(|fieldname| fieldname.as_str() == name) =>
                                    {
                                        (
                                            field.clone(),
                                            Self::remove_segment(kindinfo, path.clone()),
                                        )
                                    }
                                    _ => (field.clone(), kindinfo.clone()),
                                })
                                .collect(),
                        ),

                        (
                            TypeKind::Object(object),
                            Some(SegmentBuf::Coalesce(fieldnames)),
                            None,
                        ) => TypeKind::Object(
                            object
                                .iter()
                                .filter_map(|(field, kindinfo)| match field {
                                    Field::Field(name)
                                        if fieldnames
                                            .iter()
                                            .any(|fieldname| fieldname.as_str() == name) =>
                                    {
                                        // With coalesced paths we need to remove all fields within
                                        // the coalesce.
                                        None
                                    }
                                    _ => Some((field.clone(), kindinfo.clone())),
                                })
                                .collect(),
                        ),

                        (TypeKind::Array(array), Some(SegmentBuf::Index(index)), Some(_)) => {
                            TypeKind::Array(
                                array
                                    .iter()
                                    .map(|(idx, kindinfo)| match idx {
                                        Index::Index(idx)
                                            if *index >= 0 && *idx == *index as usize =>
                                        {
                                            (
                                                Index::Index(*idx),
                                                Self::remove_segment(kindinfo, path.clone()),
                                            )
                                        }
                                        _ => (*idx, kindinfo.clone()),
                                    })
                                    .collect(),
                            )
                        }

                        (TypeKind::Array(array), Some(SegmentBuf::Index(index)), None) => {
                            TypeKind::Array(
                                array
                                    .iter()
                                    .filter_map(|(idx, kindinfo)| match idx {
                                        Index::Index(idx)
                                            if *index >= 0 && *idx == *index as usize =>
                                        {
                                            None
                                        }
                                        Index::Index(idx)
                                            if *index >= 0 && *idx > *index as usize =>
                                        {
                                            // Elements after the removed index need the index
                                            // shifting down one.
                                            Some((Index::Index(idx - 1), kindinfo.clone()))
                                        }
                                        Index::Index(_) if *index < 0 => {
                                            // After attempting to delete an element in an array
                                            // with a negative index we can no longer maintain the
                                            // type definition of any specific element in the array
                                            // as we don't know which precise index is deleted
                                            // until runtime.
                                            None
                                        }
                                        _ => Some((*idx, kindinfo.clone())),
                                    })
                                    .collect(),
                            )
                        }

                        (kind, _, _) => kind.clone(),
                    })
                    .collect(),
            ),
        }
    }

    /// Removes the given path from the typedef
    pub fn remove_path(&self, path: &LookupBuf) -> Self {
        let segments = path.as_segments();
        let peekable = segments.iter().peekable();

        TypeDef {
            fallible: self.fallible,
            kind: Self::remove_segment(&self.kind, peekable),
        }
    }
}

impl Default for TypeDef {
    fn default() -> Self {
        Self {
            fallible: false,
            kind: KindInfo::Unknown,
        }
    }
}

impl From<Kind> for TypeDef {
    fn from(kind: Kind) -> Self {
        Self {
            fallible: false,
            kind: kind.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use lookup::{FieldBuf, SegmentBuf};
    use vector_common::btreemap;

    use super::*;
    use crate::type_def;

    #[test]
    fn collect_subtypes() {
        let kind = TypeKind::Array({
            let mut set1 = BTreeSet::new();
            set1.insert(TypeKind::Integer);
            let mut set2 = BTreeSet::new();
            set2.insert(TypeKind::Bytes);

            let mut map = BTreeMap::new();
            map.insert(Index::Index(1), KindInfo::Known(set1));
            map.insert(Index::Index(2), KindInfo::Known(set2));
            map
        });

        let kind = kind.collect_kinds();

        let expected = TypeKind::Array({
            let mut set = BTreeSet::new();
            set.insert(TypeKind::Integer);
            set.insert(TypeKind::Bytes);

            let mut map = BTreeMap::new();
            map.insert(Index::Any, KindInfo::Known(set));
            map
        });

        assert_eq!(kind, expected);
    }

    #[test]
    fn update_path() {
        struct TestCase {
            old: TypeDef,
            path: &'static str,
            new: TypeDef,
        }

        let cases = vec![
            // Simple case
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { array [
                        type_def! { object {
                            "noog" => type_def! { bytes },
                            "nork" => type_def! { bytes },
                        } },
                    ] },
                } },
                path: ".nonk",
                new: type_def! { object {
                    "nonk" => type_def! { bytes },
                } },
            },
            // Same field in different branches
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { object {
                        "shnoog" => type_def! { array [
                            type_def! { object {
                                "noog" => type_def! { bytes },
                            } },
                        ] },
                    } },
                    "nink" => type_def! { object {
                        "shnoog" => type_def! { array [
                            type_def! { object {
                                "noog" => type_def! { bytes },
                            } },
                        ] },
                    } },
                } },
                path: "nonk.shnoog",
                new: type_def! { object {
                    "nonk" => type_def! { object {
                        "shnoog" => type_def! { bytes },
                    } },
                    "nink" => type_def! { object {
                        "shnoog" => type_def! { array [
                            type_def! { object {
                                "noog" => type_def! { bytes },
                            } },
                        ] },
                    } },
                } },
            },
            // Indexed any should add the new type as a specific index and retain the any.
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { array [
                        type_def! { object {
                            "noog" => type_def! { array [
                                type_def! { bytes },
                            ] },
                            "nork" => type_def! { bytes },
                        } },
                    ] },
                } },
                path: ".nonk[0].noog",
                new: type_def! { object {
                    "nonk" => type_def! { array {
                        Index::Any => type_def! { object {
                            "noog" => type_def! { array [
                                type_def! { bytes },
                            ] },
                            "nork" => type_def! { bytes },
                        } },
                        Index::Index(0) => type_def! { object {
                            "noog" => type_def! { bytes },
                            "nork" => type_def! { bytes },
                        } },
                    } },
                } },
            },
            // Indexed specific
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { array {
                        Index::Index(0) => type_def! { object {
                            "noog" => type_def! { array [
                                type_def! { bytes },
                            ] },
                            "nork" => type_def! { bytes },
                        } },
                    } },
                } },
                path: ".nonk[0].noog",
                new: type_def! { object {
                    "nonk" => type_def! { array {
                        Index::Index(0) => type_def! { object {
                            "noog" => type_def! { bytes },
                            "nork" => type_def! { bytes },
                        } },
                    } },
                } },
            },
            // More nested
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { object {
                        "shnoog" => type_def! { array [
                            type_def! { object {
                                "noog" => type_def! { bytes },
                                "nork" => type_def! { bytes },
                            } },
                        ] },
                    } },
                } },
                path: ".nonk.shnoog",
                new: type_def! { object {
                    "nonk" => type_def! { object {
                        "shnoog" => type_def! { bytes },
                    } },
                } },
            },
            // Coalesce
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { object {
                        "shnoog" => type_def! { array [
                            type_def! { object {
                                "noog" => type_def! { bytes },
                                "nork" => type_def! { bytes },
                            } },
                        ] },
                    } },
                } },
                path: ".(nonk | nork).shnoog",
                new: type_def! { object {
                    "nonk" => type_def! { object {
                        "shnoog" => type_def! { bytes },
                    } }.add_null(),
                } },
            },
        ];

        let newkind = KindInfo::Known(std::iter::once(TypeKind::Bytes).collect());
        for case in cases {
            let path = LookupBuf::from_str(case.path).unwrap();
            let new = case.old.update_path(&path, &newkind);
            assert_eq!(case.new, new, "{}", path);
        }
    }

    #[test]
    fn remove_path() {
        struct TestCase {
            old: TypeDef,
            path: &'static str,
            new: TypeDef,
        }

        let cases = vec![
            // A field is removed.
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { array {
                        0 => type_def! { object {
                            "noog" => type_def! { bytes },
                            "nork" => type_def! { bytes },
                        } },
                    } },
                } },
                path: "nonk[0].noog",
                new: type_def! { object {
                    "nonk" => type_def! { array {
                        0 => type_def! { object {
                            "nork" => type_def! { bytes },
                        } },
                    } },
                } },
            },
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { object {
                        "nork" => type_def! { bytes },
                        "nark" => type_def! { bytes },
                    } },
                    "noog" => type_def! { object {
                        "nork" => type_def! { bytes },
                        "nark" => type_def! { bytes },
                    } },
                } },
                path: "nonk.nork",
                new: type_def! { object {
                    "nonk" => type_def! { object {
                        "nark" => type_def! { bytes },
                    } },
                    "noog" => type_def! { object {
                        "nork" => type_def! { bytes },
                        "nark" => type_def! { bytes },
                    } },
                } },
            },
            // Coalesced field
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { object {
                        "nork" => type_def! { bytes },
                        "noog" => type_def! { bytes },
                        "nonk" => type_def! { bytes },
                    } },
                } },
                path: "nonk.(noog | nonk)",
                new: type_def! { object {
                    "nonk" => type_def! { object {
                        "nork" => type_def! { bytes },
                    } },
                } },
            },
            // A single array element when the typedef is for any index.
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { array [
                        type_def! { object {
                            "noog" => type_def! { bytes },
                            "nork" => type_def! { bytes },
                        } },
                    ] },
                } },
                path: "nonk[0]",
                new: type_def! { object {
                    "nonk" => type_def! { array [
                        type_def! { object {
                            "noog" => type_def! { bytes },
                            "nork" => type_def! { bytes },
                        } },
                    ] },
                } },
            },
            // A single array element when the typedef is for that index.
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { array {
                        0 => type_def! { object {
                            "noog" => type_def! { bytes },
                            "nork" => type_def! { bytes },
                        } },
                    } },
                } },
                path: "nonk[0]",
                new: type_def! { object {
                    "nonk" => type_def! { array },
                } },
            },
            // A single array element with a negative index has to remove all typedefs.
            TestCase {
                old: type_def! { object {
                    "nonk" => type_def! { array {
                        0 => type_def! { object {
                            "noog" => type_def! { bytes },
                            "nork" => type_def! { bytes },
                        } },
                    } },
                } },
                path: "nonk[-1]",
                new: type_def! { object {
                    "nonk" => type_def! { array },
                } },
            },
        ];

        for case in cases {
            let path = LookupBuf::from_str(case.path).unwrap();
            assert_eq!(case.new, case.old.remove_path(&path));
        }
    }

    mod kind_info {
        use super::*;

        #[test]
        fn for_path() {
            struct TestCase {
                info: KindInfo,
                path: Vec<SegmentBuf>,
                want: KindInfo,
            }

            let cases: Vec<TestCase> = vec![
                // overwrite unknown
                TestCase {
                    info: KindInfo::Unknown,
                    path: vec![SegmentBuf::Index(0)],
                    want: KindInfo::Known({
                        let mut map = BTreeMap::new();
                        map.insert(Index::Index(0), KindInfo::Unknown);

                        let mut set = BTreeSet::new();
                        set.insert(TypeKind::Array(map));
                        set
                    }),
                },
                // insert scalar at root
                TestCase {
                    info: KindInfo::Known({
                        let mut set = BTreeSet::new();
                        set.insert(TypeKind::Integer);
                        set
                    }),
                    path: vec![],
                    want: KindInfo::Known({
                        let mut set = BTreeSet::new();
                        set.insert(TypeKind::Integer);
                        set
                    }),
                },
                // insert scalar at nested path
                TestCase {
                    info: KindInfo::Known({
                        let mut set = BTreeSet::new();
                        set.insert(TypeKind::Integer);
                        set
                    }),
                    path: vec![SegmentBuf::Field(FieldBuf::from("foo"))],
                    want: KindInfo::Known({
                        let map = {
                            let mut set = BTreeSet::new();
                            set.insert(TypeKind::Integer);

                            let mut map = BTreeMap::new();
                            map.insert(Field::Field("foo".to_owned()), KindInfo::Known(set));
                            map
                        };

                        let mut set = BTreeSet::new();
                        set.insert(TypeKind::Object(map));
                        set
                    }),
                },
                // insert non-negative index
                TestCase {
                    info: KindInfo::Known({
                        let mut set = BTreeSet::new();
                        set.insert(TypeKind::Integer);
                        set
                    }),
                    path: vec![SegmentBuf::Index(1)],
                    want: KindInfo::Known({
                        let map = {
                            let mut set = BTreeSet::new();
                            set.insert(TypeKind::Integer);

                            let mut map = BTreeMap::new();
                            map.insert(Index::Index(1), KindInfo::Known(set));
                            map
                        };

                        let mut set = BTreeSet::new();
                        set.insert(TypeKind::Array(map));
                        set
                    }),
                },
                // insert negative index
                TestCase {
                    info: KindInfo::Known({
                        let mut set = BTreeSet::new();
                        set.insert(TypeKind::Integer);
                        set
                    }),
                    path: vec![SegmentBuf::Index(-1)],
                    want: KindInfo::Known({
                        let mut set = BTreeSet::new();
                        set.insert(TypeKind::Array({
                            let mut map = BTreeMap::new();
                            map.insert(Index::Any, KindInfo::Unknown);
                            map
                        }));
                        set
                    }),
                },
            ];

            for TestCase { info, path, want } in cases {
                let path = LookupBuf::from_segments(path);

                assert_eq!(info.for_path(path), want);
            }
        }
    }
}
