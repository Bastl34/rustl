use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

#[cfg(target_arch="wasm32")]
use wasm_bindgen::prelude::*;

use crate::interface::main_interface::MainInterface;

struct App
{
    interface: MainInterface,
}

impl App
{
    async fn new(window: Arc<Window>,) -> Self
    {
        let interface = MainInterface::new(window).await;
        Self
        {
            interface,
        }
    }
}

// TODO: optimize this like https://github.com/rust-windowing/winit/releases/tag/v0.30.0
// inspired by: https://github.com/Dunrar/WebGpuTuts/tree/main

enum CustomEvent
{
    Initialized(App),
}

enum AppState
{
    // TODO: EventLoopProxy will no longer be required here once https://github.com/rust-windowing/winit/issues/3741 lands
    Uninitialized(EventLoopProxy<CustomEvent>),
    Initialized(App),
}

impl ApplicationHandler<CustomEvent> for AppState
{
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop)
    {
        let width = 1920.0;
        let height = 1080.0;

        match self
        {
            AppState::Uninitialized(event_loop_proxy) =>
            {
                let mut window_attrs = Window::default_attributes();
                window_attrs.title = "Rustl".to_string();
                window_attrs.inner_size = Some(winit::dpi::Size::Logical(LogicalSize::new(width, height)));
                window_attrs.resizable = true;

                #[cfg(not(target_arch = "wasm32"))]
                {
                    let window = Arc::new(event_loop.create_window(window_attrs).unwrap());
                    let app = pollster::block_on(App::new(window));

                    //let proxy = event_loop_proxy.clone();

                    assert!(event_loop_proxy.send_event(CustomEvent::Initialized(app)).is_ok());
                }

                #[cfg(target_arch = "wasm32")]
                {
                    use winit::dpi::PhysicalSize;
                    use winit::platform::web::WindowAttributesExtWebSys;

                    let window_attrs = window_attrs.with_append(true);
                    let window = Arc::new(event_loop.create_window(window_attrs).unwrap());

                    let _ = window.request_inner_size(PhysicalSize::new(width, height));

                    let event_loop_proxy = event_loop_proxy.clone();
                    wasm_bindgen_futures::spawn_local(async move
                    {
                        let app = App::new(window).await;
                        assert!(event_loop_proxy.send_event(CustomEvent::Initialized(app)).is_ok());
                    });
                }
            }
            AppState::Initialized(_) => {}
        }
    }

    fn window_event(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, _window_id: winit::window::WindowId, event: winit::event::WindowEvent, )
    {
        let app = match self
        {
            AppState::Initialized(app) => app,
            AppState::Uninitialized(_) => return,
        };

        match event
        {
            winit::event::WindowEvent::Resized(size) => app.interface.resize(Some(size.clone()), None),
            winit::event::WindowEvent::ScaleFactorChanged { scale_factor, .. } => app.interface.resize(None, Some(scale_factor.clone())),
            winit::event::WindowEvent::RedrawRequested =>
            {
                app.interface.update();

                if app.interface.check_exit()
                {
                    event_loop.exit();
                }
                else
                {
                    // TODO: check vsync (check web)
                    // https://github.com/rust-windowing/winit/issues/2900
                    // https://github.com/sotrh/learn-wgpu/pull/560/files
                    //

                    app.interface.window().request_redraw();
                    app.interface.update_done();

                }
            },
            winit::event::WindowEvent::CloseRequested => event_loop.exit(),
            _ => app.interface.input(&event)
        }
    }

    /*
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop)
    {
        self.window.request_redraw();
        //self.counter += 1;
    }
    */

    fn user_event(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop, user_event: CustomEvent,)
    {
        match user_event
        {
            CustomEvent::Initialized(app) =>
            {
                take_mut::take(self, |state| match state
                {
                    AppState::Uninitialized(_) =>
                    {
                        app.interface.window().request_redraw();
                        AppState::Initialized(app)
                    },
                    AppState::Initialized(_) => state,
                });
            }
        }
    }
}


pub fn run()
{
    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();
    }

    let event_loop = winit::event_loop::EventLoop::with_user_event().build().unwrap();
    let mut app = AppState::Uninitialized(event_loop.create_proxy());

    #[cfg(not(target_arch = "wasm32"))]
    {
        event_loop.run_app(&mut app).unwrap();
    }

    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::EventLoopExtWebSys;

        event_loop.spawn_app(app);
    }
}