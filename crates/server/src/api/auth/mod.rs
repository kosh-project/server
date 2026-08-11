//! Authentication handlers: user registration and session creation.
//!
//! Both endpoints are unauthenticated (they sit outside the `auth_guard` middleware)
//! and are mounted under `/api/auth`. They operate on the [`User`] and [`Session`]
//! model types to manage identities and opaque session tokens.
//!
//! [`User`]: crate::model::user::User
//! [`Session`]: crate::model::session::Session
mod login;
mod register;

pub use login::login;
pub use register::register;
