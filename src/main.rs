use std::{
    collections::HashMap,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

use cef::*;
#[cfg(target_os = "linux")]
use winit::platform::x11::EventLoopBuilderExtX11;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowAttributes, WindowId},
};

mod platform;
use platform::NativeHandle;

#[derive(Clone)]
enum AppEvent {
    CefWork(Duration),
    CefInitialized,
    BrowserClosed,
}

#[derive(Clone, Copy)]
enum CefPump {
    Idle,
    DueAt(Instant),
}

#[derive(Clone, Copy)]
pub(crate) enum Layout {
    Fill,
    LeftHalf,
    RightHalf,
    Fixed {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
}

struct BrowserChild {
    handle: NativeHandle,
    host: cef::BrowserHost,
    layout: Layout,
}

struct HostWindow {
    window: Box<dyn Window>,
    size: PhysicalSize<u32>,
    children: Vec<BrowserChild>,
}

struct WinitHostApp {
    sender: Sender<AppEvent>,
    receiver: Receiver<AppEvent>,
    proxy: EventLoopProxy,
    windows: HashMap<WindowId, HostWindow>,
    live_browsers: usize,
    cef_pump: CefPump,
}

impl WinitHostApp {
    fn new(sender: Sender<AppEvent>, receiver: Receiver<AppEvent>, proxy: EventLoopProxy) -> Self {
        Self {
            sender,
            receiver,
            proxy,
            windows: HashMap::new(),
            live_browsers: 0,
            cef_pump: CefPump::Idle,
        }
    }

    fn create_windows(&mut self, event_loop: &dyn ActiveEventLoop) {
        let full = self.create_host_window(
            event_loop,
            "1 - one browser fills the Winit window",
            LogicalSize::new(900.0, 650.0),
            vec![("https://www.tauri.app/", Layout::Fill)],
        );
        self.windows.insert(full.window.id(), full);

        let split = self.create_host_window(
            event_loop,
            "2 - two browsers split side-by-side",
            LogicalSize::new(1100.0, 650.0),
            vec![
                ("https://www.github.com/tauri-apps/tauri", Layout::LeftHalf),
                ("https://x.com/TauriApps", Layout::RightHalf),
            ],
        );
        self.windows.insert(split.window.id(), split);

        let overlay = self.create_host_window(
            event_loop,
            "3 - one fill browser plus fixed overlay browser",
            LogicalSize::new(1000.0, 700.0),
            vec![
                ("https://www.youtube.com/", Layout::Fill),
                (
                    "https://www.docs.rs/",
                    Layout::Fixed {
                        x: 100,
                        y: 100,
                        width: 400,
                        height: 400,
                    },
                ),
            ],
        );
        self.windows.insert(overlay.window.id(), overlay);
        self.layout_all();
    }

    fn create_host_window(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        title: &str,
        size: LogicalSize<f64>,
        browsers: Vec<(&str, Layout)>,
    ) -> HostWindow {
        let window = event_loop
            .create_window(
                WindowAttributes::default()
                    .with_title(title)
                    .with_surface_size(size),
            )
            .expect("failed to create Winit window");

        let host_handle = platform::host_handle(window.as_ref());
        let size = window.surface_size();
        let browser_count = browsers.len();
        let children = browsers
            .into_iter()
            .map(|(url, layout)| {
                let mut client = NativeChildClient::new(self.sender.clone(), self.proxy.clone());
                create_browser_child(host_handle, url, layout, &mut client)
            })
            .collect();
        self.live_browsers += browser_count;

        HostWindow {
            window,
            size,
            children,
        }
    }

    fn close_window(&mut self, window_id: WindowId, event_loop: &dyn ActiveEventLoop) {
        let Some(host) = self.windows.remove(&window_id) else {
            return;
        };

        for child in &host.children {
            child.host.close_browser(0);
        }

        self.exit_if_done(event_loop);
    }

    fn browser_closed(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.live_browsers = self.live_browsers.saturating_sub(1);
        self.exit_if_done(event_loop);
    }

    fn exit_if_done(&self, event_loop: &dyn ActiveEventLoop) {
        if self.windows.is_empty() && self.live_browsers == 0 {
            event_loop.exit();
        }
    }

    fn layout_all(&self) {
        for host in self.windows.values() {
            layout_host_window(host);
        }
    }

    fn advance_cef(&mut self) {
        #[cfg(target_os = "linux")]
        drain_glib();
        cef::do_message_loop_work();
        #[cfg(target_os = "linux")]
        drain_glib();
    }

    fn schedule_cef_work(&mut self, event_loop: &dyn ActiveEventLoop, delay: Duration) {
        let deadline = Instant::now()
            .checked_add(delay)
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(60));
        self.cef_pump = CefPump::DueAt(deadline);
        event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
    }

    fn advance_cef_if_due(&mut self, event_loop: &dyn ActiveEventLoop) {
        if let CefPump::DueAt(deadline) = self.cef_pump {
            if Instant::now() < deadline {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                return;
            }

            self.cef_pump = CefPump::Idle;
        }

        self.advance_cef();
    }

    fn handle_app_event(&mut self, event_loop: &dyn ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::CefWork(delay) => {
                self.schedule_cef_work(event_loop, delay);
            }
            AppEvent::CefInitialized => {
                self.create_windows(event_loop);
            }
            AppEvent::BrowserClosed => {
                self.browser_closed(event_loop);
            }
        }
    }

    fn drain_app_events(&mut self, event_loop: &dyn ActiveEventLoop) {
        while let Ok(event) = self.receiver.try_recv() {
            self.handle_app_event(event_loop, event);
        }
    }
}

impl ApplicationHandler for WinitHostApp {
    fn can_create_surfaces(&mut self, _event_loop: &dyn ActiveEventLoop) {}

    fn new_events(&mut self, _event_loop: &dyn ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::Init) {
            self.advance_cef();
        }
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.drain_app_events(event_loop);
        self.advance_cef_if_due(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.advance_cef_if_due(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                self.close_window(window_id, event_loop);
            }
            WindowEvent::SurfaceResized(size) => {
                if let Some(host) = self.windows.get_mut(&window_id) {
                    host.size = size;
                    layout_host_window(host);
                }
            }
            _ => {}
        }
    }
}

#[cfg(target_os = "linux")]
fn drain_glib() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

fn create_browser_child(
    parent: NativeHandle,
    url: &str,
    layout: Layout,
    client: &mut Client,
) -> BrowserChild {
    let bounds = cef::Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 100,
    };
    #[cfg(target_os = "linux")]
    let parent = parent as usize as cef::sys::cef_window_handle_t;
    #[cfg(target_os = "macos")]
    let parent = parent as cef::sys::cef_window_handle_t;
    #[cfg(target_os = "windows")]
    let parent = cef::sys::HWND(parent.cast());

    let window_info = cef::WindowInfo::default().set_as_child(parent, &bounds);
    let settings = cef::BrowserSettings {
        background_color: 0xff000000,
        ..Default::default()
    };
    let url = cef::CefString::from(url);
    let browser = cef::browser_host_create_browser_sync(
        Some(&window_info),
        Some(client),
        Some(&url),
        Some(&settings),
        None,
        None,
    )
    .expect("failed to create CEF browser");
    let host = browser.host().expect("CEF browser has no host");

    BrowserChild {
        #[cfg(unix)]
        handle: host.window_handle() as usize as NativeHandle,
        #[cfg(windows)]
        handle: host.window_handle().0.cast(),
        host,
        layout,
    }
}

wrap_client! {
    struct NativeChildClient {
        sender: Sender<AppEvent>,
        proxy: EventLoopProxy,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(NativeChildLifeSpanHandler::new(
                self.sender.clone(),
                self.proxy.clone(),
            ))
        }
    }
}

wrap_life_span_handler! {
    struct NativeChildLifeSpanHandler {
        sender: Sender<AppEvent>,
        proxy: EventLoopProxy,
    }

    impl LifeSpanHandler {
        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if browser.is_none() {
                return;
            }

            let _ = self.sender.send(AppEvent::BrowserClosed);
            self.proxy.wake_up();
        }
    }
}

fn layout_host_window(host: &HostWindow) {
    let PhysicalSize { width, height } = host.size;
    let width = width as i32;
    let height = height as i32;

    for child in &host.children {
        let (x, y, child_width, child_height) = bounds_for_layout(child.layout, width, height);
        platform::set_browser_bounds(child.handle, &child.host, x, y, child_width, child_height);
    }
}

pub(crate) fn bounds_for_layout(layout: Layout, width: i32, height: i32) -> (i32, i32, i32, i32) {
    match layout {
        Layout::Fill => (0, 0, width, height),
        Layout::LeftHalf => (0, 0, width / 2, height),
        Layout::RightHalf => {
            let left_width = width / 2;
            (left_width, 0, width - left_width, height)
        }
        Layout::Fixed {
            x,
            y,
            width,
            height,
        } => (x, y, width, height),
    }
}

wrap_app! {
    struct NativeChildApp {
        sender: Sender<AppEvent>,
        proxy: EventLoopProxy,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefStringUtf16>,
            command_line: Option<&mut CommandLine>,
        ) {
            let Some(command_line) = command_line else {
                return;
            };

            #[cfg(target_os = "linux")]
            {
                command_line.append_switch_with_value(Some(&"ozone-platform".into()), Some(&"x11".into()));
                command_line.append_switch_with_value(Some(&"password-store".into()), Some(&"basic".into()));
            }

            #[cfg(target_os = "macos")]
            command_line.append_switch(Some(&"use-mock-keychain".into()));
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(NativeChildBrowserProcessHandler::new(self.sender.clone(), self.proxy.clone()))
        }
    }
}

wrap_browser_process_handler! {
    struct NativeChildBrowserProcessHandler {
        sender: Sender<AppEvent>,
        proxy: EventLoopProxy,
    }

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            debug_assert_ne!(currently_on(ThreadId::UI), 0);
            let _ = self.sender.send(AppEvent::CefInitialized);
            self.proxy.wake_up();
        }

        fn on_schedule_message_pump_work(&self, delay_ms: i64) {
            let delay = Duration::from_millis(delay_ms.max(0) as u64);
            let _ = self.sender.send(AppEvent::CefWork(delay));
            self.proxy.wake_up();
        }
    }
}

fn main() -> Result<(), &'static str> {
    #[cfg(target_os = "macos")]
    let _loader = {
        let loader = library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), false);
        assert!(loader.load());
        loader
    };

    #[cfg(target_os = "macos")]
    platform::setup_application();

    let _ = cef::api_hash(sys::CEF_API_VERSION_LAST, 0);

    let args = cef::args::Args::new();
    let Some(cmd_line) = args.as_cmd_line() else {
        return Err("failed to parse command line arguments");
    };

    let mut builder = EventLoop::builder();
    #[cfg(target_os = "linux")]
    builder.with_x11();
    let event_loop = builder.build().expect("failed to create Winit event loop");

    let proxy = event_loop.create_proxy();
    let (sender, receiver) = mpsc::channel();
    let mut app = NativeChildApp::new(sender.clone(), proxy.clone());
    let ret = cef::execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );

    let switch = cef::CefString::from("type");
    let is_browser_process = cmd_line.has_switch(Some(&switch)) != 1;
    if is_browser_process {
        assert_eq!(ret, -1, "cannot execute browser process");
    } else {
        assert!(ret >= 0, "cannot execute non-browser process");
        return Ok(());
    }

    let settings = cef::Settings {
        no_sandbox: 1,
        external_message_pump: 1,
        ..Default::default()
    };
    assert_eq!(
        cef::initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(&mut app),
            std::ptr::null_mut(),
        ),
        1
    );

    let winit_app = WinitHostApp::new(sender, receiver, proxy);
    event_loop
        .run_app(winit_app)
        .map_err(|_| "winit event loop failed")?;

    cef::shutdown();
    Ok(())
}
