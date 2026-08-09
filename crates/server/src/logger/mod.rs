mod service;
mod error;


use bincode_next::{Decode, Encode};


#[derive(Encode, Decode)]
pub struct Entry {
    module: Module,
    level : Level,
    timestamp_ms: i64,
    message: String,
}

#[derive(Encode, Decode)]
pub enum Level {
    High = 0    
}

#[derive(Encode, Decode)]
pub enum Module {

}


pub use service::Service;