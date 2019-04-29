//! Register plugin with this registry to listen to flutter MethodChannel calls.

mod navigation;
mod platform;
pub mod prelude;
mod textinput;

pub use self::{
    navigation::NavigationPlugin, platform::PlatformPlugin, textinput::TextInputPlugin,
};

use crate::{channel::ChannelRegistrar, desktop_window_state::RuntimeData, ffi::PlatformMessage};

use std::{
    any::Any,
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::{Arc, RwLock, Weak},
};

use log::{trace, warn};

pub struct PluginRegistrar {
    plugins: HashMap<String, Arc<RwLock<dyn Any>>>,
    runtime_data: Weak<RuntimeData>,
    channel_registrar: ChannelRegistrar,
}

impl PluginRegistrar {
    pub fn new(runtime_data: Weak<RuntimeData>) -> Self {
        Self {
            plugins: HashMap::new(),
            channel_registrar: ChannelRegistrar::new(runtime_data.clone()),
            runtime_data,
        }
    }

    pub fn add_system_plugins(&mut self) {
        self.add_plugin(platform::PlatformPlugin::new())
            .add_plugin(textinput::TextInputPlugin::new())
            .add_plugin(navigation::NavigationPlugin::new());
    }

    pub fn add_plugin<P>(&mut self, plugin: P) -> &mut Self
    where
        P: Plugin + 'static,
    {
        let arc = Arc::new(RwLock::new(plugin));
        {
            let mut plugin = arc.write().unwrap();
            plugin.init_channels(Arc::downgrade(&arc), &mut self.channel_registrar);
        }
        self.plugins.insert(P::plugin_name().to_owned(), arc);
        self
    }

    pub fn handle(&mut self, mut message: PlatformMessage) {
        self.channel_registrar.handle(message);
    }

    pub fn with_plugin<F, P>(&self, mut f: F)
    where
        F: FnMut(&P),
        P: Plugin + 'static,
    {
        if let Some(arc) = self.plugins.get(P::plugin_name()) {
            let plugin = arc.read().unwrap();
            let plugin = plugin.deref().downcast_ref::<P>().unwrap();
            f(plugin);
        }
    }

    pub fn with_plugin_mut<F, P>(&mut self, mut f: F)
    where
        F: FnMut(&mut P),
        P: Plugin + 'static,
    {
        if let Some(arc) = self.plugins.get_mut(P::plugin_name()) {
            let mut plugin = arc.write().unwrap();
            let plugin = plugin.deref_mut().downcast_mut::<P>().unwrap();
            f(plugin);
        }
    }
}

pub trait Plugin {
    fn plugin_name() -> &'static str;
    fn init_channels(&mut self, plugin: Weak<RwLock<Self>>, registrar: &mut ChannelRegistrar);
}
