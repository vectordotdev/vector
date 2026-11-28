use std::{collections::BTreeMap, fmt};

#[derive(Clone, Debug)]
pub struct TaskCompletedError {
    pub message: String,
    pub fields: BTreeMap<&'static str, String>,
}

impl TaskCompletedError {
    pub fn new(message: String, fields: impl IntoIterator<Item = (&'static str, String)>) -> Self {
        let fields = fields.into_iter().collect();
        Self { message, fields }
    }
}

impl fmt::Display for TaskCompletedError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "{:?}", self.message)?;
        let mut sep = " ";
        for field in &self.fields {
            write!(fmt, "{sep}{} = {:?}", field.0, field.1)?;
            sep = ", ";
        }
        Ok(())
    }
}
