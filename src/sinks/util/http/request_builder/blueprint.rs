use http::{HeaderMap, HeaderName, HeaderValue, Method, Request, Uri};

pub struct RequestBlueprint {
    pub uri: Uri,
    pub method: Method,
    pub headers: HeaderMap,
}

impl RequestBlueprint {
    pub fn from_uri(uri: Uri) -> Self {
        Self {
            uri,
            method: Method::Post,
            headers: HeaderMap::new(),
        }
    }

    pub fn with_method(mut self, method: Method) -> Self {
        self.method = method;
        self
    }

    pub fn with_headers(mut self, headers: HeaderMap) -> Self {
        self.headers = headers;
        self
    }

    pub fn with_header<K, V>(mut self, header_name: K, header_value: V) -> crate::Result<Self>
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<crate::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<crate::Error>,
    {
        self.headers
            .append(header_name.try_into()?, header_value.try_into()?);
        Ok(self)
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

        let mut headers = builder
            .headers_mut()
            .expect("request parts should be present");
        headers.extend(self.headers.clone());

        builder
            .body(body)
            .expect("building request should never fail")
    }
}
