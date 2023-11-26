/*    _
    \`*-.
    )  _`-.
    .  : `. .
    : _   '  \                          ███    ███ ██ ██████  ███    ██ ██  ██████  ██   ██ ████████
    ; *` _.   `*-._                     ████  ████ ██ ██   ██ ████   ██ ██ ██       ██   ██    ██
    `-.-'          `-.                  ██ ████ ██ ██ ██   ██ ██ ██  ██ ██ ██   ███ ███████    ██
      ;       `       `.                ██  ██  ██ ██ ██   ██ ██  ██ ██ ██ ██    ██ ██   ██    ██
      :.       .        \               ██      ██ ██ ██████  ██   ████ ██  ██████  ██   ██    ██
      . \  .   :   .-'   .
      '  `+.;  ;  '      :
      :  '  |    ;       ;-.
      ; '   : :`-:     _.`* ;
    [bug] .*' /  .*' ; .*`- +'  `*'
    `*-*   `*-*  `*-*'
*/

extern crate midnight2_core as core;
#[macro_use]
extern crate log;

use core::identifier;
use core::render::{self};
use core::sim::{self};
use std::{os::windows::io::AsHandle, thread::JoinHandle};

use crate::core::logging;

//use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::{
    dpi::LogicalSize,
    event::{self, ElementState, Event, KeyEvent, WindowEvent},
    event_loop::ControlFlow,
    keyboard::{Key, NamedKey},
};

fn spawn_world() -> Result<JoinHandle<()>, Box<dyn std::error::Error>> {
    info!("Initializing sim thread!");
    sim::init()
}

fn spawn_window() {
    info!("Spawning window!");

    let event_loop = winit::event_loop::EventLoop::new().unwrap();
    let window = winit::window::WindowBuilder::new()
        .with_title("Midnight2 Application")
        .with_inner_size(LogicalSize::new(1280.0, 720.0))
        .build(&event_loop)
        .unwrap();

    let mut render_thread: Option<std::thread::JoinHandle<()>> =
        Some(render::init(&window).unwrap());

    event_loop
        .run(move |e, target| {
            let _ = &window;
            target.set_control_flow(ControlFlow::Poll);
            match e {
                Event::LoopExiting => {
                    info!("Spinning down render!");
                    unsafe { render::shutdown() };
                    render_thread.take().map(JoinHandle::join);
                    info!("Done!");

                    info!("Spinning down sim!");
                    unsafe { sim::shutdown() };
                    info!("Done!");
                }
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::KeyboardInput {
                        event:
                            KeyEvent {
                                logical_key: Key::Named(NamedKey::Escape),
                                state: ElementState::Pressed,
                                ..
                            },
                        ..
                    }
                    | WindowEvent::CloseRequested => target.exit(),
                    _ => {}
                },
                _ => {}
            }
        })
        .unwrap();
}

fn main() {
    logging::init();
    info!("Hello midnight!");
    info!("Initializing game sim!");
    let id = identifier::ThreadLocalId::allocate().unwrap();
    info!("Main Thread ID: {:?}", id);
    if let Ok(sim_thread) = spawn_world() {
        spawn_window();
        info!("Shutting down, joining sim thread!");
        sim_thread.join().expect("Failed to join sim thread from the main thread!, typically this ocurrs during shutdown");
    }
}
