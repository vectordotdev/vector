use buffer_lazy::BufferLazyLayer;
use tower::{layer::util::Stack, ServiceBuilder};

pub trait ServiceBuilderExt<L>: Sized {
    fn layer_ext<T>(self, layer: T) -> ServiceBuilder<Stack<T, L>>;

    fn buffer_lazy(self, bound: usize) -> ServiceBuilder<Stack<BufferLazyLayer, L>> {
        self.layer_ext(BufferLazyLayer::new(bound))
    }
}

impl<L> ServiceBuilderExt<L> for ServiceBuilder<L> {
    fn layer_ext<T>(self, layer: T) -> ServiceBuilder<Stack<T, L>> {
        self.layer(layer)
    }
}
