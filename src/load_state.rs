#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebviewLoadState { Started, Finished, Failed, ProcessGone }

pub(crate) type LoadHandler = dyn Fn(WebviewLoadState) + Send;

impl WebviewLoadState {
  pub(crate) fn from_event(event: &str) -> Option<Self> {
    match event {
      "webview.loadStarted" => Some(Self::Started),
      "webview.loadFinished" => Some(Self::Finished),
      "webview.loadFailed" => Some(Self::Failed),
      "webview.processGone" => Some(Self::ProcessGone),
      _ => None,
    }
  }
}

#[cfg(feature = "tauri")]
pub fn on_webview_load_state<F>(
  webview: &tauri::Webview<crate::ElectronRuntime>, handler: F,
) -> tauri::Result<()>
where F: Fn(WebviewLoadState) + Send + 'static {
  crate::browser::route(webview, move |view| {
    let id = view.id;
    view.context.on_main_thread(move |context| {
      let handler: std::rc::Rc<LoadHandler> = std::rc::Rc::new(handler);
      context.load_state_handlers.borrow_mut().insert(id, handler.clone());
      let state = context.load_states.borrow().get(&id).copied();
      if let Some(state) = state { handler(state); }
    })
  })
}

#[test]
fn load_events_preserve_all_terminal_states() {
  assert_eq!(WebviewLoadState::from_event("webview.loadStarted"), Some(WebviewLoadState::Started));
  assert_eq!(WebviewLoadState::from_event("webview.loadFinished"), Some(WebviewLoadState::Finished));
  assert_eq!(WebviewLoadState::from_event("webview.loadFailed"), Some(WebviewLoadState::Failed));
  assert_eq!(WebviewLoadState::from_event("webview.processGone"), Some(WebviewLoadState::ProcessGone));
  assert_eq!(WebviewLoadState::from_event("webview.titleChanged"), None);
}
