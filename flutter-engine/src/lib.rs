use std::sync::{mpsc::Receiver, Arc};

#[macro_use]
mod macros;

pub mod channel;
pub mod codec;
pub mod error;
mod flutter_callbacks;
pub mod plugins;
//pub mod texture_registry;
mod utils;
pub mod tasks;
pub mod ffi;


use std::{
    ffi::CString,
    mem, ptr,
    time::{SystemTime, UNIX_EPOCH},
};

use log::trace;

use flutter_engine_sys::FlutterTask;
use std::sync::{Weak, mpsc};
use std::os::raw::{c_char, c_void};
use parking_lot::RwLock;
use crate::plugins::{PluginRegistrar, Plugin};
use crate::tasks::{TaskRunner, TaskRunnerHandler};
use std::time::Instant;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::mpsc::Sender;
use crate::channel::Channel;
use crate::ffi::{FlutterPointerPhase, FlutterPointerMouseButtons, PlatformMessage, PlatformMessageResponseHandle, ExternalTexture, FlutterPointerSignalKind};


pub(crate) type MainThreadEngineFn = Box<dyn FnOnce(&FlutterEngine) + Send>;
//pub(crate) type MainThreadWindowFn = Box<dyn FnMut(&mut glfw::Window) + Send>;
pub(crate) type MainThreadChannelFn = (String, Box<dyn FnMut(&dyn Channel) + Send>);
//pub(crate) type MainThreadPlatformMsg = (String, Vec<u8>);
pub(crate) type MainThreadRenderThreadFn = Box<dyn FnOnce(&FlutterEngine) + Send>;
//pub(crate) type MainTheadWindowStateFn = Box<dyn FnMut(&mut DesktopWindowState) + Send>;

pub(crate) enum MainThreadCallback {
    EngineFn(MainThreadEngineFn),
//    WindowFn(MainThreadWindowFn),
    ChannelFn(MainThreadChannelFn),
//    PlatformMessage(MainThreadPlatformMsg),
    RenderThreadFn(MainThreadRenderThreadFn),
//    WindowStateFn(MainTheadWindowStateFn),
}

struct FlutterEngineInner {
    handler: Weak<dyn FlutterEngineHandler>,
    engine_ptr: AtomicPtr<flutter_engine_sys::_FlutterEngine>,
    plugins: RwLock<PluginRegistrar>,
    platform_runner: TaskRunner,
    _platform_runner_handler: Arc<PlatformRunnerHandler>,
    platform_receiver: Receiver<MainThreadCallback>,
    platform_sender: Sender<MainThreadCallback>,
}

pub struct FlutterEngineWeakRef {
    inner: Weak<FlutterEngineInner>,
}

impl FlutterEngineWeakRef {
    fn upgrade(&self) -> Option<FlutterEngine> {
        match self.inner.upgrade() {
            None => None,
            Some(arc) => Some(FlutterEngine {
                inner: arc
            }),
        }
    }
}

impl Default for FlutterEngineWeakRef {
    fn default() -> Self {
        Self {
            inner: Weak::new()
        }
    }
}

impl Clone for FlutterEngineWeakRef {
    fn clone(&self) -> Self {
        Self {
            inner: Weak::clone(&self.inner)
        }
    }
}

pub struct FlutterEngine {
    inner: Arc<FlutterEngineInner>,
}

impl Clone for FlutterEngine {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

pub trait FlutterEngineHandler {
    fn swap_buffers(&self) -> bool;

    fn make_current(&self) -> bool;

    fn clear_current(&self) -> bool;

    fn fbo_callback(&self) -> u32;

    fn make_resource_current(&self) -> bool;

    fn gl_proc_resolver(&self, proc: *const c_char) -> *mut c_void;

    fn wake_platform_thread(&self);

    fn run_in_background(&self, func: Box<dyn FnOnce()>);
}

struct PlatformRunnerHandler {
    handler: Weak<dyn FlutterEngineHandler>,
}

impl TaskRunnerHandler for PlatformRunnerHandler {
    fn wake(&self) {
        if let Some(handler) = self.handler.upgrade() {
            handler.wake_platform_thread();
        }
    }
}

impl FlutterEngine {
    pub fn new(handler: Weak<dyn FlutterEngineHandler>) -> Self {
        let platform_handler = Arc::new(PlatformRunnerHandler {
            handler: handler.clone()
        });

        let (main_tx, main_rx) = mpsc::channel();

        let engine = Self {
            inner: Arc::new(FlutterEngineInner {
                handler,
                engine_ptr: AtomicPtr::new(ptr::null_mut()),
                plugins: RwLock::new(PluginRegistrar::new()),
                platform_runner: TaskRunner::new(Arc::downgrade(&platform_handler) as Weak<dyn TaskRunnerHandler>),
                _platform_runner_handler: platform_handler,
                platform_receiver: main_rx,
                platform_sender: main_tx,
            })
        };

        let inner = &engine.inner;
        inner.plugins.write().init(engine.downgrade());
        inner.platform_runner.init(engine.downgrade());

        engine
    }

    #[inline]
    fn engine_ptr(&self) -> flutter_engine_sys::FlutterEngine {
        self.inner.engine_ptr.load(Ordering::Relaxed)
    }

    pub fn add_plugin<P>(&self, plugin: P) -> &Self
        where
            P: Plugin + 'static,
    {
        self.inner.plugins.write().add_plugin(plugin);
        self
    }

    pub fn downgrade(&self) -> FlutterEngineWeakRef {
        FlutterEngineWeakRef {
            inner: Arc::downgrade(&self.inner)
        }
    }
    
    pub fn run(&self,
        assets_path: String,
        icu_data_path: String,
        mut arguments: Vec<String>,
    ) -> Result<(), ()> {
        if !self.is_platform_thread() {
            panic!("Not on platform thread")
        }

        arguments.insert(0, "flutter-rs".into());
        let arguments = utils::CStringVec::new(&arguments);

        let renderer_config = flutter_engine_sys::FlutterRendererConfig {
            type_: flutter_engine_sys::FlutterRendererType::kOpenGL,
            __bindgen_anon_1: flutter_engine_sys::FlutterRendererConfig__bindgen_ty_1 {
                open_gl: flutter_engine_sys::FlutterOpenGLRendererConfig {
                    struct_size: std::mem::size_of::<flutter_engine_sys::FlutterOpenGLRendererConfig>(
                    ),
                    make_current: Some(flutter_callbacks::make_current),
                    clear_current: Some(flutter_callbacks::clear_current),
                    present: Some(flutter_callbacks::present),
                    fbo_callback: Some(flutter_callbacks::fbo_callback),
                    make_resource_current: Some(flutter_callbacks::make_resource_current),
                    fbo_reset_after_present: false,
                    surface_transformation: None,
                    gl_proc_resolver: Some(flutter_callbacks::gl_proc_resolver),
                    gl_external_texture_frame_callback: Some(
                        flutter_callbacks::gl_external_texture_frame,
                    ),
                },
            },
        };

        // TODO: Should be downgraded to a weak once weak::into_raw lands in stable
        let runner_ptr = {
            let arc = self.inner.platform_runner.clone().inner;
            Arc::into_raw(arc) as *mut std::ffi::c_void
        };

        let platform_task_runner = flutter_engine_sys::FlutterTaskRunnerDescription {
            struct_size: std::mem::size_of::<flutter_engine_sys::FlutterTaskRunnerDescription>(),
            user_data: runner_ptr,
            runs_task_on_current_thread_callback: Some(
                flutter_callbacks::runs_task_on_current_thread,
            ),
            post_task_callback: Some(flutter_callbacks::post_task),
        };
        let custom_task_runners = flutter_engine_sys::FlutterCustomTaskRunners {
            struct_size: std::mem::size_of::<flutter_engine_sys::FlutterCustomTaskRunners>(),
            platform_task_runner: &platform_task_runner
                as *const flutter_engine_sys::FlutterTaskRunnerDescription,
        };
        let project_args = flutter_engine_sys::FlutterProjectArgs {
            struct_size: std::mem::size_of::<flutter_engine_sys::FlutterProjectArgs>(),
            assets_path: CString::new(assets_path).unwrap().into_raw(),
            main_path__unused__: std::ptr::null(),
            packages_path__unused__: std::ptr::null(),
            icu_data_path: CString::new(icu_data_path).unwrap().into_raw(),
            command_line_argc: arguments.len() as i32,
            command_line_argv: arguments.into_raw(),
            platform_message_callback: Some(flutter_callbacks::platform_message_callback),
            vm_snapshot_data: std::ptr::null(),
            vm_snapshot_data_size: 0,
            vm_snapshot_instructions: std::ptr::null(),
            vm_snapshot_instructions_size: 0,
            isolate_snapshot_data: std::ptr::null(),
            isolate_snapshot_data_size: 0,
            isolate_snapshot_instructions: std::ptr::null(),
            isolate_snapshot_instructions_size: 0,
            root_isolate_create_callback: Some(flutter_callbacks::root_isolate_create_callback),
            update_semantics_node_callback: None,
            update_semantics_custom_action_callback: None,
            persistent_cache_path: std::ptr::null(),
            is_persistent_cache_read_only: false,
            vsync_callback: None,
            custom_dart_entrypoint: std::ptr::null(),
            custom_task_runners: &custom_task_runners
                as *const flutter_engine_sys::FlutterCustomTaskRunners,
        };

        unsafe {
            // TODO: Should be downgraded to a weak once weak::into_raw lands in stable
            let inner_ptr = Arc::into_raw(self.inner.clone()) as *mut std::ffi::c_void;

            let engine_ptr: flutter_engine_sys::FlutterEngine = std::ptr::null_mut();
            if flutter_engine_sys::FlutterEngineRun(
                1,
                &renderer_config,
                &project_args,
                inner_ptr,
                &engine_ptr as *const flutter_engine_sys::FlutterEngine
                    as *mut flutter_engine_sys::FlutterEngine,
            ) != flutter_engine_sys::FlutterEngineResult::kSuccess
                || engine_ptr.is_null()
            {
                Err(())
            } else {
                self.inner.engine_ptr.store(engine_ptr, Ordering::Relaxed);
                Ok(())
            }
        }
    }

    pub(crate) fn post_platform_callback(&self, callback: MainThreadCallback) {
        self.inner.platform_sender.send(callback).unwrap();
        self.inner.platform_runner.wake();
    }

    #[inline]
    fn is_platform_thread(&self) -> bool {
        self.inner.platform_runner.runs_task_on_current_thread()
    }

    pub fn run_on_platform_thread<F>(&self, f: F) where F: FnOnce(&FlutterEngine) -> () + 'static + Send {
        if self.is_platform_thread() {
            f(self);
        } else {
            self.post_platform_callback(MainThreadCallback::EngineFn(Box::new(f)));
        }
    }

    pub fn run_on_render_thread<F>(&self, f: F) where F: FnOnce(&FlutterEngine) -> () + 'static + Send {
        if self.is_platform_thread() {
            f(self);
        } else {
            self.post_platform_callback(MainThreadCallback::RenderThreadFn(Box::new(f)));
        }
    }

    pub(crate) fn run_in_background<F>(&self, func: F) where F : FnOnce() + 'static {
        if let Some(handler) = self.inner.handler.upgrade() {
            handler.run_in_background(Box::new(func));
        }
    }

    pub fn send_window_metrics_event(&self, width: i32, height: i32, pixel_ratio: f64) {
        if !self.is_platform_thread() {
            panic!("Not on platform thread")
        }

        let event = flutter_engine_sys::FlutterWindowMetricsEvent {
            struct_size: std::mem::size_of::<flutter_engine_sys::FlutterWindowMetricsEvent>(),
            width: width as usize,
            height: height as usize,
            pixel_ratio,
        };
        unsafe {
            flutter_engine_sys::FlutterEngineSendWindowMetricsEvent(self.engine_ptr(), &event);
        }
    }

    pub fn send_pointer_event(
        &self,
        phase: FlutterPointerPhase,
        x: f64,
        y: f64,
        signal_kind: FlutterPointerSignalKind,
        scroll_delta_x: f64,
        scroll_delta_y: f64,
        buttons: FlutterPointerMouseButtons,
    ) {
        if !self.is_platform_thread() {
            panic!("Not on platform thread")
        }

        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let buttons: flutter_engine_sys::FlutterPointerMouseButtons = buttons.into();
        let event = flutter_engine_sys::FlutterPointerEvent {
            struct_size: mem::size_of::<flutter_engine_sys::FlutterPointerEvent>(),
            timestamp: timestamp.as_micros() as usize,
            phase: phase.into(),
            x,
            y,
            device: 0,
            signal_kind: signal_kind.into(),
            scroll_delta_x,
            scroll_delta_y,
            device_kind:
            flutter_engine_sys::FlutterPointerDeviceKind::kFlutterPointerDeviceKindMouse,
            buttons: buttons as i64,
        };
        unsafe {
            flutter_engine_sys::FlutterEngineSendPointerEvent(self.engine_ptr(), &event, 1);
        }
    }

    pub(crate) fn send_platform_message(&self, message: PlatformMessage) {
        if !self.is_platform_thread() {
            panic!("Not on platform thread")
        }

        trace!("Sending message on channel {}", message.channel);
        unsafe {
            flutter_engine_sys::FlutterEngineSendPlatformMessage(self.engine_ptr(), &message.into());
        }
    }

    pub(crate) fn send_platform_message_response(
        &self,
        response_handle: PlatformMessageResponseHandle,
        bytes: &[u8],
    ) {
        if !self.is_platform_thread() {
            panic!("Not on platform thread")
        }

        trace!("Sending message response");
        unsafe {
            flutter_engine_sys::FlutterEngineSendPlatformMessageResponse(
                self.engine_ptr(),
                response_handle.into(),
                bytes.as_ptr(),
                bytes.len(),
            );
        }
    }

    pub fn shutdown(&self) {
        if !self.is_platform_thread() {
            panic!("Not on platform thread")
        }

        unsafe {
            flutter_engine_sys::FlutterEngineShutdown(self.engine_ptr());
        }
    }

    pub fn execute_platform_tasks(&self) -> Option<Instant> {
        if !self.is_platform_thread() {
            panic!("Not on platform thread")
        }

        let next_task = self.inner.platform_runner.execute_tasks();

        let mut render_thread_fns = Vec::new();
        let callbacks: Vec<MainThreadCallback> = self.inner.platform_receiver.try_iter().collect();
        for cb in callbacks {
            match cb {
                MainThreadCallback::EngineFn(func) => func(self),
//                MainThreadCallback::WindowFn(mut f) => f(self.window_ref.window()),
                MainThreadCallback::ChannelFn((name, mut f)) => {
                    self.inner.plugins.write()
                        .channel_registry
                        .with_channel(&name, |channel| {
                            f(channel);
                        });
                }
//                MainThreadCallback::PlatformMessage(msg) => {
//                    let platform_msg = crate::ffi::PlatformMessage {
//                        channel: msg.0.into(),
//                        message: &msg.1,
//                        response_handle: None,
//                    };
//                    self.init_data.engine.send_platform_message(platform_msg);
//                }
                MainThreadCallback::RenderThreadFn(f) => render_thread_fns.push(f),
//                MainThreadCallback::WindowStateFn(mut f) => f(self),
            }
        }
        if !render_thread_fns.is_empty() {
            let engine_copy = self.clone();
            self.post_render_thread_task(move || {
                for f in render_thread_fns {
                    f(&engine_copy);
                }
            });
        }

        next_task
    }

    pub(crate) fn run_task(&self, task: &FlutterTask) {
        unsafe {
            flutter_engine_sys::FlutterEngineRunTask(self.engine_ptr(), task as *const FlutterTask);
        }
    }

    fn post_render_thread_task<F>(&self, f: F)
        where
            F: FnOnce() -> () + 'static,
    {
        unsafe {
            let cbk = CallbackBox { cbk: Box::new(f) };
            let b = Box::new(cbk);
            let ptr = Box::into_raw(b);
            flutter_engine_sys::FlutterEnginePostRenderThreadTask(
                self.engine_ptr(),
                Some(render_thread_task),
                ptr as *mut libc::c_void,
            );
        }

        struct CallbackBox {
            pub cbk: Box<dyn FnOnce()>,
        }

        unsafe extern "C" fn render_thread_task(user_data: *mut libc::c_void) {
            let ptr = user_data as *mut CallbackBox;
            let b = Box::from_raw(ptr);
            (b.cbk)()
        }
    }

    pub fn register_external_texture(&self, texture_id: i64) -> ExternalTexture {
        trace!("registering new external texture with id {}", texture_id);
        unsafe {
            flutter_engine_sys::FlutterEngineRegisterExternalTexture(self.engine_ptr(), texture_id);
        }
        ExternalTexture {
            engine_ptr: self.engine_ptr(),
            texture_id,
        }
    }
}
