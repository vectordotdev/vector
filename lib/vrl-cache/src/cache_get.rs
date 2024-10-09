use vrl::prelude::*;

use crate::{
    caches::{VrlCacheRegistry, VrlCacheSearch},
    vrl_util,
};

#[derive(Clone, Copy, Debug)]
pub struct CacheGet;
impl Function for CacheGet {
    fn identifier(&self) -> &'static str {
        "cache_get"
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
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "read from cache",
            source: r#"cache_get!("test_cache", "test_key")"#,
            result: Ok(r#""test_value""#),
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

        Ok(CacheGetFn {
            cache,
            key,
            registry: registry.as_readonly(),
        }
        .as_expr())
    }
}

#[derive(Debug, Clone)]
pub struct CacheGetFn {
    cache: String,
    key: Box<dyn Expression>,
    registry: VrlCacheSearch,
}

impl FunctionExpression for CacheGetFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let key = self.key.resolve(ctx)?.try_bytes_utf8_lossy()?.into_owned();

        Ok(self
            .registry
            .get_val(&self.cache, &key)
            .ok_or_else(|| format!("key not found in cache: {key}"))?)
    }

    fn type_def(&self, _: &TypeState) -> TypeDef {
        TypeDef::any().fallible()
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
    fn get_val() {
        let registry = get_cache_registry();
        registry
            .writer()
            .put_val("test", "test_key", Value::from("test_value"));
        let func = CacheGetFn {
            cache: "test".to_string(),
            key: expr!("test_key"),
            registry: registry.as_readonly(),
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
    }

    #[test]
    fn get_val_key_not_found() {
        let registry = get_cache_registry();
        let func = CacheGetFn {
            cache: "test".to_string(),
            key: expr!("test_key"),
            registry: registry.as_readonly(),
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

        assert_eq!(
            Err(ExpressionError::Error {
                message: "key not found in cache: test_key".to_string(),
                labels: vec![],
                notes: vec![]
            }),
            got
        );
    }
}
