#![cfg(target_os = "windows")]

use crate::logger;
use anyhow::{Context, Result};
use windows::Win32::{
    Foundation::HWND,
    System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    },
    UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern, TreeScope,
        UIA_InvokePatternId,
    },
};

/// Invoke an OSK key through UI Automation. This never moves or clicks the mouse.
pub fn invoke_control(parent: HWND, wanted: &str) -> Result<()> {
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    initialized
        .ok()
        .context("could not initialize COM for Windows UI Automation")?;

    let result = unsafe { invoke_control_inner(parent, wanted) };
    unsafe {
        CoUninitialize();
    }
    result
}

unsafe fn invoke_control_inner(parent: HWND, wanted: &str) -> Result<()> {
    let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL)
        .context("could not create the Windows UI Automation client")?;
    let root = automation
        .ElementFromHandle(parent)
        .context("could not get the OSK UI Automation root")?;
    let Some(element) = find_named_element(&automation, &root, wanted)? else {
        anyhow::bail!("OSK UI Automation key '{wanted}' was not found");
    };
    let invoke: IUIAutomationInvokePattern = element
        .GetCurrentPatternAs(UIA_InvokePatternId)
        .with_context(|| format!("OSK key '{wanted}' does not support UI Automation Invoke"))?;
    invoke
        .Invoke()
        .with_context(|| format!("could not invoke OSK key '{wanted}'"))?;
    logger::log(format!("OSK input: invoked UI Automation key '{wanted}'"));
    Ok(())
}

unsafe fn find_named_element(
    automation: &IUIAutomation,
    root: &IUIAutomationElement,
    wanted: &str,
) -> Result<Option<IUIAutomationElement>> {
    let condition = automation
        .CreateTrueCondition()
        .context("could not create the UI Automation search condition")?;
    let elements = root
        .FindAll(TreeScope(4), &condition)
        .context("could not enumerate OSK UI Automation elements")?;
    let count = elements
        .Length()
        .context("could not read UI Automation element count")?;
    let mut discovered_names = Vec::new();
    for index in 0..count {
        let element = elements
            .GetElement(index)
            .context("could not read an OSK UI Automation element")?;
        let name = element
            .CurrentName()
            .map(|value| value.to_string())
            .unwrap_or_default();
        if !name.is_empty() && discovered_names.len() < 24 {
            discovered_names.push(name.clone());
        }
        if name_matches(&name, wanted) {
            return Ok(Some(element));
        }
    }
    logger::log(format!(
        "OSK input: UI Automation key '{wanted}' was not found; discovered names: {}",
        discovered_names.join(" | ")
    ));
    Ok(None)
}

fn name_matches(name: &str, wanted: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    let wanted = wanted.trim().to_ascii_lowercase();
    if name == wanted {
        return true;
    }

    let wanted = canonical_key_name(&wanted);
    name.split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .any(|part| canonical_key_name(part) == wanted)
}

fn canonical_key_name(value: &str) -> String {
    match value {
        "control" => "ctrl",
        "zero" => "0",
        "one" => "1",
        "two" => "2",
        "three" => "3",
        "four" => "4",
        "five" => "5",
        "six" => "6",
        "seven" => "7",
        "eight" => "8",
        "nine" => "9",
        other => other,
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::name_matches;

    #[test]
    fn matches_ui_automation_key_names() {
        assert!(name_matches("1", "1"));
        assert!(name_matches("Key 1", "1"));
        assert!(name_matches("Number one", "1"));
        assert!(name_matches("` ~ 1", "1"));
        assert!(name_matches("A", "a"));
        assert!(name_matches("Control", "Ctrl"));
        assert!(!name_matches("2", "1"));
    }
}
