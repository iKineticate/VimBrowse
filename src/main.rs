#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod monitor;
mod uiaccess;

use hotkey::send_keys;
use hsv::hsv_to_rgb;
use monitor::get_primary_monitor_logical_size;
use uiaccess::prepare_uiaccess_token;
mod hotkey;

use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::Result;
use softbuffer::Surface;
use win_hotkeys::{HotkeyManager, VKey};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_A, VK_CONTROL, VK_D, VK_DOWN, VK_END, VK_F5, VK_HOME, VK_SHIFT, VK_T, VK_TAB, VK_UP, VK_W,
};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::windows::{WindowAttributesExtWindows, WindowExtWindows},
    window::{Window, WindowId, WindowLevel},
};

const SPEED: f64 = 0.1;

struct App {
    window: Option<Rc<Window>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    time: Instant,
    last_window_size: (u32, u32),
    border_width: u32,
    perimeter: f64,
    show_state: Arc<AtomicBool>,
}

impl App {
    fn create_window(&mut self, event_loop: &ActiveEventLoop) {
        let show_state = self.show_state.load(Ordering::Relaxed);
        let (monitor_width, monitor_height) = get_primary_monitor_logical_size().unwrap();

        if !show_state {
            if let Some(window) = self.window.take() {
                window.set_visible(false);
                self.window = None;
                self.surface = None;
            }
            return;
        }

        if self.window.is_none() {
            let window = event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("VimBrowse")
                        .with_skip_taskbar(!cfg!(debug_assertions))
                        .with_undecorated_shadow(cfg!(debug_assertions))
                        .with_content_protected(!cfg!(debug_assertions))
                        .with_decorations(false)
                        .with_window_level(WindowLevel::AlwaysOnTop)
                        .with_transparent(true)
                        .with_inner_size(PhysicalSize::new(monitor_width, monitor_height))
                        .with_position(PhysicalPosition::new(0, 0))
                        .with_active(false)
                        .with_resizable(false),
                )
                .unwrap();

            window.set_enable(false);
            window.set_cursor_hittest(false).unwrap();
            window.request_redraw();

            let (window, _context, mut surface) = {
                let window = Rc::new(window);
                let context = softbuffer::Context::new(window.clone())
                    .expect("Failed to create a new instance of context - {e}");
                let surface = softbuffer::Surface::new(&context, window.clone())
                    .expect("Failed to create a surface for drawing to window - {e}");
                (window, context, surface)
            };

            let (width, height): (u32, u32) = window.inner_size().into();

            surface
                .resize(
                    NonZeroU32::new(width).unwrap(),
                    NonZeroU32::new(height).unwrap(),
                )
                .expect("Failed to set the size of the buffer");

            let mut buffer = surface.buffer_mut().unwrap();

            buffer.fill(0);
            buffer.present().unwrap();

            self.window = Some(window);
            self.surface = Some(surface);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.create_window(event_loop)
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let window = match self.window.as_ref().filter(|w| w.id() == id) {
            Some(w) => w,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                sleep(Duration::from_millis(60));

                if !self.show_state.load(Ordering::Relaxed) {
                    return;
                }

                let (width, height) = {
                    let size = window.inner_size();
                    (size.width, size.height)
                };

                // 更新图形资源
                let surface = self.surface.as_mut().unwrap();
                if self.last_window_size != (width, height) {
                    surface
                        .resize(
                            NonZeroU32::new(width).unwrap(),
                            NonZeroU32::new(height).unwrap(),
                        )
                        .unwrap();
                    self.last_window_size = (width, height);
                }

                // 更新边框参数
                let scale_factor = window.scale_factor();
                self.border_width = (4.0 * scale_factor).round() as u32;
                self.perimeter =
                    2.0 * (width as f64 + height as f64 - 2.0 * self.border_width as f64);

                // 获取绘图缓冲区
                let mut buffer = surface.buffer_mut().unwrap();
                let buffer_len = (width * height) as usize;
                if buffer.len() != buffer_len {
                    return;
                }

                let elapsed = self.time.elapsed().as_secs_f64();
                let time_phase = (elapsed * SPEED) % 1.0;

                let buffer_slice = buffer.as_mut();
                let border_width = self.border_width;
                let perimeter = self.perimeter;

                let bottom_y = height - border_width;
                let right_x = width - border_width;
                buffer_slice.iter_mut().enumerate().for_each(|(i, pixel)| {
                    let x = i as u32 % width;
                    let y = i as u32 / width;

                    let in_top = y < border_width;
                    let in_bottom = y >= bottom_y;
                    let in_left = x < border_width;
                    let in_right = x >= right_x;

                    if in_top || in_bottom || in_left || in_right {
                        let pos = match () {
                            _ if in_top => x as f64,
                            _ if in_right => width as f64 + (y - border_width) as f64,
                            _ if in_bottom => {
                                width as f64
                                    + (height - 2 * border_width) as f64
                                    + (width - x - 1) as f64
                            }
                            _ => {
                                (2 * width + height - 2 * border_width) as f64
                                    + (height - y - border_width - 1) as f64
                            }
                        } / perimeter;

                        let phase = (pos + time_phase) % 1.0;
                        let rgb = hsv_to_rgb(phase * 360.0, 1.0, 1.0);
                        *pixel = ((rgb.0 as u32) << 16) | ((rgb.1 as u32) << 8) | rgb.2 as u32;
                    }
                });

                buffer.present().unwrap();

                if self.show_state.load(Ordering::Relaxed) {
                    window.request_redraw();
                }
            }
            _ => (),
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: ()) {
        self.create_window(event_loop);
    }
}

fn main() -> Result<()> {
    let _ = prepare_uiaccess_token().inspect(|_| println!("Successful acquisition of Uiaccess"));

    let show_state = Arc::new(AtomicBool::new(true));
    let show_state_clone = Arc::clone(&show_state);

    let event_loop = EventLoop::new()?;
    let event_loop_proxy = event_loop.create_proxy();

    std::thread::spawn(move || listen_and_send(show_state_clone, event_loop_proxy));

    let mut app = App {
        window: None,
        surface: None,
        time: Instant::now(),
        last_window_size: (0, 0),
        border_width: 4,
        perimeter: 0.0,
        show_state,
    };
    event_loop.run_app(&mut app).unwrap();

    Ok(())
}

fn listen_and_send(
    show_state: Arc<AtomicBool>,
    event_loop_proxy: winit::event_loop::EventLoopProxy<()>,
) {
    let show_state = Arc::clone(&show_state);

    // 睡眠唤醒后键盘钩子失效，解决办法：重启键盘钩子？
    let mut hkm = HotkeyManager::new();

    // 返回顶部
    hkm.register_hotkey(VKey::Q, &[], move || {
        send_keys(&[VK_CONTROL, VK_HOME]);
    })
    .unwrap();

    // 返回底部
    hkm.register_hotkey(VKey::E, &[], move || {
        send_keys(&[VK_CONTROL, VK_END]);
    })
    .unwrap();

    // 关闭应用内窗口
    hkm.register_hotkey(VKey::X, &[], move || {
        send_keys(&[VK_CONTROL, VK_W]);
    })
    .unwrap();

    // 创建应用内窗口
    hkm.register_hotkey(VKey::T, &[], move || {
        send_keys(&[VK_CONTROL, VK_T]);
    })
    .unwrap();

    // 上
    hkm.register_hotkey(VKey::W, &[], move || {
        send_keys(&[VK_UP]);
    })
    .unwrap();

    // 下
    hkm.register_hotkey(VKey::S, &[], move || {
        send_keys(&[VK_DOWN]);
    })
    .unwrap();

    // 切换左标题页
    hkm.register_hotkey(VKey::A, &[], move || {
        send_keys(&[VK_CONTROL, VK_SHIFT, VK_TAB, VK_A]);
    })
    .unwrap();

    // 切换左标题页
    hkm.register_hotkey(VKey::D, &[], move || {
        send_keys(&[VK_CONTROL, VK_TAB, VK_D]);
    })
    .unwrap();

    // 刷新
    hkm.register_hotkey(VKey::R, &[], move || {
        send_keys(&[VK_F5]);
    })
    .unwrap();

    // 暂停/启动
    hkm.register_pause_hotkey(VKey::F23, &[VKey::LWin, VKey::Shift], move || {
        show_state.store(!show_state.load(Ordering::Relaxed), Ordering::Relaxed);
        event_loop_proxy.send_event(()).unwrap();
    })
    .unwrap();

    hkm.event_loop();
}