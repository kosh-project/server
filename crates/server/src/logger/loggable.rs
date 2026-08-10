use crate::logger::{Level, Module};

pub trait Loggable {
    fn log_level(&self) -> Level;
    fn log_module(&self) -> Module;
}
