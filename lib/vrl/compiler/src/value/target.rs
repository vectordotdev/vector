use crate::{Target, Value};
use lookup::LookupBuf;

impl Target for Value {
    fn insert(&mut self, path: &LookupBuf, value: Value) -> Result<(), String> {
        self.insert_by_path(path, value);
        Ok(())
    }

    fn get(&self, path: &LookupBuf) -> Result<Option<Value>, String> {
        Ok(self.get_by_path(path).cloned())
    }

    fn remove(&mut self, path: &LookupBuf, compact: bool) -> Result<Option<Value>, String> {
        let value = Target::get(self, path)?;
        self.remove_by_path(path, compact);

        Ok(value)
    }
}
