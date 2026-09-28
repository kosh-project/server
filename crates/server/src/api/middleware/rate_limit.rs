use std::sync::Arc;

use axum::Router;
use hyper::Request;
use tower_governor::{
    GovernorError, GovernorLayer, governor::GovernorConfigBuilder,
    key_extractor::KeyExtractor,
};

use crate::{app, model::session::TokenHash};

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

pub trait RateLimitExt {
    #[must_use]
    fn with_global_ip_limit(self) -> Self;
    #[must_use]
    fn with_auth_ip_limit(self) -> Self;
    #[must_use]
    fn with_device_limit(self) -> Self;
}

#[allow(clippy::expect_used)]
impl RateLimitExt for Router<app::State> {
    fn with_auth_ip_limit(self) -> Self {
        let config = Arc::new(
            GovernorConfigBuilder::default()
                .per_second(25)
                .burst_size(5)
                .finish()
                .expect("Failed to build global IP governor"),
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

    fn with_global_ip_limit(self) -> Self {
        let config = Arc::new(
            GovernorConfigBuilder::default()
                .per_second(2)
                .burst_size(2)
                .finish()
                .expect("Failed to build auth IP governor"),
        );

        self.layer(GovernorLayer::new(config))
    }
}
