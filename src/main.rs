#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod uiaccess;

use rgb::RGB;
use uiaccess::prepare_uiaccess_token;

use std::sync::{Arc, Mutex};
use std::num::NonZeroU32;

use anyhow::Result;
use win_hotkeys::{VKey, HotkeyManager};
use winit::{
    platform::windows::{WindowAttributesExtWindows, WindowExtWindows},
    event::WindowEvent,
    application::ApplicationHandler,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId, WindowLevel},
};

use std::rc::Rc;

const SPEED: f32 = 0.1;

struct App {
    window: Option<Rc<Window>>,
    time: std::time::Instant,
    shortcut_key: Arc<Mutex<bool>>,
}

impl App {
    fn create_window(&mut self, event_loop: &ActiveEventLoop) {
        let state = self.shortcut_key.lock().unwrap();

        if *state {
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
                            .with_maximized(true)
                            .with_active(false)
                            .with_resizable(false)
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
            }
        } else {
            self.window = None
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.create_window(event_loop)
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let window = if let Some(window) = self.window.as_ref().filter(|w| w.id() == id) {
            window
        } else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            },
            WindowEvent::RedrawRequested => {
                let (width, height) = {
                    let size = window.inner_size();
                    (size.width, size.height)
                };

                let scale_factor = window.scale_factor();

                let border_width = (4.0 * scale_factor).round() as u32;

                let (window, _context, mut surface) = {
                    let window = Rc::new(window);
                    let context = softbuffer::Context::new(window.clone())
                        .expect("Failed to create a new instance of context - {e}");
                    let surface = softbuffer::Surface::new(&context, window.clone())
                        .expect("Failed to create a surface for drawing to window - {e}");
                    (window, context, surface)
                };
                surface
                    .resize(
                        NonZeroU32::new(width).unwrap(),
                        NonZeroU32::new(height).unwrap(),
                    )
                    .unwrap();

                let mut buffer = surface.buffer_mut().unwrap();

                // buffer.fill(0);

                let buffer_len = (width * height) as usize;
                if buffer.len() != buffer_len {
                    return;
                    // buffer.resize(buffer_len, 0x000000);
                }

                let perimeter = 2.0 * (width as f32 + height as f32 - 2.0 * border_width as f32);
                if perimeter == 0.0 {
                    return;
                }

                // let start_time = std::time::Instant::now();
                let elapsed = self.time.elapsed().as_secs_f32();
                // self.time = elapsed;

                let time_phase = (elapsed * SPEED) % 1.0;

                // 绘制上边框
                for y in 0..border_width {
                    for x in 0..width {
                        let p = x as f32;
                        let pos = p / perimeter;
                        let phase = (pos + time_phase) % 1.0;
                        let rgb = hsv_to_rgb(phase * 360.0, 1.0, 1.0);
                        let color = (rgb.r as u32) << 16 | (rgb.g as u32) << 8 | rgb.b as u32;
                        let index = y * width + x;
                        buffer[index as usize] = color;
                    }
                }

                // 绘制右边框
                let right_x_start = width.saturating_sub(border_width);
                for x in right_x_start..width {
                    for y in border_width..height.saturating_sub(border_width) {
                        let p = width as f32 + (y - border_width) as f32;
                        let pos = p / perimeter;
                        let phase = (pos + time_phase) % 1.0;
                        let rgb = hsv_to_rgb(phase * 360.0, 1.0, 1.0);
                        let color = (rgb.r as u32) << 16 | (rgb.g as u32) << 8 | rgb.b as u32;
                        let index = y * width + x;
                        buffer[index as usize] = color;
                    }
                }

                // 绘制下边框
                let bottom_y_start = height.saturating_sub(border_width);
                for y in bottom_y_start..height {
                    for x in 0..width {
                        let reversed_x = width - 1 - x;
                        let p = width as f32 + (height - 2 * border_width) as f32 + reversed_x as f32;
                        let pos = p / perimeter;
                        let phase = (pos + time_phase) % 1.0;
                        let rgb = hsv_to_rgb(phase * 360.0, 1.0, 1.0);
                        let color = (rgb.r as u32) << 16 | (rgb.g as u32) << 8 | rgb.b as u32;
                        let index = y * width + x;
                        buffer[index as usize] = color;
                    }
                }

                // 绘制左边框
                for x in 0..border_width {
                    for y in border_width..height.saturating_sub(border_width) {
                        let reversed_y = (height - 2 * border_width - 1) - (y - border_width);
                        let p = (2 * width + height - 2 * border_width) as f32 + reversed_y as f32;
                        let pos = p / perimeter;
                        let phase = (pos + time_phase) % 1.0;
                        let rgb = hsv_to_rgb(phase * 360.0, 1.0, 1.0);
                        let color = (rgb.r as u32) << 16 | (rgb.g as u32) << 8 | rgb.b as u32;
                        let index = y * width + x;
                        buffer[index as usize] = color;
                    }
                }

                buffer.present().unwrap();

                window.request_redraw();
            },
            _ => (),
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: ()) {
        self.create_window(event_loop);
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> RGB<u8> {
    let h = h % 360.0;
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    RGB {
        r: ((r + m) * 255.0).round() as u8,
        g: ((g + m) * 255.0).round() as u8,
        b: ((b + m) * 255.0).round() as u8,
    }
}

fn main() -> Result<()> {
    let _ = prepare_uiaccess_token().inspect(|_| println!("Successful acquisition of Uiaccess"));

    let shortcut_key = Arc::new(Mutex::new(false));
    let shortcut_key_clone = Arc::clone(&shortcut_key);

    let event_loop = EventLoop::new()?;
    let event_loop_proxy = event_loop.create_proxy();

    std::thread::spawn(move || {
        let mut hkm = HotkeyManager::new();

        let shortcut_key = Arc::clone(&shortcut_key_clone);
        hkm.register_hotkey(VKey::F23, &[VKey::LWin, VKey::Shift], move || {
            let key = Arc::clone(&shortcut_key);
            let mut key_guard = key.lock().unwrap();
            *key_guard = !*key_guard;
            event_loop_proxy.send_event(()).unwrap();
        }).unwrap();

        hkm.event_loop();
    });

    let mut app = App {
        window: None,
        time: std::time::Instant::now(),
        shortcut_key: Arc::clone(&shortcut_key),
    };
    event_loop.run_app(&mut app).unwrap();

    Ok(())
}