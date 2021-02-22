use crate::value::Kind;
use crate::{map, path, Path};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Sub,
};

/// Properties for a given expression that express the expected outcome of the
/// expression.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TypeDef {
    /// True, if an expression can return an error.
    ///
    /// Some expressions are infallible (e.g. the [`Literal`] expression, or any
    /// custom function designed to be infallible).
    pub fallible: bool,

    /// The [`value::Kind`]s this definition represents.
    ///
    /// This is wrapped in a [`TypeKind`] enum, such that we encode details
    /// about potential inner kinds for collections (arrays or objects).
    kind: KindInfo,
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
enum KindInfo {
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
    pub fn for_path(mut self, path: Path) -> Self {
        use path::Segment;

        for segment in path.segments().iter().rev() {
            match segment {
                Segment::Field(field) => {
                    let mut map = BTreeMap::default();
                    map.insert(Field::Field(field.as_str().to_owned()), self);

                    let mut set = BTreeSet::new();
                    set.insert(TypeKind::Object(map));

                    self = KindInfo::Known(set);
                }
                Segment::Coalesce(fields) => {
                    let field = fields.last().unwrap();
                    let mut map = BTreeMap::default();
                    map.insert(Field::Field(field.as_str().to_owned()), self);

                    let mut set = BTreeSet::new();
                    set.insert(TypeKind::Object(map));

                    self = KindInfo::Known(set);
                }
                Segment::Index(index) => {
                    let index = *index as usize;
                    let mut map = BTreeMap::default();

                    let mut i = 0;
                    while i < index {
                        let mut set = BTreeSet::new();
                        set.insert(TypeKind::Null);
                        map.insert(Index::Index(i), Self::Known(set));
                        i += 1;
                    }

                    map.insert(Index::Index(index), self);

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
    pub fn at_path(&self, path: Path) -> Self {
        use path::Segment;

        let mut iter = path.into_iter();

        let info = match self {
            kind @ KindInfo::Unknown => return kind.clone(),
            kind @ KindInfo::Known(_) => {
                let new = match iter.next() {
                    None => return kind.clone(),
                    Some(segment) => match segment {
                        Segment::Coalesce(fields) => match kind.object() {
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
                        Segment::Field(field) => match kind.object() {
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
                        Segment::Index(index) => match kind.array() {
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

        info.at_path(iter.collect())
    }

    fn merge(self, rhs: Self, shallow: bool) -> Self {
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
                    .map(|(mut l, r)| {
                        let r_start = l
                            .keys()
                            .filter_map(|i| i.to_inner())
                            .max()
                            .map(|i| i + 1)
                            .unwrap_or_default();

                        let mut r = r
                            .into_iter()
                            .map(|(i, v)| (i.shift(r_start), v))
                            .collect::<BTreeMap<_, _>>();

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
                                        *v2 = v1.clone().merge(v2.clone(), false);
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
}

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
enum TypeKind {
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

    pub fn at_path(&self, path: Path) -> TypeDef {
        let fallible = self.fallible;
        let kind = self.kind.at_path(path);

        Self { fallible, kind }
    }

    pub fn for_path(self, path: Path) -> TypeDef {
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

        self.kind = self.kind.merge(kind.into(), false);
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

        self.kind = self.kind.merge(KindInfo::Known(set), false);
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

    /// Performs a bitwise-or operation, and returns the resulting type definition.
    pub fn merge(self, rhs: Self) -> Self {
        Self {
            fallible: self.fallible | rhs.fallible,
            kind: self.kind.merge(rhs.kind, false),
        }
    }

    /// Performs a shallow bitwise-or operation, and returns the resulting type
    /// definition.
    pub fn merge_shallow(self, rhs: Self) -> Self {
        Self {
            fallible: self.fallible | rhs.fallible,
            kind: self.kind.merge(rhs.kind, true),
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
