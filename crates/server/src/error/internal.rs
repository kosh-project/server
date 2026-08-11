use std::{num::TryFromIntError, time::SystemTimeError};

/// Low-level standard library errors wrapped into a common type for domain propagation.
///
/// This enum exists to bridge the gap between standard library error types (such as
/// `TryFromIntError`) and the domain-specific error enums in `api`, `storage`, and
/// `model`. Each domain error has an `Internal(#[from] internal::Error)` variant, and
/// the [`crate::wrap_internal_err!`] macro generates the additional `From` implementations
/// that allow the `?` operator to chain through two conversions automatically.
///
/// Consumers should never match on this type directly; it is an implementation detail
/// of the error propagation chain.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// An integer type conversion overflowed or underflowed.
    ///
    /// Typically produced when converting between `usize`, `u64`, `i64`, and similar
    /// primitive integer types using `TryFrom` or `TryInto`.
    #[error("Integer conversion overflow : {}", .0)]
    IntConversion(#[from] TryFromIntError),

    /// A system time operation failed.
    ///
    /// Produced when a `SystemTime` value is before the Unix epoch, which can occur
    /// when computing elapsed durations or converting to timestamp integers.
    #[error("Time conversion failed : {}", .0)]
    TimerError(#[from] SystemTimeError),

    /// A freeform error message for cases not covered by the other variants.
    ///
    /// Used when a custom string description is the most practical way to surface
    /// an internal condition without defining a new strongly-typed variant.
    #[error("{}", .0)]
    Message(String),
}

/// Wires up low-level standard library errors directly into your domain-specific error enums.
///
/// This macro generates the `From` implementations required to bypass the "double-from"
/// limitation of the `?` operator. It automatically intercepts specific errors (like `TryFromIntError`)
/// and wraps them in your target domain's `Internal` error variant.
///
/// # Example
///
/// ```rust
/// use std::num::TryFromIntError;
/// use std::time::SystemTimeError;
/// // Assuming your crate is named `webdav_server`
/// use webdav_server::error::internal::Error as InternalErr;
/// use webdav_server::wrap_internal_err;
///
/// #[derive(thiserror::Error, Debug)]
/// pub enum StorageError {
///     // thiserror handles the immediate InternalErr -> StorageError conversion
///     #[error(transparent)]
///     Internal(#[from] InternalErr),
/// }
///
/// // This macro generates the TryFromIntError -> InternalErr -> StorageError conversions!
/// wrap_internal_err! {
///     TryFromIntError,
///     SystemTimeError
///     => StorageError::Internal
/// }
///
/// // Now you can use `?` on infallible casts in functions returning Result<T, StorageError>
/// ```
#[macro_export]
macro_rules! wrap_internal_err {
    ($($err:ty),+ $(,)? => $target:ident::$variant:ident) => {
        $(
            impl From<$err> for $target {
                fn from(e: $err) -> Self {
                    // Use $crate instead of crate so this resolves correctly anywhere it's called!
                    $target::$variant($crate::error::internal::Error::from(e))
                }
            }
        )+
    };
}
