use std::{num::TryFromIntError, time::SystemTimeError};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Integer conversion overflow : {}", .0)]
    IntConversion(#[from] TryFromIntError),

    #[error("Time conversion failed : {}", .0)]
    TimerError(#[from] SystemTimeError),

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