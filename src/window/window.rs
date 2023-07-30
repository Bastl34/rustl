//use egui_winit::winit;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow};

#[cfg(target_arch="wasm32")]
use wasm_bindgen::prelude::*;

use crate::interface::main_interface::MainInterface;

fn setup_window() -> (winit::event_loop::EventLoop<()>, winit::window::Window)
{
    /*
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .with_module_level("wgpu_core", log::LevelFilter::Warn)
        .with_module_level("wgpu_hal", log::LevelFilter::Warn)
        .init()
        .unwrap();

    */

    let width = 1024;
    let height = 786;

    let event_loop = winit::event_loop::EventLoop::new();
    let window = winit::window::WindowBuilder::new()
        .with_title("Rustl")
        .with_inner_size(winit::dpi::PhysicalSize::new(width, height))
        .with_resizable(true)
        .build(&event_loop)
        .unwrap();

    #[cfg(target_arch = "wasm32")]
    {
        // Winit prevents sizing with CSS, so we have to set
        // the size manually when on web.
        use winit::dpi::PhysicalSize;
        window.set_inner_size(PhysicalSize::new(width, height));

        use winit::platform::web::WindowExtWebSys;
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| {
                let dst = doc.get_element_by_id("container")?;
                let canvas = web_sys::Element::from(window.canvas());
                dst.append_child(&canvas).ok()?;
                Some(())
            })
            .expect("Couldn't append canvas to document body.");
    }

    log::info!("winit window initialized");

    (event_loop, window)
}

fn run(event_loop: winit::event_loop::EventLoop<()>, mut interface: MainInterface)
{
    event_loop.run(move |event, _, control_flow| match event
    {
        Event::WindowEvent { ref event, .. } =>
        {
            match event
            {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::Resized(resize) => interface.resize(resize.clone(), None),
                WindowEvent::ScaleFactorChanged { new_inner_size, scale_factor } => interface.resize(**new_inner_size, Some(scale_factor.clone())),
                _ => interface.input(event),
            }
        },
        Event::RedrawRequested(_) => interface.update(),
        Event::MainEventsCleared => interface.window().request_redraw(),
        _ => (),
    });
}

pub async fn start()
{
    cfg_if::cfg_if!
    {
        if #[cfg(target_arch = "wasm32")]
        {
            std::panic::set_hook(Box::new(console_error_panic_hook::hook));
            console_log::init_with_level(log::Level::Debug).expect("Couldn't initialize logger");
        }
        else
        {
           //env_logger::init();
        }
    }

    let (event_loop, window) = setup_window();
    let interface = MainInterface::new(window, &event_loop).await;

    run(event_loop, interface);
}