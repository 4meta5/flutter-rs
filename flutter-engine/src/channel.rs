//! Plugins use MethodChannel to interop with flutter/dart.
//! It contains two implementations StandardMethodChannel using binary encoding
//! and JsonMethodChannel using json encoding.

pub use self::{
    event_channel::EventChannel,
    json_method_channel::JsonMethodChannel,
    message_channel::MessageChannel,
    registry::{ChannelRegistrar, ChannelRegistry},
    standard_method_channel::StandardMethodChannel,
};
use crate::{
    codec::{MethodCall, MethodCallResult, MethodCodec, Value},
    desktop_window_state::{InitData, RuntimeData},
    error::MethodCallError,
    ffi::{PlatformMessage, PlatformMessageResponseHandle},
};

use std::{
    borrow::Cow,
    sync::{Arc, RwLock, Weak},
};

use log::{error, trace};
use tokio::prelude::Future;

mod event_channel;
mod json_method_channel;
mod message_channel;
mod registry;
mod standard_method_channel;

pub trait Channel {
    fn name(&self) -> &'static str;
    fn init_data(&self) -> Option<Arc<InitData>>;
    fn init(&mut self, runtime_data: Weak<InitData>, plugin_name: &'static str);
    fn method_handler(&self) -> Option<Arc<RwLock<MethodCallHandler + Send + Sync>>>;
    fn plugin_name(&self) -> &'static str;
    fn codec(&self) -> &MethodCodec;

    /// Handle a method call received on this channel
    fn handle_method(&self, mut msg: PlatformMessage) {
        debug_assert_eq!(msg.channel, self.name());
        if let Some(handler) = self.method_handler() {
            if let Some(init_data) = self.init_data() {
                let runtime_data = (*init_data.runtime_data).clone();
                let call = self.decode_method_call(&msg).unwrap();
                let channel = self.name();
                trace!(
                    "on channel {}, got method call {} with args {:?}",
                    channel,
                    call.method,
                    call.args
                );
                let plugin_name = self.plugin_name();
                let mut response_handle = msg.response_handle.take();
                init_data
                    .runtime_data
                    .task_executor
                    .spawn(tokio::prelude::future::ok(()).map(move |_| {
                        let mut handler = handler.write().unwrap();
                        let method = call.method.clone();
                        let tx = runtime_data.main_thread_sender.clone();
                        let result = handler.on_method_call(call, runtime_data);
                        let response = match result {
                            Ok(value) => MethodCallResult::Ok(value),
                            Err(error) => {
                                error!(
                                target: handler
                                    .log_target()
                                    .unwrap_or(plugin_name),
                                "error in method call {}#{}: {}",
                                channel,
                                method,
                                error);
                                error.into()
                            }
                        };
                        tx.send(crate::desktop_window_state::MainThreadCallback::ChannelFn(
                            (
                                channel,
                                Box::new(move |channel| {
                                    channel
                                        .send_method_call_response(&mut response_handle, &response);
                                }),
                            ),
                        ))
                        .unwrap();
                    }));
            }
        }
    }

    /// Invoke a flutter method using this channel
    fn invoke_method(&self, method_call: MethodCall) {
        let buf = self.codec().encode_method_call(&method_call);
        self.send_buffer(&buf);
    }

    /// Send a plain value on this channel.
    fn send(&self, value: &Value) {
        let buf = self.codec().encode_message(value);
        self.send_buffer(&buf);
    }

    /// Send a buffer. This is a low level method. Please use one of the other send_* methods.
    fn send_buffer(&self, buf: &[u8]) {
        self.send_platform_message(PlatformMessage {
            channel: Cow::Borrowed(self.name()),
            message: &buf,
            response_handle: None,
        });
    }

    /// Decode dart method call
    fn decode_method_call(&self, msg: &PlatformMessage) -> Option<MethodCall> {
        self.codec().decode_method_call(msg.message)
    }

    /// When flutter listen to a stream of events using EventChannel.
    /// This method send back a success event.
    /// It can be call multiple times to simulate stream.
    fn send_success_event(&self, data: &Value) {
        let buf = self.codec().encode_success_envelope(data);
        self.send_buffer(&buf);
    }

    /// When flutter listen to a stream of events using EventChannel.
    /// This method send back a error event.
    /// It can be call multiple times to simulate stream.
    fn send_error_event(&self, code: &str, message: &str, data: &Value) {
        let buf = self.codec().encode_error_envelope(code, message, data);
        self.send_buffer(&buf);
    }

    /// Send a method call response
    fn send_method_call_response(
        &self,
        response_handle: &mut Option<PlatformMessageResponseHandle>,
        response: &MethodCallResult,
    ) {
        if let Some(response_handle) = response_handle.take() {
            let buf = match response {
                MethodCallResult::Ok(data) => self.codec().encode_success_envelope(data),
                MethodCallResult::Err {
                    code,
                    message,
                    details,
                } => self.codec().encode_error_envelope(code, message, details),
                MethodCallResult::NotImplemented => vec![],
            };
            self.send_response(response_handle, &buf);
        }
    }

    /// When flutter call a method using MethodChannel,
    /// it can wait for rust response using await syntax.
    /// This method send a response to flutter. This is a low level method.
    /// Please use send_method_call_response if that will work.
    fn send_response(&self, response_handle: PlatformMessageResponseHandle, buf: &[u8]) {
        if let Some(init_data) = self.init_data() {
            init_data
                .engine
                .send_platform_message_response(response_handle, buf);
        } else {
            error!("Channel {} was not initialized", self.name());
        }
    }

    /// Send a platform message over this channel. This is a low level method.
    fn send_platform_message(&self, message: PlatformMessage) {
        if let Some(init_data) = self.init_data() {
            init_data.engine.send_platform_message(message);
        } else {
            error!("Channel {} was not initialized", self.name());
        }
    }
}

pub trait MethodCallHandler {
    fn log_target(&self) -> Option<&'static str> {
        None
    }

    fn on_method_call(
        &mut self,
        call: MethodCall,
        runtime_data: RuntimeData,
    ) -> Result<Value, MethodCallError>;
}

pub trait EventHandler {
    fn on_listen(
        &mut self,
        args: Value,
        runtime_data: RuntimeData,
    ) -> Result<Value, MethodCallError>;
    fn on_cancel(&mut self, runtime_data: RuntimeData) -> Result<Value, MethodCallError>;
}
