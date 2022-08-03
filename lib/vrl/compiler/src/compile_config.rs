use anymap::AnyMap;

pub struct CompileConfig {
    /// Custom context injected by the external environment
    custom: AnyMap,
}

impl CompileConfig {
    /// Get external context data from the external environment.
    pub fn get_external_context<T: 'static>(&self) -> Option<&T> {
        self.custom.get::<T>()
    }

    /// Swap the existing external contexts with new ones, returning the old ones.
    #[must_use]
    #[cfg(feature = "expr-function_call")]
    pub(crate) fn swap_external_context(&mut self, ctx: AnyMap) -> AnyMap {
        std::mem::replace(&mut self.custom, ctx)
    }

    /// Sets the external context data for VRL functions to use.
    pub fn set_external_context<T: 'static>(&mut self, data: T) {
        self.custom.insert::<T>(data);
    }

    pub fn custom_mut(&mut self) -> &mut AnyMap {
        &mut self.custom
    }
}

impl Default for CompileConfig {
    fn default() -> Self {
        Self {
            custom: AnyMap::new(),
        }
    }
}
