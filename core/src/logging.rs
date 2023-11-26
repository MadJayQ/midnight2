extern crate pretty_env_logger;

pub fn init() {
    println!("Initializing pretty_env_logger...");
    pretty_env_logger::init();
    println!("Done!");
}