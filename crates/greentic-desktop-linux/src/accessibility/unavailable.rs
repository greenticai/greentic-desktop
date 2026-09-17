//! Backend used where AT-SPI does not exist (non-Linux builds): every call
//! fails with the same explicit reason, so nothing reports a false success.

use super::model::{AccessibleBackend, AccessibleNodeInfo};
use greentic_desktop_adapter::{AdapterError, AdapterResult};

#[derive(Debug, Clone, Copy, Default)]
pub struct UnavailableBackend;

fn unavailable<T>() -> AdapterResult<T> {
    Err(AdapterError::ExecutionFailed(
        "Linux desktop automation can only run on Linux.".to_owned(),
    ))
}

impl AccessibleBackend for UnavailableBackend {
    type Handle = ();

    fn applications(&self) -> AdapterResult<Vec<()>> {
        unavailable()
    }
    fn children(&self, _node: &()) -> AdapterResult<Vec<()>> {
        unavailable()
    }
    fn describe(&self, _node: &()) -> AdapterResult<AccessibleNodeInfo> {
        unavailable()
    }
    fn process_id(&self, _node: &()) -> Option<u32> {
        None
    }
    fn perform_action(&self, _node: &(), _preferred: &[&str]) -> AdapterResult<String> {
        unavailable()
    }
    fn set_text_contents(&self, _node: &(), _value: &str) -> AdapterResult<()> {
        unavailable()
    }
    fn grab_focus(&self, _node: &()) -> AdapterResult<()> {
        unavailable()
    }
    fn replace_text_by_keyboard(&self, _node: &(), _value: &str) -> AdapterResult<()> {
        unavailable()
    }
    fn select_child(&self, _parent: &(), _index: usize) -> AdapterResult<()> {
        unavailable()
    }
}
