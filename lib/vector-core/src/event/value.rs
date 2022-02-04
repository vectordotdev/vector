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
