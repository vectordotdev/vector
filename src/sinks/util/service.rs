use futures::Poll;
use std::sync::Arc;
use tower::layer::util::Stack;
use tower::{layer::Layer, Service, ServiceBuilder};

pub trait ServiceBuilderExt<L> {
    fn map<R1, R2, F>(self, f: F) -> ServiceBuilder<Stack<MapLayer<R1, R2>, L>>
    where
        F: Fn(R1) -> R2 + Send + Sync + 'static;
}

impl<L> ServiceBuilderExt<L> for ServiceBuilder<L> {
    fn map<R1, R2, F>(self, f: F) -> ServiceBuilder<Stack<MapLayer<R1, R2>, L>>
    where
        F: Fn(R1) -> R2 + Send + Sync + 'static,
    {
        self.layer(MapLayer { f: Arc::new(f) })
    }
}

pub struct MapLayer<R1, R2> {
    f: Arc<dyn Fn(R1) -> R2 + Send + Sync + 'static>,
}

impl<S, R1, R2> Layer<S> for MapLayer<R1, R2>
where
    S: Service<R2>,
{
    type Service = Map<S, R1, R2>;

    fn layer(&self, inner: S) -> Self::Service {
        Map {
            f: self.f.clone(),
            inner,
        }
    }
}

pub struct Map<S, R1, R2> {
    f: Arc<dyn Fn(R1) -> R2 + Send + Sync + 'static>,
    inner: S,
}

impl<S, R1, R2> Service<R1> for Map<S, R1, R2>
where
    S: Service<R2>,
    S::Error: Into<crate::Error> + Send + Sync + 'static,
{
    type Response = S::Response;
    type Error = crate::Error;
    type Future = futures::future::MapErr<S::Future, fn(S::Error) -> crate::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, req: R1) -> Self::Future {
        let req = (self.f)(req);
        use futures::Future;
        self.inner.call(req).map_err(|e| e.into())
    }
}

impl<S: Clone, R1, R2> Clone for Map<S, R1, R2> {
    fn clone(&self) -> Self {
        Self {
            f: self.f.clone(),
            inner: self.inner.clone(),
        }
    }
}
