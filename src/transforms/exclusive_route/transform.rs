use vector_lib::transform::SyncTransform;

use crate::conditions::Condition;
use crate::transforms::exclusive_route::config::{ExclusiveRouteConfig, UNMATCHED_ROUTE};
use crate::transforms::TransformOutputsBuf;
use crate::{config::TransformContext, event::Event};

#[derive(Clone)]
pub struct ResolvedRoute {
    name: String,
    condition: Condition,
}

#[derive(Clone)]
pub struct ExclusiveRoute {
    routes: Vec<ResolvedRoute>,
}

impl ExclusiveRoute {
    pub fn new(config: &ExclusiveRouteConfig, context: &TransformContext) -> crate::Result<Self> {
        let resolved_routes = config
            .routes
            .iter()
            .map(|route| {
                let condition = route.condition.build(&context.enrichment_tables)?;
                Ok(ResolvedRoute {
                    name: route.name.clone(),
                    condition,
                })
            })
            .collect::<crate::Result<Vec<_>>>()?;

        Ok(Self {
            routes: resolved_routes,
        })
    }
}

impl SyncTransform for ExclusiveRoute {
    fn transform(&mut self, event: Event, output: &mut TransformOutputsBuf) {
        for route in &self.routes {
            let (result, event) = route.condition.check(event.clone());
            if result {
                output.push(Some(&route.name), event);
                return;
            }
        }

        output.push(Some(UNMATCHED_ROUTE), event);
    }
}
