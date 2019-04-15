//! Register plugin with this registry to listen to flutter MethodChannel calls.

//pub mod platform;
//pub mod textinput;
//pub mod dialog;
//pub mod window;
//pub mod navigation;

//use super::FlutterEngine;
//use flutter_engine_sys::{FlutterPlatformMessage, FlutterPlatformMessageResponseHandle};
//use std::{borrow::Cow, collections::HashMap, ffi::CString, mem, ptr::null, sync::Arc, sync::Weak};

use crate::ffi::PlatformMessage;

use std::{
    borrow::{Borrow, BorrowMut},
    collections::HashMap,
};

use log::{trace, warn};

pub struct PluginRegistrar {
    plugins: HashMap<String, Box<dyn Plugin>>,
}

impl PluginRegistrar {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn add_plugin<P>(&mut self, plugin: P)
    where
        P: Plugin + PluginChannel + 'static,
    {
        plugin.init_channel(self);
        self.plugins
            .insert(P::channel_name().to_owned(), Box::new(plugin));
    }

    pub fn handle(&mut self, message: PlatformMessage) {
        let mut message_handled = false;
        for (channel, plugin) in &mut self.plugins {
            if channel == &message.channel {
                trace!("Processing message from channel: {}", channel);
                plugin.handle(&message);
                message_handled = true;
                break;
            }
        }
        if !message_handled {
            warn!(
                "No plugin registered to handle messages from channel: {}",
                &message.channel
            );
        }
    }

    pub fn get_plugin<P>(&self) -> Option<&P>
    where
        P: Plugin + PluginChannel + 'static,
        Box<dyn Plugin>: BorrowMut<P>,
    {
        self.plugins.get(P::channel_name()).map(Box::borrow)
    }

    pub fn get_plugin_mut<P>(&mut self) -> Option<&mut P>
    where
        P: Plugin + PluginChannel + 'static,
        Box<dyn Plugin>: BorrowMut<P>,
    {
        self.plugins.get_mut(P::channel_name()).map(Box::borrow_mut)
    }
}

pub trait PluginChannel {
    fn channel_name() -> &'static str;
}

pub trait Plugin {
    fn init_channel(&self, plugin_registrar: &PluginRegistrar);
    fn handle(&mut self, message: &PlatformMessage);
}
