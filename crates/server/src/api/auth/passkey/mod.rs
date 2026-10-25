use uuid::Uuid;

use crate::api::{self, Error::BadRequest};

pub(crate) fn user_id_to_handle(user_id: i64) -> api::Result<Uuid> {
    let raw = u64::try_from(user_id)?;

    Ok(Uuid::from_u128(u128::from(raw)))
}

pub(crate) fn handle_to_user_id(handle: &Uuid) -> api::Result<i64> {
    let raw = handle.as_u128();

    i64::try_from(raw)
        .map_err(|_| BadRequest("Invalid user handle integer bounds"))
}

mod delete;
mod list;
mod login;
mod register;

pub use delete::delete;
pub use list::list;
pub use login::{login_finish, login_start};
pub use register::{register_finish, register_start};
