use super::Transform;
use crate::record::Record;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AugmenterConfig {
    pub key: String,
    pub value: String,
}

pub struct Augmenter {
    key: Atom,
    value: String,
}

#[typetag::serde(name = "augmenter")]
impl crate::topology::config::TransformConfig for AugmenterConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        Ok(Box::new(Augmenter::new(
            self.key.clone(),
            self.value.clone(),
        )))
    }
}

impl Augmenter {
    pub fn new(key: String, value: String) -> Self {
        Augmenter {
            key: key.into(),
            value,
        }
    }
}

impl Transform for Augmenter {
    fn transform(&self, mut record: Record) -> Option<Record> {
        record.custom.insert(self.key.clone(), self.value.clone());
        Some(record)
    }
}

#[cfg(test)]
mod tests {
    use super::Augmenter;
    use crate::{record::Record, transforms::Transform};
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn augment_record() {
        let record = Record::from("augment me");
        let augment = Augmenter::new("some_key".into(), "some_val".into());

        let new_record = augment.transform(record).unwrap();

        let key = Atom::from("some_key".to_string());
        let kv = new_record.custom.get(&key);

        let val = "some_val".to_string();
        assert_eq!(kv, Some(&val));
    }
}
