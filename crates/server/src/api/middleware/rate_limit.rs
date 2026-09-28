use std::sync::Arc;

use axum::Router;
use hyper::Request;
use tower_governor::{
    GovernorError, GovernorLayer, governor::GovernorConfigBuilder,
    key_extractor::KeyExtractor,
};

use crate::{app, model::session::TokenHash};

/// A `tower_governor` key extractor that keys rate limiting on an authenticated
/// session's [`TokenHash`].
///
/// This extractor is placed after [`mac_guard`] in the middleware stack, which
/// guarantees that a valid [`TokenHash`] extension is already present on the
/// request before this extractor runs. If the extension is absent for any
/// reason, the extractor returns an error and the governor rejects the request.
///
/// [`mac_guard`]: crate::api::middleware::mac_guard
#[derive(Clone, Copy)]
pub struct TokenExtractor;

impl KeyExtractor for TokenExtractor {
    type Key = TokenHash;

    fn extract<T>(&self, req: &Request<T>) -> Result<Self::Key, GovernorError> {
        req.extensions()
            .get::<TokenHash>()
            .copied()
            .ok_or(GovernorError::UnableToExtractKey)
    }
}

/// Extension trait that attaches pre-configured rate-limiting layers to an Axum router.
///
/// This trait hides the generic complexity of `tower_governor`'s `GovernorConfig<K, M>`
/// type parameters behind simple, readable method calls. Each method is self-contained
/// and configures a distinct layer of the ingress funnel.
///
/// # Rate Limit Summary
///
/// | Method                  | Scope         | Key       | Quota      |
/// |-------------------------|---------------|-----------|------------|
/// | [`with_global_ip_limit`] | All routes   | Client IP | 25 req/s   |
/// | [`with_auth_ip_limit`]   | `/api/auth/*`| Client IP | 2 req/s    |
/// | [`with_device_limit`]    | `/api/v1/*`  | TokenHash | 15 req/s   |
///
/// [`with_global_ip_limit`]: RateLimitExt::with_global_ip_limit
/// [`with_auth_ip_limit`]: RateLimitExt::with_auth_ip_limit
/// [`with_device_limit`]: RateLimitExt::with_device_limit
pub trait RateLimitExt {
    /// Applies a coarse, per-IP rate limit across the entire router.
    ///
    /// This is the outermost layer of the ingress funnel. It absorbs volumetric
    /// floods and denial-of-service attempts before any business logic runs.
    ///
    /// Quota: 25 requests per second per IP, with a burst allowance of 10.
    #[must_use]
    fn with_global_ip_limit(self) -> Self;

    /// Applies a strict, per-IP rate limit to authentication endpoints.
    ///
    /// Authentication endpoints invoke Argon2id on the client side, but the
    /// server still needs to guard against credential-stuffing and brute-force
    /// attempts that would otherwise exhaust Tokio worker threads. This limit
    /// is intentionally aggressive.
    ///
    /// Quota: 2 requests per second per IP, with a burst allowance of 2.
    #[must_use]
    fn with_auth_ip_limit(self) -> Self;

    /// Applies a per-device rate limit to authenticated endpoints.
    ///
    /// This layer is keyed on the [`TokenHash`] injected by [`mac_guard`], so
    /// each authenticated device has its own independent quota. This prevents a
    /// single runaway client sync loop from monopolizing I/O bandwidth and
    /// thrashing the mechanical disk with random seeks.
    ///
    /// Quota: 15 requests per second per device, with a burst allowance of 5.
    ///
    /// [`mac_guard`]: crate::api::middleware::mac_guard
    #[must_use]
    fn with_device_limit(self) -> Self;
}

#[allow(clippy::expect_used)]
impl RateLimitExt for Router<app::State> {
    fn with_global_ip_limit(self) -> Self {
        let config = Arc::new(
            GovernorConfigBuilder::default()
                .per_second(25)
                .burst_size(10)
                .finish()
                .expect("Failed to build global IP governor"),
        );

        self.layer(GovernorLayer::new(config))
    }

    fn with_auth_ip_limit(self) -> Self {
        let config = Arc::new(
            GovernorConfigBuilder::default()
                .per_second(2)
                .burst_size(2)
                .finish()
                .expect("Failed to build auth IP governor"),
        );

        self.layer(GovernorLayer::new(config))
    }

    fn with_device_limit(self) -> Self {
        let config = Arc::new(
            GovernorConfigBuilder::default()
                .per_second(15)
                .burst_size(5)
                .key_extractor(TokenExtractor)
                .finish()
                .expect("Failed to build device governor"),
        );

        self.route_layer(GovernorLayer::new(config))
    }
}
