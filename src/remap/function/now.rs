use chrono::Utc;
use remap::prelude::*;

#[derive(Debug)]
pub struct Now;

impl Function for Now {
    fn identifier(&self) -> &'static str {
        "now"
    }

    fn compile(&self, _: ArgumentList) -> Result<Box<dyn Expression>> {
        Ok(Box::new(NowFn))
    }
}

#[derive(Debug)]
struct NowFn;

impl Expression for NowFn {
    fn execute(&self, _: &mut State, _: &mut dyn Object) -> Result<Option<Value>> {
        Ok(Some(Value::Timestamp(Utc::now()).into()))
    }
}
