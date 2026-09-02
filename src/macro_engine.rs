use crate::config::{AppConfig, MacroSlot};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacroAction {
    ModifierDown(&'static str),
    KeyDown(String),
    Hold(u64),
    KeyUp(String),
    ModifierUp(&'static str),
    Delay(u64),
}

pub fn actions(config: &AppConfig) -> Vec<MacroAction> {
    let mut result = Vec::new();
    let enabled: Vec<&MacroSlot> = config
        .slots
        .iter()
        .filter(|s| s.enabled && s.key.is_some())
        .collect();
    for (n, slot) in enabled.iter().enumerate() {
        let key = slot.key.as_ref().unwrap();
        if key.ctrl {
            result.push(MacroAction::ModifierDown("Ctrl"));
        }
        if key.shift {
            result.push(MacroAction::ModifierDown("Shift"));
        }
        result.push(MacroAction::KeyDown(key.label.clone()));
        result.push(MacroAction::Hold(slot.press_duration_ms));
        result.push(MacroAction::KeyUp(key.label.clone()));
        if key.shift {
            result.push(MacroAction::ModifierUp("Shift"));
        }
        if key.ctrl {
            result.push(MacroAction::ModifierUp("Ctrl"));
        }
        result.push(MacroAction::Delay(slot.release_delay_ms));
        if n + 1 < enabled.len() {
            result.push(MacroAction::Delay(slot.interval_ms));
        }
    }
    result
}

pub fn wait(ms: u64, stop: &AtomicBool) -> bool {
    let mut left = ms;
    while left > 0 {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        let step = left.min(5);
        thread::sleep(Duration::from_millis(step));
        left -= step;
    }
    true
}
pub fn run<F>(config: &AppConfig, stop: Arc<AtomicBool>, mut perform: F) -> bool
where
    F: FnMut(&MacroAction) -> bool,
{
    for action in actions(config) {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        match action {
            MacroAction::Hold(ms) | MacroAction::Delay(ms) => {
                if !wait(ms, &stop) {
                    return false;
                }
            }
            other => {
                if !perform(&other) {
                    return false;
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, KeyBinding};
    #[test]
    fn builds_modifier_order() {
        let mut c = AppConfig::default();
        c.slots[0].key = Some(KeyBinding {
            label: "A".into(),
            ctrl: true,
            shift: true,
        });
        let a = actions(&c);
        assert_eq!(a[0], MacroAction::ModifierDown("Ctrl"));
        assert_eq!(a[1], MacroAction::ModifierDown("Shift"));
        assert_eq!(a[2], MacroAction::KeyDown("A".into()));
        assert!(a.contains(&MacroAction::ModifierUp("Ctrl")));
    }
}
