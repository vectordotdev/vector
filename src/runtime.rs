use std::io;

pub struct Runtime {
	rt: tokio::runtime::Runtime,
}

impl Runtime {
    pub fn new() -> io::Result<Self> {
        Ok(Runtime{
            rt: tokio::runtime::Runtime::new()
        })
    }
}