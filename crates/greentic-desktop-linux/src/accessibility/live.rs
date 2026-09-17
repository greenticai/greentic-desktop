//! [`AccessibleBackend`] over the real AT-SPI bus, using the `atspi` proxies
//! on a `zbus` connection. Every call is blocking from the caller's point of
//! view; the futures are driven by zbus' own executor.
//!
//! Proxies are built with property caching disabled: a cached proxy subscribes
//! to `PropertiesChanged` for every node it touches, which on a web page means
//! hundreds of match rules per tree walk.

use super::model::{AccessibleBackend, AccessibleNodeInfo, NodeInterfaces, NodeStates};
use atspi::proxy::accessible::AccessibleProxy;
use atspi::proxy::action::ActionProxy;
use atspi::proxy::component::ComponentProxy;
use atspi::proxy::device_event_controller::{DeviceEventControllerProxy, KeySynthType};
use atspi::proxy::editable_text::EditableTextProxy;
use atspi::proxy::selection::SelectionProxy;
use atspi::proxy::text::TextProxy;
use atspi::{AccessibilityConnection, Interface, ObjectRefOwned, State};
use greentic_desktop_adapter::{AdapterError, AdapterResult};
use std::time::Duration;
use zbus::names::BusName;
use zbus::proxy::CacheProperties;

const REGISTRY_BUS_NAME: &str = "org.a11y.atspi.Registry";
const ROOT_PATH: &str = "/org/a11y/atspi/accessible/root";

/// A live connection to the accessibility bus.
pub struct AtspiBackend {
    connection: zbus::Connection,
}

impl std::fmt::Debug for AtspiBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AtspiBackend")
            .field("unique_name", &self.connection.unique_name())
            .finish()
    }
}

/// Handle to one accessible object: its bus name and object path.
pub type AtspiHandle = ObjectRefOwned;

fn failed(context: &str, error: impl std::fmt::Display) -> AdapterError {
    AdapterError::ExecutionFailed(format!("AT-SPI {context} failed: {error}"))
}

macro_rules! object_proxy {
    ($proxy:ident, $connection:expr, $object:expr) => {{
        let object: &ObjectRefOwned = $object;
        let name = object
            .name()
            .ok_or_else(|| {
                AdapterError::ExecutionFailed("AT-SPI object reference is null".to_owned())
            })?
            .clone();
        $proxy::builder($connection)
            .destination(name)
            .and_then(|builder| builder.path(object.path().clone()))
            .map(|builder| builder.cache_properties(CacheProperties::No))
            .map_err(|error| failed("proxy setup", error))?
            .build()
            .await
            .map_err(|error| failed("proxy setup", error))?
    }};
}

impl AtspiBackend {
    /// Connect to the accessibility bus. `AT_SPI_BUS_ADDRESS` wins when set;
    /// otherwise the address is read from `org.a11y.Bus` on the session bus.
    pub fn connect() -> AdapterResult<Self> {
        let connection = zbus::block_on(async {
            let connection = match std::env::var("AT_SPI_BUS_ADDRESS")
                .ok()
                .filter(|address| !address.trim().is_empty())
            {
                Some(address) => {
                    let address = address
                        .parse()
                        .map_err(|error| failed("bus address parse", error))?;
                    AccessibilityConnection::from_address(address).await
                }
                None => AccessibilityConnection::new().await,
            };
            connection
                .map(|connection| connection.connection().clone())
                .map_err(|error| {
                    AdapterError::ExecutionFailed(format!(
                        "AT-SPI accessibility bus is unavailable: {error}. Start at-spi-bus-launcher (at-spi2-core) in the desktop session."
                    ))
                })
        })?;
        Ok(Self { connection })
    }

    /// True when an accessibility bus can be reached, bounded by `timeout`.
    pub fn probe(timeout: Duration) -> bool {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(Self::connect().is_ok());
        });
        receiver.recv_timeout(timeout).unwrap_or(false)
    }
}

impl AccessibleBackend for AtspiBackend {
    type Handle = AtspiHandle;

    fn applications(&self) -> AdapterResult<Vec<AtspiHandle>> {
        zbus::block_on(async {
            let root = AccessibleProxy::builder(&self.connection)
                .destination(REGISTRY_BUS_NAME)
                .and_then(|builder| builder.path(ROOT_PATH))
                .map(|builder| builder.cache_properties(CacheProperties::No))
                .map_err(|error| failed("registry proxy", error))?
                .build()
                .await
                .map_err(|error| failed("registry proxy", error))?;
            root.get_children()
                .await
                .map_err(|error| failed("registry GetChildren", error))
        })
        .map(|children| {
            children
                .into_iter()
                .filter(|child| !child.is_null())
                .collect()
        })
    }

    fn children(&self, node: &AtspiHandle) -> AdapterResult<Vec<AtspiHandle>> {
        zbus::block_on(async {
            let proxy = object_proxy!(AccessibleProxy, &self.connection, node);
            proxy
                .get_children()
                .await
                .map_err(|error| failed("GetChildren", error))
        })
        .map(|children| {
            children
                .into_iter()
                .filter(|child| !child.is_null())
                .collect()
        })
    }

    fn describe(&self, node: &AtspiHandle) -> AdapterResult<AccessibleNodeInfo> {
        zbus::block_on(async {
            let proxy = object_proxy!(AccessibleProxy, &self.connection, node);
            let role = proxy
                .get_role_name()
                .await
                .map_err(|error| failed("GetRoleName", error))?;
            let name = proxy.name().await.unwrap_or_default();
            let description = proxy.description().await.unwrap_or_default();
            let attributes = proxy
                .get_attributes()
                .await
                .map(|attributes| attributes.into_iter().collect())
                .unwrap_or_default();
            let interfaces = proxy.get_interfaces().await.unwrap_or_default();
            let states = proxy.get_state().await.unwrap_or_default();
            let text = if interfaces.contains(Interface::Text) {
                let text_proxy = object_proxy!(TextProxy, &self.connection, node);
                text_proxy.get_text(0, -1).await.ok()
            } else {
                None
            };
            Ok(AccessibleNodeInfo {
                role,
                name,
                description,
                text,
                attributes,
                states: NodeStates {
                    showing: states.contains(State::Showing),
                    visible: states.contains(State::Visible),
                    enabled: states.contains(State::Enabled),
                    focused: states.contains(State::Focused),
                    editable: states.contains(State::Editable),
                    selected: states.contains(State::Selected),
                    checked: states.contains(State::Checked),
                },
                interfaces: NodeInterfaces {
                    action: interfaces.contains(Interface::Action),
                    text: interfaces.contains(Interface::Text),
                    editable_text: interfaces.contains(Interface::EditableText),
                    selection: interfaces.contains(Interface::Selection),
                    component: interfaces.contains(Interface::Component),
                },
            })
        })
    }

    fn process_id(&self, node: &AtspiHandle) -> Option<u32> {
        let name = node.name()?.clone();
        zbus::block_on(async {
            let dbus = zbus::fdo::DBusProxy::new(&self.connection).await.ok()?;
            dbus.get_connection_unix_process_id(BusName::Unique(name))
                .await
                .ok()
        })
    }

    fn perform_action(&self, node: &AtspiHandle, preferred: &[&str]) -> AdapterResult<String> {
        zbus::block_on(async {
            let proxy = object_proxy!(ActionProxy, &self.connection, node);
            let count = proxy
                .n_actions()
                .await
                .map_err(|error| failed("Action.NActions", error))?;
            if count <= 0 {
                return Err(AdapterError::ExecutionFailed(
                    "AT-SPI element exposes no actions to invoke.".to_owned(),
                ));
            }
            let mut names = Vec::new();
            for index in 0..count {
                names.push(proxy.get_name(index).await.unwrap_or_default());
            }
            let index = preferred
                .iter()
                .find_map(|wanted| {
                    names
                        .iter()
                        .position(|name| name.eq_ignore_ascii_case(wanted))
                })
                .unwrap_or(0);
            let done = proxy
                .do_action(index as i32)
                .await
                .map_err(|error| failed("Action.DoAction", error))?;
            if done {
                Ok(names[index].clone())
            } else {
                Err(AdapterError::ExecutionFailed(format!(
                    "AT-SPI action {:?} was refused by the application.",
                    names[index]
                )))
            }
        })
    }

    fn set_text_contents(&self, node: &AtspiHandle, value: &str) -> AdapterResult<()> {
        zbus::block_on(async {
            let proxy = object_proxy!(EditableTextProxy, &self.connection, node);
            match proxy.set_text_contents(value).await {
                Ok(true) => Ok(()),
                Ok(false) => Err(AdapterError::ExecutionFailed(
                    "AT-SPI EditableText.SetTextContents was refused.".to_owned(),
                )),
                Err(error) => Err(failed("EditableText.SetTextContents", error)),
            }
        })
    }

    fn grab_focus(&self, node: &AtspiHandle) -> AdapterResult<()> {
        zbus::block_on(async {
            let proxy = object_proxy!(ComponentProxy, &self.connection, node);
            proxy
                .grab_focus()
                .await
                .map_err(|error| failed("Component.GrabFocus", error))
                .and_then(|focused| {
                    focused.then_some(()).ok_or_else(|| {
                        AdapterError::ExecutionFailed(
                            "AT-SPI Component.GrabFocus was refused.".to_owned(),
                        )
                    })
                })
        })
    }

    fn replace_text_by_keyboard(&self, node: &AtspiHandle, value: &str) -> AdapterResult<()> {
        self.grab_focus(node)?;
        zbus::block_on(async {
            let text = object_proxy!(TextProxy, &self.connection, node);
            let count = text
                .character_count()
                .await
                .map_err(|error| failed("Text.CharacterCount", error))?;
            if count > 0 && !text.set_selection(0, 0, count).await.unwrap_or(false) {
                let added = text.add_selection(0, count).await.unwrap_or(false);
                if !added {
                    return Err(AdapterError::ExecutionFailed(
                        "AT-SPI could not select the existing text before typing, so typing would append instead of replace.".to_owned(),
                    ));
                }
            }
            let controller = DeviceEventControllerProxy::builder(&self.connection)
                .cache_properties(CacheProperties::No)
                .build()
                .await
                .map_err(|error| failed("DeviceEventController proxy", error))?;
            if value.is_empty() {
                // BackSpace deletes the selection.
                controller
                    .generate_keyboard_event(0xff08, "", KeySynthType::Sym)
                    .await
                    .map_err(|error| failed("GenerateKeyboardEvent", error))
            } else {
                controller
                    .generate_keyboard_event(0, value, KeySynthType::String)
                    .await
                    .map_err(|error| failed("GenerateKeyboardEvent", error))
            }
        })
    }

    fn select_child(&self, parent: &AtspiHandle, index: usize) -> AdapterResult<()> {
        zbus::block_on(async {
            let proxy = object_proxy!(SelectionProxy, &self.connection, parent);
            let index = i32::try_from(index)
                .map_err(|_| AdapterError::ExecutionFailed("child index overflow".to_owned()))?;
            match proxy.select_child(index).await {
                Ok(true) => Ok(()),
                Ok(false) => Err(AdapterError::ExecutionFailed(
                    "AT-SPI Selection.SelectChild was refused.".to_owned(),
                )),
                Err(error) => Err(failed("Selection.SelectChild", error)),
            }
        })
    }
}
