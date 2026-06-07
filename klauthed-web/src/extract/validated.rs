//! The [`Validated`] body extractor (deserialize + [`Validate`]).

use std::ops::{Deref, DerefMut};

use actix_web::dev::Payload;
use actix_web::{FromRequest, HttpRequest};
use futures_util::future::LocalBoxFuture;
use klauthed_core::validation::Validate;
use serde::de::DeserializeOwned;

use crate::error::AppError;

use super::parse_json;

/// JSON body extractor that deserializes then [`Validate`]s the value.
///
/// A malformed body yields the same `BadRequest` as [`Json`](super::Json); a well-formed but
/// invalid body yields the type's [`ValidationErrors`](klauthed_core::validation::ValidationErrors)
/// as a `BadRequest` [`AppError`].
///
/// Deref to `T`; [`Validated::into_inner`] takes ownership of the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Validated<T>(pub T);

impl<T> Validated<T> {
    /// Consume the wrapper and return the validated value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Validated<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Validated<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> FromRequest for Validated<T>
where
    T: DeserializeOwned + Validate + 'static,
{
    type Error = AppError;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let fut = parse_json::<T>(req, payload);
        Box::pin(async move {
            let value = fut.await?;
            value.validate().map_err(AppError::from_domain)?;
            Ok(Validated(value))
        })
    }
}
