









pub use value::Value;



// #[derive(PartialOrd, Debug, Clone, Deserialize)]
// pub enum Value {
//     Bytes(Bytes),
//     Integer(i64),
//     Float(f64),
//     Boolean(bool),
//     Timestamp(DateTime<Utc>),
//     Map(BTreeMap<String, Value>),
//     Array(Vec<Value>),
//     Null,
// }
//
// impl Eq for Value {}
//

//
// impl Hash for Value {
//     fn hash<H: Hasher>(&self, state: &mut H) {
//         core::mem::discriminant(self).hash(state);
//         match self {
//             Value::Array(v) => {
//                 v.hash(state);
//             }
//             Value::Boolean(v) => {
//                 v.hash(state);
//             }
//             Value::Bytes(v) => {
//                 v.hash(state);
//             }
//             Value::Float(v) => {
//                 // This hashes floats with the following rules:
//                 // * NaNs hash as equal (covered by above discriminant hash)
//                 // * Positive and negative infinity has to different values
//                 // * -0 and +0 hash to different values
//                 // * otherwise transmute to u64 and hash
//                 if v.is_finite() {
//                     v.is_sign_negative().hash(state);
//                     let trunc: u64 = unsafe { std::mem::transmute(v.trunc().to_bits()) };
//                     trunc.hash(state);
//                 } else if !v.is_nan() {
//                     v.is_sign_negative().hash(state);
//                 } //else covered by discriminant hash
//             }
//             Value::Integer(v) => {
//                 v.hash(state);
//             }
//             Value::Map(v) => {
//                 v.hash(state);
//             }
//             Value::Null => {
//                 //covered by discriminant hash
//             }
//             Value::Timestamp(v) => {
//                 v.hash(state);
//             }
//         }
//     }
// }
//

//

//

//

//

//

//

//

//

//
// impl Value {

//

//

//

//
//     /// Returns self as a `Vec<Value>`
//     ///
//     /// # Panics
//     ///
//     /// This function will panic if self is anything other than `Value::Array`.
//     pub fn as_array(&self) -> &Vec<Value> {
//         match self {
//             Value::Array(ref a) => a,
//             _ => panic!("Tried to call `Value::as_array` on a non-array value."),
//         }
//     }
//
//     /// Returns self as a mutable `Vec<Value>`
//     ///
//     /// # Panics
//     ///
//     /// This function will panic if self is anything other than `Value::Array`.
//     pub fn as_array_mut(&mut self) -> &mut Vec<Value> {
//         match self {
//             Value::Array(ref mut a) => a,
//             _ => panic!("Tried to call `Value::as_array` on a non-array value."),
//         }
//     }
//

//

//

//

//

//

//

//

//

//

//
//     /// Get a mutable borrow of the value by lookup.
//     ///
//     /// ```rust
//     /// use vector_core::event::Value;
//     /// use lookup::Lookup;
//     /// use std::collections::BTreeMap;
//     ///
//     /// let mut inner_map = Value::from(BTreeMap::default());
//     /// inner_map.insert("baz", 1);
//     ///
//     /// let mut map = Value::from(BTreeMap::default());
//     /// map.insert("bar", inner_map.clone());
//     ///
//     /// assert_eq!(map.get_mut("bar").unwrap(), Some(&mut Value::from(inner_map)));
//     ///
//     /// let lookup_key = Lookup::from_str("bar.baz").unwrap();
//     /// assert_eq!(map.get_mut(lookup_key).unwrap(), Some(&mut Value::from(1)));
//     /// ```
//     ///
//     /// # Panics
//     ///
//     /// This function may panic if an invariant is violated, indicating a
//     /// serious bug.
//     #[allow(clippy::missing_errors_doc)]
//     pub fn get_mut<'a>(
//         &mut self,
//         lookup: impl Into<Lookup<'a>> + Debug,
//     ) -> std::result::Result<Option<&mut Value>, EventError> {
//         let mut working_lookup = lookup.into();
//         let span = trace_span!("get_mut", lookup = %working_lookup);
//         let _guard = span.enter();
//
//         let this_segment = working_lookup.pop_front();
//         match (this_segment, self) {
//             // We've met an end and found our value.
//             (None, item) => Ok(Some(item)),
//             // This is just not allowed!
//             (_, Value::Boolean(_))
//             | (_, Value::Bytes(_))
//             | (_, Value::Timestamp(_))
//             | (_, Value::Float(_))
//             | (_, Value::Integer(_))
//             | (_, Value::Null) => unimplemented!(),
//             // Descend into a coalesce
//             (Some(Segment::Coalesce(sub_segments)), value) => {
//                 // Creating a needle with a back out of the loop is very important.
//                 let mut needle = None;
//                 for sub_segment in sub_segments {
//                     let mut lookup = Lookup::from(sub_segment);
//                     lookup.extend(working_lookup.clone()); // We need to include the rest of the get.
//                                                            // Notice we cannot take multiple mutable borrows in a loop, so we must pay the
//                                                            // contains cost extra. It's super unfortunate, hopefully future work can solve this.
//                     if value.contains(lookup.clone()) {
//                         needle = Some(lookup);
//                         break;
//                     }
//                 }
//                 match needle {
//                     Some(needle) => value.get_mut(needle),
//                     None => Ok(None),
//                 }
//             }
//             // Descend into a map
//             (Some(Segment::Field(Field { name, .. })), Value::Map(map)) => {
//                 match map.get_mut(name) {
//                     Some(inner) => inner.get_mut(working_lookup.clone()),
//                     None => Ok(None),
//                 }
//             }
//             (Some(Segment::Index(_)), Value::Map(_))
//             | (Some(Segment::Field(_)), Value::Array(_)) => Ok(None),
//             // Descend into an array
//             (Some(Segment::Index(i)), Value::Array(array)) => {
//                 let index = if i.is_negative() {
//                     if i.abs() > array.len() as isize {
//                         // The index is before the start of the array.
//                         return Ok(None);
//                     }
//                     (array.len() as isize + i) as usize
//                 } else {
//                     i as usize
//                 };
//
//                 match array.get_mut(index) {
//                     Some(inner) => inner.get_mut(working_lookup.clone()),
//                     None => Ok(None),
//                 }
//             }
//         }
//     }
//

//
//     /// Produce an iterator over all 'nodes' in the graph of this value.
//     ///
//     /// This includes leaf nodes as well as intermediaries.
//     ///
//     /// If provided a `prefix`, it will always produce with that prefix included, and all nodes
//     /// will be prefixed with that lookup.
//     ///
//     /// ```rust
//     /// use vector_core::event::Value;
//     /// use lookup::{Lookup, LookupBuf};
//     /// let plain_key = "lick";
//     /// let lookup_key = LookupBuf::from_str("vic.stick.slam").unwrap();
//     /// let mut value = Value::from(std::collections::BTreeMap::default());
//     /// value.insert(plain_key, 1);
//     /// value.insert(lookup_key, 2);
//     ///
//     /// let mut keys = value.lookups(None, false);
//     /// assert_eq!(keys.next(), Some(Lookup::root()));
//     /// assert_eq!(keys.next(), Some(Lookup::from_str("lick").unwrap()));
//     /// assert_eq!(keys.next(), Some(Lookup::from_str("vic").unwrap()));
//     /// assert_eq!(keys.next(), Some(Lookup::from_str("vic.stick").unwrap()));
//     /// assert_eq!(keys.next(), Some(Lookup::from_str("vic.stick.slam").unwrap()));
//     ///
//     /// let mut keys = value.lookups(None, true);
//     /// assert_eq!(keys.next(), Some(Lookup::from_str("lick").unwrap()));
//     /// assert_eq!(keys.next(), Some(Lookup::from_str("vic.stick.slam").unwrap()));
//     /// ```
//     #[instrument(level = "trace", skip(self, prefix, only_leaves))]
//     pub fn lookups<'a>(
//         &'a self,
//         prefix: Option<Lookup<'a>>,
//         only_leaves: bool,
//     ) -> Box<dyn Iterator<Item = Lookup<'a>> + 'a> {
//         match &self {
//             Value::Boolean(_)
//             | Value::Bytes(_)
//             | Value::Timestamp(_)
//             | Value::Float(_)
//             | Value::Integer(_)
//             | Value::Null => Box::new(prefix.into_iter()),
//             Value::Map(m) => {
//                 let this = prefix
//                     .clone()
//                     .or_else(|| Some(Lookup::default()))
//                     .into_iter();
//                 let children = m.iter().flat_map(move |(k, v)| {
//                     let lookup = prefix.clone().map_or_else(
//                         || Lookup::from(k),
//                         |mut l| {
//                             l.push_back(Segment::from(k.as_str()));
//                             l
//                         },
//                     );
//                     v.lookups(Some(lookup), only_leaves)
//                 });
//
//                 if only_leaves && !self.is_empty() {
//                     Box::new(children)
//                 } else {
//                     Box::new(this.chain(children))
//                 }
//             }
//             Value::Array(a) => {
//                 let this = prefix
//                     .clone()
//                     .or_else(|| Some(Lookup::default()))
//                     .into_iter();
//                 let children = a.iter().enumerate().flat_map(move |(k, v)| {
//                     let lookup = prefix.clone().map_or_else(
//                         || Lookup::from(k as isize),
//                         |mut l| {
//                             l.push_back(Segment::index(k as isize));
//                             l
//                         },
//                     );
//                     v.lookups(Some(lookup), only_leaves)
//                 });
//
//                 if only_leaves && !self.is_empty() {
//                     Box::new(children)
//                 } else {
//                     Box::new(this.chain(children))
//                 }
//             }
//         }
//     }

// /// Produce an iterator over all 'nodes' in the graph of this value.
// ///
// /// This includes leaf nodes as well as intermediaries.
// ///
// /// If provided a `prefix`, it will always produce with that prefix included, and all nodes
// /// will be prefixed with that lookup.
// ///
// /// ```rust
// /// use vector_core::event::Value;
// /// use lookup::{Lookup, LookupBuf};
// /// let plain_key = "lick";
// /// let lookup_key = LookupBuf::from_str("vic.stick.slam").unwrap();
// /// let mut value = Value::from(std::collections::BTreeMap::default());
// /// value.insert(plain_key, 1);
// /// value.insert(lookup_key, 2);
// ///
// /// let mut keys = value.pairs(None, false);
// /// assert_eq!(keys.next(), Some((Lookup::root(), &Value::from({
// ///     let mut inner_inner_map = std::collections::BTreeMap::default();
// ///     inner_inner_map.insert(String::from("slam"), Value::from(2));
// ///     let mut inner_map = std::collections::BTreeMap::default();
// ///     inner_map.insert(String::from("stick"), Value::from(inner_inner_map));
// ///     let mut map = std::collections::BTreeMap::default();
// ///     map.insert(String::from("vic"), Value::from(inner_map));
// ///     map.insert(String::from("lick"), Value::from(1));
// ///     map
// /// }))));
// /// assert_eq!(keys.next(), Some((Lookup::from_str("lick").unwrap(), &Value::from(1))));
// /// assert_eq!(keys.next(), Some((Lookup::from_str("vic").unwrap(), &Value::from({
// ///     let mut inner_map = std::collections::BTreeMap::default();
// ///     inner_map.insert(String::from("slam"), Value::from(2));
// ///     let mut map = std::collections::BTreeMap::default();
// ///     map.insert(String::from("stick"), Value::from(inner_map));
// ///     map
// /// }))));
// /// assert_eq!(keys.next(), Some((Lookup::from_str("vic.stick").unwrap(), &Value::from({
// ///     let mut map = std::collections::BTreeMap::default();
// ///     map.insert(String::from("slam"), Value::from(2));
// ///     map
// /// }))));
// /// assert_eq!(keys.next(), Some((Lookup::from_str("vic.stick.slam").unwrap(), &Value::from(2))));
// ///
// /// let mut keys = value.pairs(None, true);
// /// assert_eq!(keys.next(), Some((Lookup::from_str("lick").unwrap(), &Value::from(1))));
// /// assert_eq!(keys.next(), Some((Lookup::from_str("vic.stick.slam").unwrap(), &Value::from(2))));
// /// ```
// #[instrument(level = "trace", skip(self, prefix, only_leaves))]
// pub fn pairs<'a>(
//     &'a self,
//     prefix: Option<Lookup<'a>>,
//     only_leaves: bool,
// ) -> Box<dyn Iterator<Item = (Lookup<'a>, &'a Value)> + 'a> {
//     match &self {
//         Value::Boolean(_)
//         | Value::Bytes(_)
//         | Value::Timestamp(_)
//         | Value::Float(_)
//         | Value::Integer(_)
//         | Value::Null => Box::new(prefix.map(move |v| (v, self)).into_iter()),
//         Value::Map(m) => {
//             let this = prefix
//                 .clone()
//                 .or_else(|| Some(Lookup::default()))
//                 .map(|v| (v, self))
//                 .into_iter();
//             let children = m.iter().flat_map(move |(k, v)| {
//                 let lookup = prefix.clone().map_or_else(
//                     || Lookup::from(k),
//                     |mut l| {
//                         l.push_back(Segment::from(k.as_str()));
//                         l
//                     },
//                 );
//                 v.pairs(Some(lookup), only_leaves)
//             });
//
//             if only_leaves && !self.is_empty() {
//                 Box::new(children)
//             } else {
//                 Box::new(this.chain(children))
//             }
//         }
//         Value::Array(a) => {
//             let this = prefix
//                 .clone()
//                 .or_else(|| Some(Lookup::default()))
//                 .map(|v| (v, self))
//                 .into_iter();
//             let children = a.iter().enumerate().flat_map(move |(k, v)| {
//                 let lookup = prefix.clone().map_or_else(
//                     || Lookup::from(k as isize),
//                     |mut l| {
//                         l.push_back(Segment::index(k as isize));
//                         l
//                     },
//                 );
//                 v.pairs(Some(lookup), only_leaves)
//             });
//
//             if only_leaves && !self.is_empty() {
//                 Box::new(children)
//             } else {
//                 Box::new(this.chain(children))
//             }
//         }
//     }
// }
// }
