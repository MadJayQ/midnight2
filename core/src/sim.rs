use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread::{JoinHandle, self},
};

use crate::{
    ecs::ecs_world
};

static mut S_SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub unsafe fn should_shutdown() -> bool {
    S_SHUTDOWN.load(Ordering::Relaxed)
}

pub unsafe fn shutdown() {
    S_SHUTDOWN.store(true, Ordering::Relaxed);
}

pub fn init() -> Result<JoinHandle<()>, Box<dyn std::error::Error>> {
    Ok(thread::spawn(move || loop {
        unsafe {
            if should_shutdown() {
                break;
            }
        }
        // render_loop(&mut game_renderer);
    }))
}