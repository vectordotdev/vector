use http::{HeaderMap, HeaderName, HeaderValue, Method, Request, Uri};

use crate::http::Auth;

pub struct RequestBlueprint {
    pub uri: Uri,
    pub method: Method,
    pub headers: HeaderMap,
    pub auth: Option<Auth>,
}

impl RequestBlueprint {
    pub fn from_uri(uri: Uri) -> Self {
        Self {
            uri,
            method: Method::POST,
            headers: HeaderMap::new(),
            auth: None,
        }
    }

    pub fn with_method(mut self, method: Method) -> Self {
        self.method = method;
        self
    }

    pub fn add_headers<I, K, V>(mut self, headers: I) -> crate::Result<Self>
    where
        I: IntoIterator<Item = (K, V)>,
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: std::error::Error + Send + Sync + 'static,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: std::error::Error + Send + Sync + 'static,
    {
        for (header_name, header_value) in headers.into_iter() {
            self = self.add_header(header_name, header_value)?
        }
        Ok(self)
    }

    pub fn add_header<K, V>(mut self, header_name: K, header_value: V) -> crate::Result<Self>
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: std::error::Error + Send + Sync + 'static,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: std::error::Error + Send + Sync + 'static,
    {
        let header_name: HeaderName = header_name.try_into()?;
        self.headers.append(header_name, header_value.try_into()?);
        Ok(self)
    }

    pub fn add_header_maybe<K, V>(
        self,
        header_name: K,
        maybe_header_value: Option<V>,
    ) -> crate::Result<Self>
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: std::error::Error + Send + Sync + 'static,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: std::error::Error + Send + Sync + 'static,
    {
        if let Some(header_value) = maybe_header_value {
            self.add_header(header_name, header_value)
        } else {
            Ok(self)
        }
    }

    pub fn add_auth_maybe(mut self, maybe_auth: Option<Auth>) -> Self {
        self.auth = maybe_auth;
        self
    }

    pub fn create_http_request<B>(&self, body: B) -> Request<B> {
        // SAFETY: We're passing a `Uri` for the URI and `Method` for the method, so those fallible
        // conversions it uses in the builder methods should never return an error. Similarly,
        // because they should never error, the header map should always be present for us to extend
        // with our own headers. In turn, providing the body to build the final request value should
        // also never fail because there were no prior errors during building.
        //
        // This is safe according to the documentation/logic of `Builder`, even if it's not the best
        // thought out API.
        let mut builder = Request::builder()
            .uri(self.uri.clone())
            .method(self.method.clone());

        let headers = builder
            .headers_mut()
            .expect("request parts should be present");
        headers.extend(self.headers.clone());

        if let Some(auth) = self.auth.as_ref() {
            auth.apply_headers_map(headers);
        }

        builder
            .body(body)
            .expect("building request should never fail")
    }
}
