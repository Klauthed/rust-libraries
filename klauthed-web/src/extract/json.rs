//! The [`Json`] body extractor.

use std::ops::{Deref, DerefMut};

use actix_web::dev::Payload;
use actix_web::{FromRequest, HttpRequest};
use futures_util::future::LocalBoxFuture;
use serde::de::DeserializeOwned;

use crate::error::AppError;

use super::parse_json;

/// JSON body extractor that maps deserialization failures to
/// [`AppError::bad_request`].
///
/// Deref to `T`; [`Json::into_inner`] takes ownership of the parsed value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Json<T>(pub T);

impl<T> Json<T> {
    /// Consume the wrapper and return the parsed value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Json<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Json<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> FromRequest for Json<T>
where
    T: DeserializeOwned + 'static,
{
    type Error = AppError;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let fut = parse_json::<T>(req, payload);
        Box::pin(async move { fut.await.map(Json) })
    }
}
