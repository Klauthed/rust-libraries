//! The [`Context`] extractor handing the per-request `RequestContext` to handlers.

use std::future::{Ready, ready};
use std::ops::Deref;

use actix_web::{Error, FromRequest, HttpMessage, HttpRequest};
use klauthed_core::context::RequestContext;

// ── Extractor ─────────────────────────────────────────────────────────────────

/// Extractor handing the per-request [`RequestContext`] to handlers.
///
/// It reads the context the [`RequestContextMiddleware`](super::RequestContextMiddleware) stored in the request
/// extensions. If none is present (e.g. the middleware was not mounted), it
/// yields a fresh default context rather than failing the request.
///
/// Deref to [`RequestContext`], so all of its accessors are available directly,
/// and [`Context::into_inner`] takes ownership of the underlying context.
#[derive(Debug, Clone)]
pub struct Context(RequestContext);

impl Context {
    /// Consume the extractor and return the owned [`RequestContext`].
    pub fn into_inner(self) -> RequestContext {
        self.0
    }
}

impl Deref for Context {
    type Target = RequestContext;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Context> for RequestContext {
    fn from(ctx: Context) -> Self {
        ctx.0
    }
}

impl FromRequest for Context {
    type Error = Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
        let ctx = req.extensions().get::<RequestContext>().cloned().unwrap_or_default();
        ready(Ok(Context(ctx)))
    }
}
