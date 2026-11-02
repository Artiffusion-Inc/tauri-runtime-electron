use objc2::{rc::Retained, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
use serde_json::{json, Value};

#[derive(Debug, Default)]
pub(crate) struct MenuMirror {
  snapshot: Value,
  items: Vec<Retained<NSMenuItem>>,
  first_id: u32,
  next_id: u32,
}

impl MenuMirror {
  pub(crate) fn sync(&mut self, ipc: &crate::rpc_runtime::Ipc) -> Result<(), String> {
    objc2::rc::autoreleasepool(|_| {
      let Some(mtm) = MainThreadMarker::new() else {
        return if cfg!(test) {
          Ok(())
        } else {
          Err("macOS menus require the main thread".into())
        };
      };
      let app = NSApplication::sharedApplication(mtm);
      let mut items = Vec::new();
      let value = app
        .mainMenu()
        .map(|menu| snapshot(&menu, &mut items))
        .unwrap_or(Value::Null);
      if value == self.snapshot && items == self.items {
        return Ok(());
      }
      let first = self.next_id;
      let next = first
        .checked_add(u32::try_from(items.len()).map_err(|_| "too many menu items")?)
        .ok_or("menu command identifiers exhausted")?;
      let mut wire = value.clone();
      offset_ids(&mut wire, first);
      let menu =
        serde_json::from_value(wire).map_err(|error| format!("invalid native menu: {error}"))?;
      ipc
        .call(electron_rs::rpc::app::SetMenu { menu })
        .map_err(|error| error.to_string())?;
      self.snapshot = value;
      self.items = items;
      self.first_id = first;
      self.next_id = next;
      Ok(())
    })
  }

  pub(crate) fn command(&mut self, command: u32) -> Option<Retained<NSMenuItem>> {
    let index = command.checked_sub(self.first_id)? as usize;
    let item = self.items.get(index)?.clone();
    self.snapshot = Value::Null;
    Some(item)
  }
}

fn offset_ids(value: &mut Value, offset: u32) {
  if let Some(items) = value.as_array_mut() {
    for item in items {
      if let Some(id) = item["commandId"].as_u64() {
        item["commandId"] = json!(id + u64::from(offset));
      }
      if let Some(submenu) = item.get_mut("submenu") {
        offset_ids(submenu, offset);
      }
    }
  }
}

fn role(action: &str) -> Option<&'static str> {
  Some(match action {
    "undo:" => "undo",
    "redo:" => "redo",
    "cut:" => "cut",
    "copy:" => "copy",
    "paste:" => "paste",
    "selectAll:" => "selectAll",
    "hide:" => "hide",
    "hideOtherApplications:" => "hideOthers",
    "unhideAllApplications:" => "unhide",
    "terminate:" => "quit",
    "performClose:" => "close",
    "performMiniaturize:" => "minimize",
    "performZoom:" => "zoom",
    "arrangeInFront:" => "front",
    "toggleFullScreen:" => "togglefullscreen",
    "showAboutPanel:" | "orderFrontStandardAboutPanel:" => "about",
    _ => return None,
  })
}

fn accelerator(key: &str, flags: usize) -> Option<String> {
  let mut characters = key.chars();
  let code = characters.next()?;
  let single = characters.next().is_none();
  let function_key;
  let key = match key {
    " " => "Space",
    "\r" | "\u{3}" => "Return",
    "\u{8}" | "\u{7f}" => "Backspace",
    "\u{f700}" => "Up",
    "\u{f701}" => "Down",
    "\u{f702}" => "Left",
    "\u{f703}" => "Right",
    "\u{f727}" => "Insert",
    "\u{f728}" => "Delete",
    "\u{f729}" => "Home",
    "\u{f72b}" => "End",
    "\u{f72c}" => "PageUp",
    "\u{f72d}" => "PageDown",
    "\t" => "Tab",
    "\u{1b}" => "Escape",
    "+" => "Plus",
    _ if single && ('\u{f704}'..='\u{f726}').contains(&code) => {
      function_key = format!("F{}", code as u32 - 0xf703);
      &function_key
    }
    key => key,
  };
  let mut parts = Vec::new();
  for (flag, name) in [
    (1 << 20, "Super"),
    (1 << 18, "Ctrl"),
    (1 << 19, "Alt"),
    (1 << 17, "Shift"),
  ] {
    if flags & flag != 0 {
      parts.push(name);
    }
  }
  parts.push(key);
  Some(parts.join("+"))
}

pub(crate) fn snapshot(menu: &NSMenu, commands: &mut Vec<Retained<NSMenuItem>>) -> Value {
  let mut result = Vec::new();
  for item in menu.itemArray() {
    if item.isHidden() {
      continue;
    }
    if item.isSeparatorItem() {
      result.push(json!({"type": "separator"}));
      continue;
    }
    let mut value = json!({"label": item.title().to_string(), "enabled": item.isEnabled()});
    if let Some(accelerator) = accelerator(
      &item.keyEquivalent().to_string(),
      item.keyEquivalentModifierMask().bits(),
    ) {
      value["accelerator"] = json!(accelerator);
    }
    if let Some(submenu) = item.submenu() {
      value["submenu"] = snapshot(&submenu, commands);
      if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApplication::sharedApplication(mtm);
        if app.servicesMenu().as_deref() == Some(&*submenu) {
          value["role"] = json!("services");
        } else if app.windowsMenu().as_deref() == Some(&*submenu) {
          value["role"] = json!("windowMenu");
        } else if app.helpMenu().as_deref() == Some(&*submenu) {
          value["role"] = json!("help");
        }
      }
    } else if let Some(action) = item.action() {
      if let Some(role) = role(action.name().to_str().unwrap_or_default()) {
        value["role"] = json!(role);
      } else {
        value["commandId"] = json!(commands.len());
        commands.push(item.clone());
        if item.state() != 0 {
          value["type"] = json!("checkbox");
          value["checked"] = json!(true);
        }
      }
    } else {
      value["enabled"] = json!(false);
    }
    result.push(value);
  }
  Value::Array(result)
}

pub(crate) fn dispatch(target: &NSMenuItem) {
  objc2::rc::autoreleasepool(|_| {
    let Some(mtm) = MainThreadMarker::new() else {
      return;
    };
    if let Some(menu) = NSApplication::sharedApplication(mtm).mainMenu() {
      dispatch_in_menu(&menu, target);
    }
  });
}

fn dispatch_in_menu(menu: &NSMenu, target: &NSMenuItem) -> bool {
  for (index, item) in menu.itemArray().iter().enumerate() {
    if item.isHidden() || !item.isEnabled() {
      continue;
    }
    if let Some(submenu) = item.submenu() {
      if dispatch_in_menu(&submenu, target) {
        return true;
      }
    } else if &*item == target && item.action().is_some() {
      menu.performActionForItemAtIndex(index as isize);
      return true;
    }
  }
  false
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn native_shortcuts_and_actions_keep_electron_semantics() {
    assert_eq!(accelerator(",", 1 << 20).as_deref(), Some("Super+,"));
    assert_eq!(
      accelerator("n", (1 << 20) | (1 << 17)).as_deref(),
      Some("Super+Shift+n")
    );
    assert_eq!(accelerator("", 0), None);
    assert_eq!(role("terminate:"), Some("quit"));
    assert_eq!(role("customAction:"), None);
    let mut menu = json!([{"commandId": 0}, {"submenu": [{"commandId": 1}]}]);
    offset_ids(&mut menu, 12);
    assert_eq!(
      menu,
      json!([{"commandId": 12}, {"submenu": [{"commandId": 13}]}])
    );
  }
}
