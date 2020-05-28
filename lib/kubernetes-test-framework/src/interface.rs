use std::env;

#[derive(Debug)]
pub struct Interface {
    pub deploy_vector_command: String,
    pub kubectl_command: String,
}

impl Interface {
    pub fn from_env() -> Option<Self> {
        Some(Self {
            deploy_vector_command: env::var("KUBE_TEST_DEPLOY_COMMAND").ok()?,
            kubectl_command: env::var("VECTOR_TEST_KUBECTL")
                .unwrap_or_else(|_| "kubectl".to_owned()),
        })
    }
}
