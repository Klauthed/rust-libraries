//! Ambient (tokio task-local) propagation of [`RequestContext`] (feature
//! `task-local`).

use std::future::Future;

use super::RequestContext;

tokio::task_local! {
    static CURRENT: RequestContext;
}

impl RequestContext {
    /// Run `future` with this context installed as the current one, so code
    /// below can read it via [`try_current`](RequestContext::try_current)
    /// without it being passed explicitly.
    pub async fn scope<F>(self, future: F) -> F::Output
    where
        F: Future,
    {
        CURRENT.scope(self, future).await
    }

    /// A clone of the current context, or `None` if called outside a
    /// [`scope`](RequestContext::scope).
    pub fn try_current() -> Option<RequestContext> {
        CURRENT.try_with(|ctx| ctx.clone()).ok()
    }

    /// Borrow the current context to compute a value, or `None` if called
    /// outside a [`scope`](RequestContext::scope).
    pub fn with_current<R>(f: impl FnOnce(&RequestContext) -> R) -> Option<R> {
        CURRENT.try_with(f).ok()
    }
}
