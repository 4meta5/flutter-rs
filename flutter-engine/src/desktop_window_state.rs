use crate::{
    ffi::{FlutterEngine, FlutterPointerPhase, FlutterPointerSignalKind},
    plugins::PluginRegistrar,
};

use std::{rc::Rc, sync::mpsc::Receiver};

const DP_PER_INCH: f64 = 160.0;

pub struct DesktopWindowState {
    pub runtime_data: Rc<RuntimeData>,
    pointer_currently_added: bool,
    monitor_screen_coordinates_per_inch: f64,
    window_pixels_per_screen_coordinate: f64,
    pub plugin_registrar: PluginRegistrar,
}

pub struct RuntimeData {
    pub window: *mut glfw::Window,
    pub window_event_receiver: Receiver<(f64, glfw::WindowEvent)>,
    pub engine: Rc<FlutterEngine>,
}

impl RuntimeData {
    #[allow(clippy::mut_from_ref)]
    pub fn window(&self) -> &mut glfw::Window {
        unsafe { &mut *self.window }
    }
}

impl DesktopWindowState {
    pub fn new(
        window_ref: *mut glfw::Window,
        window_event_receiver: Receiver<(f64, glfw::WindowEvent)>,
        engine: FlutterEngine,
    ) -> Self {
        let runtime_data = Rc::new(RuntimeData {
            window: window_ref,
            window_event_receiver,
            engine: Rc::new(engine),
        });
        let monitor_screen_coordinates_per_inch =
            Self::get_screen_coordinates_per_inch(&mut runtime_data.window().glfw);
        Self {
            pointer_currently_added: false,
            monitor_screen_coordinates_per_inch,
            window_pixels_per_screen_coordinate: 0.0,
            plugin_registrar: PluginRegistrar::new(Rc::downgrade(&runtime_data)),
            runtime_data,
        }
    }

    pub fn send_framebuffer_size_change(&mut self, framebuffer_size: (i32, i32)) {
        let window_size = self.runtime_data.window().get_size();
        self.window_pixels_per_screen_coordinate = framebuffer_size.0 as f64 / window_size.0 as f64;
        let dpi =
            self.window_pixels_per_screen_coordinate * self.monitor_screen_coordinates_per_inch;
        let pixel_ratio = (dpi / DP_PER_INCH).max(1.0);
        self.runtime_data.engine.send_window_metrics_event(
            framebuffer_size.0,
            framebuffer_size.1,
            pixel_ratio,
        );
    }

    fn get_screen_coordinates_per_inch(glfw: &mut glfw::Glfw) -> f64 {
        glfw.with_primary_monitor(|glfw, monitor| match monitor {
            None => DP_PER_INCH,
            Some(monitor) => match monitor.get_video_mode() {
                None => DP_PER_INCH,
                Some(video_mode) => {
                    let (width, _) = monitor.get_physical_size();
                    video_mode.width as f64 / (width as f64 / 25.4)
                }
            },
        })
    }

    fn send_pointer_event(
        &mut self,
        phase: FlutterPointerPhase,
        x: f64,
        y: f64,
        signal_kind: FlutterPointerSignalKind,
        scroll_delta_x: f64,
        scroll_delta_y: f64,
    ) {
        if !self.pointer_currently_added && phase != FlutterPointerPhase::Add {
            self.send_pointer_event(
                FlutterPointerPhase::Add,
                x,
                y,
                FlutterPointerSignalKind::None,
                0.0,
                0.0,
            );
        }
        if self.pointer_currently_added && phase == FlutterPointerPhase::Add {
            return;
        }

        self.runtime_data.engine.send_pointer_event(
            phase,
            x,
            y,
            signal_kind,
            scroll_delta_x,
            scroll_delta_y,
        );

        match phase {
            FlutterPointerPhase::Add => self.pointer_currently_added = true,
            FlutterPointerPhase::Remove => self.pointer_currently_added = false,
            _ => {}
        }
    }

    pub fn handle_glfw_event(&mut self, event: glfw::WindowEvent) {
        match event {
            glfw::WindowEvent::CursorEnter(entered) => {
                let cursor_pos = self.runtime_data.window().get_cursor_pos();
                self.send_pointer_event(
                    if entered {
                        FlutterPointerPhase::Add
                    } else {
                        FlutterPointerPhase::Remove
                    },
                    cursor_pos.0,
                    cursor_pos.1,
                    FlutterPointerSignalKind::None,
                    0.0,
                    0.0,
                );
            }
            glfw::WindowEvent::CursorPos(x, y) => {
                let phase = if self
                    .runtime_data
                    .window()
                    .get_mouse_button(glfw::MouseButtonLeft)
                    == glfw::Action::Press
                {
                    FlutterPointerPhase::Move
                } else {
                    FlutterPointerPhase::Hover
                };
                self.send_pointer_event(phase, x, y, FlutterPointerSignalKind::None, 0.0, 0.0);
            }
            glfw::WindowEvent::MouseButton(glfw::MouseButtonLeft, action, _modifiers) => {
                let (x, y) = self.runtime_data.window().get_cursor_pos();
                let phase = if action == glfw::Action::Press {
                    FlutterPointerPhase::Down
                } else {
                    FlutterPointerPhase::Up
                };
                self.send_pointer_event(phase, x, y, FlutterPointerSignalKind::None, 0.0, 0.0);
            }
            _ => {}
        }
    }
}
