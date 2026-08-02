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

#[macro_export]
macro_rules! impl_internal_from {
    ($target:ident, $variant:ident, $($err:ty),+ $(,)?) => {
        $(
            impl From<$err> for $target {
                fn from(e: $err) -> Self {
                    $target::$variant(crate::error::internal::Error::from(e))
                }
            }
        )+
    };
}
