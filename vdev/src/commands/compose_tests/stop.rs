use anyhow::Result;

use crate::testing::{integration::ComposeTestT, state::EnvsDir};

pub(crate) fn exec<T: ComposeTestT>(test_name: &str, all_features: bool) -> Result<()> {
    if let Some(active) = EnvsDir::new(test_name).active()? {
        T::stop(&T::generate(test_name, active, all_features, 0)?)
    } else {
        println!("No environment for {test_name} is active.");
        Ok(())
    }
}
