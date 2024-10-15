use vrl::prelude::*;

use crate::{caches::VrlCacheRegistry, vrl_util};

#[derive(Clone, Copy, Debug)]
pub struct CachePut;
impl Function for CachePut {
    fn identifier(&self) -> &'static str {
        "cache_put"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "cache",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "key",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "value",
                kind: kind::ANY,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "write to cache",
            source: r#"cache_put!("test_cache", "test_key", "test_value")"#,
            result: Ok(""),
        }]
    }

    fn compile(
        &self,
        state: &TypeState,
        ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let registry = ctx
            .get_external_context_mut::<VrlCacheRegistry>()
            .ok_or(Box::new(vrl_util::Error::CachesNotLoaded) as Box<dyn DiagnosticMessage>)?;

        let caches = registry
            .cache_ids()
            .into_iter()
            .map(Value::from)
            .collect::<Vec<_>>();

        let cache = arguments
            .required_enum("cache", &caches, state)?
            .try_bytes_utf8_lossy()
            .expect("cache is not valid utf8")
            .into_owned();
        let key = arguments.required("key");
        let value = arguments.required("value");

        Ok(CachePutFn {
            cache,
            key,
            value,
            registry: registry.clone(),
        }
        .as_expr())
    }
}

#[derive(Debug, Clone)]
pub struct CachePutFn {
    cache: String,
    key: Box<dyn Expression>,
    value: Box<dyn Expression>,
    registry: VrlCacheRegistry,
}

impl FunctionExpression for CachePutFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let key = self.key.resolve(ctx)?.try_bytes_utf8_lossy()?.into_owned();
        let value = self.value.resolve(ctx)?;
        self.registry
            .writer()
            .put_val(&self.cache, &key, value.clone());
        Ok(value)
    }

    fn type_def(&self, _: &TypeState) -> TypeDef {
        TypeDef::null().impure().fallible()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use vrl::compiler::prelude::TimeZone;
    use vrl::compiler::state::RuntimeState;
    use vrl::compiler::TargetValue;
    use vrl::value;
    use vrl::value::Secrets;

    use crate::caches::VrlCache;

    use super::*;

    fn get_cache_registry() -> VrlCacheRegistry {
        let registry = VrlCacheRegistry::default();
        registry.insert_caches(BTreeMap::from([("test".to_string(), VrlCache::default())]));
        registry
    }

    #[test]
    fn set_val() {
        let registry = get_cache_registry();
        let func = CachePutFn {
            cache: "test".to_string(),
            key: expr!("test_key"),
            value: expr!("test_value"),
            registry: registry.clone(),
        };

        let tz = TimeZone::default();
        let object: Value = BTreeMap::new().into();
        let mut target = TargetValue {
            value: object,
            metadata: value!({}),
            secrets: Secrets::new(),
        };
        let mut runtime_state = RuntimeState::default();
        let mut ctx = Context::new(&mut target, &mut runtime_state, &tz);

        let got = func.resolve(&mut ctx);

        assert_eq!(Ok(value!("test_value")), got);
        assert_eq!(
            value!("test_value"),
            registry
                .as_readonly()
                .get_val("test", "test_key")
                .unwrap()
                .clone()
        );
    }
}
