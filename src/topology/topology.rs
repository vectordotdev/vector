// use std::collections::HashMap;
use futures::Future;
use stream_cancel::Trigger;

pub struct Topology {
    // sinks: HashMap<String,
    server: Option<Box<dyn Future<Item = (), Error = ()> + Send>>,
    healthcheck: Option<Box<dyn Future<Item = (), Error = ()> + Send>>,
    trigger: Option<Trigger>,
}

impl Topology {
    pub fn build(config: super::Config) -> Result<(Self, Vec<String>), Vec<String>> {
        let (server, trigger, healthcheck, warnings) = super::builder::build(config)?;

        let topology = Self {
            server: Some(Box::new(server)),
            healthcheck: Some(Box::new(healthcheck)),
            trigger: Some(trigger),
        };

        Ok((topology, warnings))
    }

    pub fn healthchecks(&mut self) -> impl Future<Item = (), Error = ()> {
        self.healthcheck.take().unwrap()
    }

    pub fn start(&mut self, rt: &mut tokio::runtime::Runtime) {
        let server = self.server.take().unwrap();
        rt.spawn(server);
    }

    pub fn stop(&mut self) {
        drop(self.trigger.take().unwrap())
    }
}
