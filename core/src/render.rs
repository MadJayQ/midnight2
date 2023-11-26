extern crate wgpu_hal as hal;
extern crate wgpu_types as wgt;

use std::{
    borrow::Borrow,
    iter,
    sync::atomic::{AtomicBool, Ordering},
    thread::{self, JoinHandle},
};

use hal::{
    Adapter as _, Api, CommandEncoder as _, Device as _, Instance as _, Queue as _, Surface as _,
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use winit::window;

const MAX_FRAMES_IN_FLIGHT: u32 = 3;

cfg_if::cfg_if! {
    // Apple + Metal
    if #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "metal"))] {
        type TargetApi = hal::api::Metal;
    }
    // Wasm + Vulkan
    else if #[cfg(all(not(target_arch = "wasm32"), feature = "vulkan"))] {
        type TargetApi = hal::api::Vulkan;
    }
    // Windows + DX12
    else if #[cfg(all(windows, feature = "dx12"))] {
        type TargetApi = hal::api::Dx12;
    }
    // Anything + GLES
    else if #[cfg(feature = "gles")] {
        type TargetApi = hal::api::Gles;
    }
    // Fallback
    else {
        type TargetApi = hal::api::Empty;
    }
}

pub struct RenderFrame<A: hal::Api> {
    encoder: A::CommandEncoder,
    fence: A::Fence,
    fence_value: hal::FenceValue,
    used_views: Vec<A::TextureView>,
    used_cmd_bufs: Vec<A::CommandBuffer>,
    frames_recorded: usize,
}

impl<A: hal::Api> RenderFrame<A> {
    unsafe fn wait_and_clear(&mut self, device: &A::Device) {
        device.wait(&self.fence, self.fence_value, !0).unwrap();
        self.encoder.reset_all(self.used_cmd_bufs.drain(..));
        for view in self.used_views.drain(..) {
            device.destroy_texture_view(view);
        }
        self.frames_recorded = 0;
    }

    unsafe fn destroy(self, device: &A::Device) {
        device.destroy_command_encoder(self.encoder);
        device.destroy_fence(self.fence);
    }
}

#[allow(dead_code)]
pub struct GameRenderer<A: hal::Api> {
    instance: A::Instance,
    adapter: A::Adapter,
    surface: A::Surface,
    surface_format: wgt::TextureFormat,
    device: A::Device,
    queue: A::Queue,
    frames_in_flight: [Option<RenderFrame<A>>; MAX_FRAMES_IN_FLIGHT as usize],
    frame_index: usize,
    extent: [u32; 2],
}

impl<A: hal::Api> GameRenderer<A> {
    fn init(window: &winit::window::Window) -> Result<Self, Box<dyn std::error::Error>> {
        let instance_desc = hal::InstanceDescriptor {
            name: "Midnight2Instance",
            flags: wgt::InstanceFlags::from_build_config().with_env(),
            dx12_shader_compiler: wgt::Dx12Compiler::Dxc {
                dxil_path: None,
                dxc_path: None,
            },
            gles_minor_version: wgt::Gles3MinorVersion::Automatic,
        };

        let instance = unsafe { A::Instance::init(&instance_desc)? };
        let surface = {
            let raw_window_handle = window.window_handle()?.as_raw();
            let raw_display_handle = window.display_handle()?.as_raw();

            unsafe {
                instance
                    .create_surface(raw_display_handle, raw_window_handle)
                    .unwrap()
            }
        };
        let (adapter, capabilities) = unsafe {
            let mut adapters = instance.enumerate_adapters();
            if adapters.is_empty() {
                return Err("no adapters found".into());
            }
            let exposed = adapters.swap_remove(0);
            (exposed.adapter, exposed.capabilities)
        };

        let surface_caps = unsafe { adapter.surface_capabilities(&surface) }
            .ok_or("failed to get surface capabilities")?;
        info!("Surface caps: {:#?}", surface_caps);

        let hal::OpenDevice { device, queue } = unsafe {
            adapter
                .open(wgt::Features::empty(), &wgt::Limits::default())
                .unwrap()
        };

        let window_size: (u32, u32) = window.inner_size().into();
        let surface_config = hal::SurfaceConfiguration {
            swap_chain_size: MAX_FRAMES_IN_FLIGHT.clamp(
                *surface_caps.swap_chain_sizes.start(),
                *surface_caps.swap_chain_sizes.end(),
            ),
            present_mode: wgt::PresentMode::Fifo,
            composite_alpha_mode: wgt::CompositeAlphaMode::Opaque,
            format: wgt::TextureFormat::Bgra8UnormSrgb,
            extent: wgt::Extent3d {
                width: window_size.0,
                height: window_size.1,
                depth_or_array_layers: 1,
            },
            usage: hal::TextureUses::COLOR_TARGET,
            view_formats: vec![],
        };
        unsafe {
            surface.configure(&device, &surface_config).unwrap();
        };

        let frame_data: [Option<RenderFrame<A>>; MAX_FRAMES_IN_FLIGHT as usize] = core::array::from_fn(|_| {
            unsafe {
                let hal_desc = hal::CommandEncoderDescriptor {
                    label: None,
                    queue: &queue,
                };

                let frame: RenderFrame<A> = RenderFrame {
                    encoder: device.create_command_encoder(&hal_desc).unwrap(),
                    fence: device.create_fence().unwrap(),
                    fence_value: 0,
                    used_views: Vec::new(),
                    used_cmd_bufs: Vec::new(),
                    frames_recorded: 0,
                };
                Some(frame)
            }
        });


        Ok(Self {
            instance: instance,
            adapter: adapter,
            surface: surface,
            surface_format: surface_config.format,
            device: device,
            queue: queue,
            frames_in_flight: frame_data,
            frame_index: 0,
            extent: [window_size.0, window_size.1],
        })
    }
    fn exit(mut self) {
        let frame = &mut self.frames_in_flight[self.frame_index].as_mut().unwrap();
        unsafe {
            self.queue
                .submit(&[], Some((&mut frame.fence, frame.fence_value)))
                .unwrap();
            frame.wait_and_clear(&self.device);

            for i in 0..MAX_FRAMES_IN_FLIGHT {
                self.frames_in_flight[i as usize]
                    .take()
                    .unwrap()
                    .destroy(&self.device);
            }

            self.surface.unconfigure(&self.device);
            self.device.exit(self.queue);
            self.instance.destroy_surface(self.surface);
            drop(self.adapter);
        }
    }
}

static mut S_SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub unsafe fn should_shutdown() -> bool {
    S_SHUTDOWN.load(Ordering::Relaxed)
}

pub unsafe fn shutdown() {
    S_SHUTDOWN.store(true, Ordering::Relaxed);
}

fn render_loop(game_renderer: &mut GameRenderer<TargetApi>) {
    let device = &game_renderer.device;
    let queue = &game_renderer.queue;
    let surface = &game_renderer.surface;

    let frame = &mut game_renderer.frames_in_flight[game_renderer.frame_index]
        .as_mut()
        .unwrap();
    unsafe {
        let surface_tex = surface.acquire_texture(None).unwrap().unwrap().texture;
        let encoder = &mut frame.encoder;
        let target_barrier0: hal::TextureBarrier<'_, TargetApi> = hal::TextureBarrier {
            texture: surface_tex.borrow(),
            range: wgt::ImageSubresourceRange::default(),
            usage: hal::TextureUses::UNINITIALIZED..hal::TextureUses::COLOR_TARGET,
        };
        encoder.begin_encoding(Some("frame")).unwrap();
        encoder.transition_textures(iter::once(target_barrier0));

        let surface_view_desc = hal::TextureViewDescriptor {
            label: None,
            format: game_renderer.surface_format,
            dimension: wgt::TextureViewDimension::D2,
            usage: hal::TextureUses::COLOR_TARGET,
            range: wgt::ImageSubresourceRange::default(),
        };
        let surface_tex_view = device
            .create_texture_view(surface_tex.borrow(), &surface_view_desc)
            .unwrap();
        let pass_desc = hal::RenderPassDescriptor {
            label: None,
            extent: wgt::Extent3d {
                width: game_renderer.extent[0],
                height: game_renderer.extent[1],
                depth_or_array_layers: 1,
            },
            sample_count: 1,
            color_attachments: &[Some(hal::ColorAttachment {
                target: hal::Attachment::<TargetApi> {
                    view: &surface_tex_view,
                    usage: hal::TextureUses::COLOR_TARGET,
                },
                resolve_target: None,
                ops: hal::AttachmentOps::STORE,
                clear_value: wgt::Color {
                    r: 0.1,
                    g: 0.2,
                    b: 0.3,
                    a: 1.0,
                },
            })],
            depth_stencil_attachment: None,
            multiview: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        };
        encoder.begin_render_pass(&pass_desc);

        let target_barrier1 = hal::TextureBarrier::<TargetApi> {
            texture: surface_tex.borrow(),
            range: wgt::ImageSubresourceRange::default(),
            usage: hal::TextureUses::COLOR_TARGET..hal::TextureUses::PRESENT,
        };
        encoder.end_render_pass();
        encoder.transition_textures(iter::once(target_barrier1));
        let fence_param: Option<(&mut hal::dx12::Fence, u64)> = if true {
            Some((&mut frame.fence, frame.fence_value))
        } else {
            None
        };

        let cmd_buf = encoder.end_encoding().unwrap();
        queue.submit(&[&cmd_buf], fence_param).unwrap();
        queue.present(&surface, surface_tex).unwrap();
        frame.used_cmd_bufs.push(cmd_buf);
        frame.used_views.push(surface_tex_view);
    }

    trace!("render loop! Renderer at {:p}", game_renderer);
}

pub fn init(window: &winit::window::Window) -> Result<JoinHandle<()>, Box<dyn std::error::Error>> {
    let mut game_renderer = GameRenderer::<TargetApi>::init(window)?;

    Ok(thread::spawn(move || loop {
        
        unsafe {
            if should_shutdown() {
                game_renderer.exit();
                break;
            }
        }
        render_loop(&mut game_renderer);
    }))
}
