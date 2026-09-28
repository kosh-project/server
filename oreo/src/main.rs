use std::{thread, time::Duration};

fn main() {
    let cargo: Vec<u8> = Vec::with_capacity(1024 * 1024 * 1024);

    thread::sleep(Duration::from_secs(20));

    println!("{:?}", cargo);
}
