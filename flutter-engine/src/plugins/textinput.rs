use std::cell::RefCell;
use glfw::{Modifiers};
use crate::FlutterEngineInner;
use super::{ Plugin, PlatformMessage};
use codec::{ MethodCall };
use serde_json::Value;
use utils::StringUtils;
use channel::{ Channel, JsonMethodChannel };

pub struct TextInputPlugin {
    client_id: Option<i64>,
    editing_state: RefCell<Option<TextEditingState>>,
    channel: JsonMethodChannel,
}

impl TextInputPlugin {
    pub fn new() -> TextInputPlugin {
        TextInputPlugin {
            client_id: None,
            editing_state: RefCell::new(None),
            channel: JsonMethodChannel::new("flutter/textinput"),
        }
    }
    pub fn with_state(&self, cbk: impl Fn(&mut TextEditingState)) {
        if let Ok(mut state) = self.editing_state.try_borrow_mut() {
            if let Some(state) = &mut *state {
                cbk(state);
            }
        }
    }
    fn get_lo_and_hi_idx(&self, s: &TextEditingState) -> (i64, i64) {
        let (lo, hi) = if s.selection_base <= s.selection_extent {
            (s.selection_base, s.selection_extent)
        } else {
            (s.selection_extent, s.selection_base)
        };
        return (lo, hi);
    }
    pub fn add_chars(&self, c: &str) {
        self.remove_selected_text();

        self.with_state(|s: &mut TextEditingState| {
            let mut text = String::from(s.text.substring(0, s.selection_base as usize));
            text.push_str(c);
            text.push_str(&s.text.substring(s.selection_base as usize, s.text.count()));
            s.text = text;
            s.selection_base += c.chars().count() as i64;
            s.selection_extent = s.selection_base;
        });
        self.notify_changes();
    }
    pub fn select_all(&self) {
        self.with_state(|s: &mut TextEditingState| {
            s.selection_base = 0;
            s.selection_extent = s.text.count() as i64;
            s.selection_is_directional = true;
        });
        self.notify_changes();
    }
    /// remove_selected_text do nothing if no text is selected
    /// return true if the state has been updated
    pub fn remove_selected_text(&self) -> bool {
        let mut ret = false;
        if let Some(s) = &mut *self.editing_state.borrow_mut() {
            if s.selection_base != s.selection_extent {
                let (lo, hi) = self.get_lo_and_hi_idx(s);
                s.text = String::from(s.text.substring(0, lo as usize))
                    + &s.text.substring(hi as usize, s.text.count());
                s.selection_base = lo;
                s.selection_extent = lo;
                s.selection_is_directional = false;
                ret = true;
            }
        }
        if ret {
            self.notify_changes();
        }

        ret
    }

    /// Delete char to the left of caret
    pub fn backspace(&self) {
        if !self.remove_selected_text() {
            self.with_state(|s: &mut TextEditingState| {
                if s.selection_base > 0 {
                    s.selection_base -= 1;
                    s.selection_extent = s.selection_base;
                    s.selection_is_directional = false;
                    s.text = String::from(s.text.substring(0, s.selection_base as usize))
                        + &s.text.substring(s.selection_extent as usize + 1, s.text.count());
                }
            });
            self.notify_changes();
        }
    }
    /// Delete char to the right of caret
    pub fn delete(&self) {
        if !self.remove_selected_text() {
            self.with_state(|s: &mut TextEditingState| {
                if s.selection_extent < s.text.count() as i64 {
                    s.selection_extent -= 1;
                    s.selection_is_directional = false;
                    s.text = String::from(s.text.substring(0, s.selection_base as usize))
                        + &s.text.substring(s.selection_extent as usize + 1, s.text.count());
                }
            });
            self.notify_changes();
        }
    }
    pub fn move_cursor_left(&self, modifiers: Modifiers) {
        self.with_state(|s: &mut TextEditingState| {
            let (lo, _) = self.get_lo_and_hi_idx(s);

            if modifiers.contains(Modifiers::Shift) {
                s.selection_is_directional = true;
                s.selection_extent = (s.selection_extent - 1).max(0);
            } else if s.selection_base != s.selection_extent {
                s.selection_base = lo;
                s.selection_extent = lo;
                s.selection_is_directional = false;
            } else {
                s.selection_extent = (lo - 1).max(0);
                s.selection_base = s.selection_extent;
                s.selection_is_directional = false;
            }
        });
        self.notify_changes();
    }
    pub fn move_cursor_right(&self, modifiers: glfw::Modifiers) {
        self.with_state(|s: &mut TextEditingState| {
            let (_, hi) = self.get_lo_and_hi_idx(s);

            if modifiers.contains(Modifiers::Shift) {
                s.selection_is_directional = true;
                s.selection_extent = (s.selection_extent + 1).min(s.text.count() as i64);
            } else if s.selection_base != s.selection_extent {
                s.selection_base = hi;
                s.selection_extent = hi;
                s.selection_is_directional = false;
            } else {
                s.selection_extent = (hi + 1).min(s.text.count() as i64);
                s.selection_base = s.selection_extent;
                s.selection_is_directional = false;
            }
        });
        self.notify_changes();
    }
    pub fn move_cursor_home(&self, modifiers: glfw::Modifiers) {
        self.with_state(|s: &mut TextEditingState| {
            if modifiers.contains(Modifiers::Shift) {
                s.selection_is_directional = true;
            } else {
                s.selection_base = 0;
                s.selection_is_directional = false;
            }
            s.selection_extent = 0;
        });
    }
    pub fn move_cursor_end(&self, modifiers: glfw::Modifiers) {
        self.with_state(|s: &mut TextEditingState| {
            if modifiers.contains(Modifiers::Shift) {
                s.selection_is_directional = true;
            } else {
                s.selection_base = s.text.count() as i64;
                s.selection_is_directional = false;
            }
            s.selection_extent = s.text.count() as i64;
        });
    }

    pub fn get_selected_text(&self) -> String {
        if let Some(s) = &mut *self.editing_state.borrow_mut() {
            if s.selection_base == s.selection_extent {
                return "".to_string();
            }

            let (lo, hi) = self.get_lo_and_hi_idx(s);
            s.text.substring(lo as usize, hi as usize).to_owned()
        } else {
            return "".to_string();
        }
    }

    pub fn perform_action(&self, action: &str) {
        self.channel.invoke_method(MethodCall {
            method: String::from("TextInputClient.performAction"),
            args: json!([self.client_id, "TextInputAction.".to_owned() + action])
        });
    }

    fn notify_changes(&self) {
        self.with_state(|s: &mut TextEditingState| {
            self.channel.invoke_method(MethodCall {
                method: String::from("TextInputClient.updateEditingState"),
                args: json!([self.client_id, s]),
            });
        });
    }
}

impl Plugin for TextInputPlugin {
    fn get_channel_mut(&mut self) -> &mut Channel {
        return &mut self.channel;
    }
    fn handle(&mut self, msg: &PlatformMessage, _: &FlutterEngineInner, _: &mut glfw::Window) {
        let decoded = self.channel.decode_method_call(msg);
        match decoded.method.as_str() {
            "TextInput.setClient" => {
                if let Value::Array(v) = &decoded.args {
                    if v.len() > 0 {
                        if let Some(n) = v[0].as_i64() {
                            self.client_id = Some(n);
                        }
                    }
                }
            },
            "TextInput.clearClient" => {
                self.client_id = None;
                self.editing_state.replace(None);
            },
            "TextInput.setEditingState" => {
                if self.client_id.is_some() {
                    self.editing_state.replace(TextEditingState::from(&decoded.args));
                }
            },
            "TextInput.show" => {},
            "TextInput.hide" => {},
            _ => {}
        }
    }
}

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TextEditingState {
    composing_base: i64,
    composing_extent: i64,
    selection_affinity: String,
    selection_base: i64,
    selection_extent: i64,
    selection_is_directional: bool,
    text: String,
}

impl TextEditingState {
    fn from(v: &Value) -> Option<Self> {
        if let Some(m) = v.as_object() {
            Some(Self {
                composing_base: m.get("composingBase").unwrap().as_i64().unwrap(),
                composing_extent: m.get("composingExtent").unwrap().as_i64().unwrap(),
                selection_affinity: String::from(m.get("selectionAffinity").unwrap().as_str().unwrap()),
                selection_base: m.get("selectionBase").unwrap().as_i64().unwrap(),
                selection_extent: m.get("selectionExtent").unwrap().as_i64().unwrap(),
                selection_is_directional: m.get("selectionIsDirectional").unwrap().as_bool().unwrap(),
                text: String::from(m.get("text").unwrap().as_str().unwrap()),
                .. Default::default()
            })
        } else {
            None
        }
    }
}