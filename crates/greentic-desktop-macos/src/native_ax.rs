use greentic_desktop_adapter::{AdapterError, AdapterResult, LocatorStrategy, LocatorTarget};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

static ACTIVE_NATIVE_AX_PID: AtomicU32 = AtomicU32::new(0);

pub(crate) struct NativeAxClient {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    calls: usize,
}

impl std::fmt::Debug for NativeAxClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeAxClient")
            .field("pid", &self.child.id())
            .finish()
    }
}

impl NativeAxClient {
    pub(crate) fn start() -> AdapterResult<Self> {
        let script = std::env::temp_dir().join(format!(
            "greentic-native-ax-{}-{}.swift",
            env!("CARGO_PKG_VERSION"),
            std::process::id()
        ));
        std::fs::write(&script, SWIFT_HELPER).map_err(|error| {
            AdapterError::ExecutionFailed(format!("failed to write native AX helper: {error}"))
        })?;
        let mut child = Command::new("swift")
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| {
                AdapterError::ExecutionFailed(format!("failed to start native AX helper: {err}"))
            })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            AdapterError::ExecutionFailed("native AX helper stdin unavailable".to_owned())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AdapterError::ExecutionFailed("native AX helper stdout unavailable".to_owned())
        })?;
        ACTIVE_NATIVE_AX_PID.store(child.id(), Ordering::Release);
        Ok(Self {
            child,
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            calls: 0,
        })
    }

    pub(crate) fn should_recycle(&mut self) -> bool {
        self.calls >= 50 || self.child.try_wait().ok().flatten().is_some()
    }

    pub(crate) fn call(
        &mut self,
        operation: &str,
        app: &str,
        target: &LocatorTarget,
        expected: Option<&str>,
        value: Option<&str>,
    ) -> AdapterResult<String> {
        let fields = [
            operation.to_owned(),
            app.to_owned(),
            strategy_field(target.preferred.as_ref(), |s| s.automation_id.as_deref()),
            strategy_field(target.preferred.as_ref(), |s| s.name.as_deref()),
            strategy_field(target.preferred.as_ref(), |s| s.role.as_deref()),
            strategy_field(target.preferred.as_ref(), |s| s.text.as_deref()),
            strategy_field(target.preferred.as_ref(), |s| s.label.as_deref()),
            strategy_field(target.fallback.as_ref(), |s| s.automation_id.as_deref()),
            strategy_field(target.fallback.as_ref(), |s| s.name.as_deref()),
            strategy_field(target.fallback.as_ref(), |s| s.role.as_deref()),
            strategy_field(target.fallback.as_ref(), |s| s.text.as_deref()),
            strategy_field(target.fallback.as_ref(), |s| s.label.as_deref()),
            expected.unwrap_or_default().to_owned(),
            value.unwrap_or_default().to_owned(),
        ];
        let line = fields
            .iter()
            .map(|field| hex_encode(field))
            .collect::<Vec<_>>()
            .join("\t");
        writeln!(self.stdin, "{line}")
            .and_then(|_| self.stdin.flush())
            .map_err(|err| {
                AdapterError::ExecutionFailed(format!("native AX helper write failed: {err}"))
            })?;
        let mut response = String::new();
        self.stdout.read_line(&mut response).map_err(|err| {
            AdapterError::ExecutionFailed(format!("native AX helper read failed: {err}"))
        })?;
        self.calls += 1;
        if response.is_empty() {
            return Err(AdapterError::ExecutionFailed(
                "native AX helper exited unexpectedly".to_owned(),
            ));
        }
        parse_response(&response)
    }
}

pub(crate) fn cancel_active_native_ax() {
    let pid = ACTIVE_NATIVE_AX_PID.swap(0, Ordering::AcqRel);
    if pid != 0 {
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
}

impl Drop for NativeAxClient {
    fn drop(&mut self) {
        ACTIVE_NATIVE_AX_PID
            .compare_exchange(self.child.id(), 0, Ordering::AcqRel, Ordering::Acquire)
            .ok();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn strategy_field(
    strategy: Option<&LocatorStrategy>,
    field: impl FnOnce(&LocatorStrategy) -> Option<&str>,
) -> String {
    strategy.and_then(field).unwrap_or_default().to_owned()
}

fn hex_encode(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hex_decode(value: &str) -> Option<String> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    let bytes = (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}

fn parse_response(response: &str) -> AdapterResult<String> {
    let response = response.trim_end_matches(['\r', '\n']);
    let (status, payload) = response.split_once('\t').unwrap_or(("ERR", response));
    let payload = hex_decode(payload).unwrap_or_else(|| "invalid helper response".to_owned());
    if status == "OK" {
        Ok(payload)
    } else {
        Err(AdapterError::ExecutionFailed(payload))
    }
}

#[cfg(test)]
mod tests {
    use super::parse_response;

    #[test]
    fn empty_success_payload_keeps_protocol_separator() {
        assert_eq!(parse_response("OK\t\n").expect("success"), "");
    }
}

const SWIFT_HELPER: &str = r#"
import AppKit
import ApplicationServices
import Foundation

func unhex(_ value: Substring) -> String {
  var bytes: [UInt8] = []
  var index = value.startIndex
  while index < value.endIndex {
    let next = value.index(index, offsetBy: 2)
    bytes.append(UInt8(value[index..<next], radix: 16) ?? 0)
    index = next
  }
  return String(bytes: bytes, encoding: .utf8) ?? ""
}

func hex(_ value: String) -> String {
  value.utf8.map { String(format: "%02x", $0) }.joined()
}

func string(_ element: AXUIElement, _ attribute: String) -> String {
  var value: CFTypeRef?
  guard AXUIElementCopyAttributeValue(element, attribute as CFString, &value) == .success else { return "" }
  return value as? String ?? ""
}

func boolean(_ element: AXUIElement, _ attribute: String) -> Bool {
  var value: CFTypeRef?
  guard AXUIElementCopyAttributeValue(element, attribute as CFString, &value) == .success else { return false }
  return value as? Bool ?? false
}

func normalized(_ value: String) -> String {
  value.lowercased().unicodeScalars.filter { CharacterSet.alphanumerics.contains($0) }.map(String.init).joined()
}

func comboInputValue(_ value: String) -> String {
  guard let amount = Int(value) else { return value }
  let formatter = NumberFormatter()
  formatter.numberStyle = .decimal
  formatter.maximumFractionDigits = 0
  return "£" + (formatter.string(from: NSNumber(value: amount)) ?? value)
}

func children(_ element: AXUIElement) -> [AXUIElement] {
  var value: CFTypeRef?
  guard AXUIElementCopyAttributeValue(element, kAXChildrenAttribute as CFString, &value) == .success else { return [] }
  return value as? [AXUIElement] ?? []
}

func element(_ source: AXUIElement, _ attribute: String) -> AXUIElement? {
  var value: CFTypeRef?
  guard AXUIElementCopyAttributeValue(source, attribute as CFString, &value) == .success else { return nil }
  return (value as! AXUIElement)
}

func firstText(_ root: AXUIElement) -> String {
  var queue = [root]
  var index = 0
  while index < queue.count {
    let candidate = queue[index]
    index += 1
    let value = string(candidate, kAXValueAttribute)
    if !value.isEmpty { return value }
    let title = string(candidate, kAXTitleAttribute)
    if !title.isEmpty { return title }
    queue.append(contentsOf: children(candidate))
  }
  return ""
}

func adjacentValue(_ label: AXUIElement) -> String {
  guard let parent = element(label, kAXParentAttribute) else { return "" }
  let siblings = children(parent)
  guard let index = siblings.firstIndex(where: { CFEqual($0, label) }) else { return "" }
  for sibling in siblings.dropFirst(index + 1) {
    let value = firstText(sibling)
    if !value.isEmpty { return value }
  }
  return ""
}

func editableDescendant(_ root: AXUIElement) -> AXUIElement? {
  var queue = [root]
  var index = 0
  while index < queue.count {
    let candidate = queue[index]
    index += 1
    if ["AXTextField", "AXTextArea", "AXComboBox", "AXPopUpButton", "AXIncrementor"].contains(string(candidate, kAXRoleAttribute)) {
      return candidate
    }
    queue.append(contentsOf: children(candidate))
  }
  return nil
}

func editableTarget(_ match: AXUIElement) -> AXUIElement {
  if let editable = editableDescendant(match) { return editable }
  guard let parent = element(match, kAXParentAttribute) else { return match }
  let siblings = children(parent)
  if let index = siblings.firstIndex(where: { CFEqual($0, match) }) {
    for sibling in siblings.dropFirst(index + 1) {
      if let editable = editableDescendant(sibling) { return editable }
    }
  }
  return match
}

func paste(_ value: String, into element: AXUIElement, app: NSRunningApplication) -> Bool {
  app.activate(options: [])
  let activationDeadline = Date().addingTimeInterval(0.5)
  while !app.isActive && Date() < activationDeadline {
    usleep(10_000)
  }
  guard app.isActive else { return false }
  guard AXUIElementSetAttributeValue(element, kAXFocusedAttribute as CFString, kCFBooleanTrue) == .success else { return false }
  guard boolean(element, kAXFocusedAttribute) else { return false }
  NSPasteboard.general.clearContents()
  NSPasteboard.general.setString(value, forType: .string)
  guard let source = CGEventSource(stateID: .hidSystemState) else { return false }
  for (key, down, flags) in [(CGKeyCode(0), true, CGEventFlags.maskCommand), (CGKeyCode(0), false, CGEventFlags.maskCommand), (CGKeyCode(9), true, CGEventFlags.maskCommand), (CGKeyCode(9), false, CGEventFlags.maskCommand)] {
    guard app.isActive && boolean(element, kAXFocusedAttribute) else { return false }
    guard let event = CGEvent(keyboardEventSource: source, virtualKey: key, keyDown: down) else { return false }
    event.flags = flags
    event.post(tap: .cghidEventTap)
    usleep(5_000)
  }
  let pasteDeadline = Date().addingTimeInterval(0.3)
  while Date() < pasteDeadline {
    if string(element, kAXValueAttribute) == value { return true }
    usleep(10_000)
  }
  return false
}

func confirmComboBox(_ element: AXUIElement, value: String, app: NSRunningApplication) -> Bool {
  guard app.isActive && boolean(element, kAXFocusedAttribute) else { return false }
  guard let source = CGEventSource(stateID: .hidSystemState) else { return false }
  for down in [true, false] {
    guard let event = CGEvent(keyboardEventSource: source, virtualKey: CGKeyCode(36), keyDown: down) else { return false }
    event.post(tap: .cghidEventTap)
    usleep(5_000)
  }
  let deadline = Date().addingTimeInterval(0.3)
  while Date() < deadline {
    if normalized(string(element, kAXValueAttribute)) == normalized(value) { return true }
    usleep(10_000)
  }
  return false
}

func selectPopUp(_ target: AXUIElement, value: String, root: AXUIElement) -> Bool {
  guard AXUIElementPerformAction(target, kAXPressAction as CFString) == .success else { return false }
  usleep(50_000)
  var queue = [root]
  var index = 0
  while index < queue.count {
    let candidate = queue[index]
    index += 1
    let role = string(candidate, kAXRoleAttribute)
    let texts = [string(candidate, kAXTitleAttribute), string(candidate, kAXValueAttribute), string(candidate, kAXDescriptionAttribute)]
    if ["AXMenuItem", "AXRadioButton", "AXStaticText"].contains(role)
      && texts.contains(where: { normalized($0) == normalized(value) })
      && AXUIElementPerformAction(candidate, kAXPressAction as CFString) == .success {
      let deadline = Date().addingTimeInterval(0.3)
      while Date() < deadline {
        if normalized(string(target, kAXValueAttribute)) == normalized(value) { return true }
        usleep(10_000)
      }
      return false
    }
    queue.append(contentsOf: children(candidate))
  }
  return false
}

struct Strategy {
  let identifier: String
  let name: String
  let role: String
  let text: String
  let label: String
  var empty: Bool { identifier.isEmpty && name.isEmpty && role.isEmpty && text.isEmpty && label.isEmpty }
}

func roles(_ role: String) -> Set<String> {
  switch role.lowercased() {
  case "button": return ["AXButton"]
  case "textbox", "text field": return ["AXTextField", "AXTextArea"]
  case "combobox", "combo box": return ["AXComboBox", "AXPopUpButton"]
  case "spinbutton", "spin button": return ["AXIncrementor", "AXTextField"]
  case "heading": return ["AXHeading", "AXStaticText"]
  case "static text", "statictext": return ["AXStaticText"]
  default: return role.isEmpty ? [] : [role]
  }
}

func matches(_ element: AXUIElement, _ strategy: Strategy, _ expected: String) -> Bool {
  if strategy.empty && expected.isEmpty { return false }
  let acceptedRoles = roles(strategy.role)
  if !acceptedRoles.isEmpty && !acceptedRoles.contains(string(element, kAXRoleAttribute)) { return false }
  let title = string(element, kAXTitleAttribute)
  let value = string(element, kAXValueAttribute)
  let description = string(element, kAXDescriptionAttribute)
  let identifier = string(element, kAXIdentifierAttribute)
  let combined = title + "\n" + value + "\n" + description
  let semantic = !strategy.name.isEmpty || !strategy.label.isEmpty || !strategy.text.isEmpty
  if !strategy.identifier.isEmpty && !semantic && identifier != strategy.identifier && description != strategy.identifier { return false }
  if !strategy.name.isEmpty {
    let nameMatches = strategy.name.count <= 2
      ? [title, value, description].contains(where: { $0.caseInsensitiveCompare(strategy.name) == .orderedSame })
      : combined.localizedCaseInsensitiveContains(strategy.name)
    if !nameMatches { return false }
  }
  if !strategy.label.isEmpty && !combined.localizedCaseInsensitiveContains(strategy.label) { return false }
  if !strategy.text.isEmpty && !combined.localizedCaseInsensitiveContains(strategy.text) { return false }
  if !expected.isEmpty && !combined.localizedCaseInsensitiveContains(expected) { return false }
  return true
}

func webContentRoot(_ root: AXUIElement) -> AXUIElement {
  var candidate = root
  for _ in 0..<6 {
    if string(candidate, kAXRoleAttribute) == "AXWebArea" { return candidate }
    guard let first = children(candidate).first else { break }
    candidate = first
  }
  var queue: [(AXUIElement, Int)] = [(root, 0)]
  var index = 0
  while index < queue.count {
    let (candidate, depth) = queue[index]
    index += 1
    if string(candidate, kAXRoleAttribute) == "AXWebArea" { return candidate }
    if depth < 6 { queue.append(contentsOf: children(candidate).map { ($0, depth + 1) }) }
  }
  return root
}

var cachedElements: [AXUIElement] = []

func refreshElements(_ root: AXUIElement) -> [AXUIElement] {
  var elements: [AXUIElement] = []
  var queue: [(AXUIElement, Int)] = [(webContentRoot(root), 0)]
  var index = 0
  while index < queue.count {
    let (element, depth) = queue[index]
    index += 1
    elements.append(element)
    if depth < 10 { queue.append(contentsOf: children(element).map { ($0, depth + 1) }) }
  }
  return elements
}

func findUsing(_ elements: [AXUIElement], _ strategy: Strategy, _ expected: String) -> AXUIElement? {
  if strategy.empty { return nil }
  for element in elements {
    if matches(element, strategy, expected) { return element }
  }
  return nil
}

func find(_ root: AXUIElement, _ preferred: Strategy, _ fallback: Strategy, _ expected: String) -> AXUIElement? {
  if let match = findUsing(cachedElements, preferred, expected) { return match }
  cachedElements = refreshElements(root)
  if let match = findUsing(cachedElements, preferred, expected) { return match }
  return findUsing(cachedElements, fallback, expected)
}

func application(_ name: String) -> NSRunningApplication? {
  NSWorkspace.shared.runningApplications.first {
    ($0.localizedName ?? "").caseInsensitiveCompare(name.replacingOccurrences(of: ".app", with: "")) == .orderedSame
      || ($0.executableURL?.lastPathComponent ?? "").caseInsensitiveCompare(name) == .orderedSame
  }
}

while let line = readLine() {
  let values = line.split(separator: "\t", omittingEmptySubsequences: false).map(unhex)
  guard values.count == 14 else { print("ERR\t" + hex("invalid native AX request")); fflush(stdout); continue }
  let preferred = Strategy(identifier: values[2], name: values[3], role: values[4], text: values[5], label: values[6])
  let fallback = Strategy(identifier: values[7], name: values[8], role: values[9], text: values[10], label: values[11])
  guard let app = application(values[1]) else { print("ERR\t" + hex("application not running: " + values[1])); fflush(stdout); continue }
  let root = AXUIElementCreateApplication(app.processIdentifier)
  guard let element = find(root, preferred, fallback, values[12]) else { print("ERR\t" + hex("No matching native AX element was visible.")); fflush(stdout); continue }
  var result = ""
  switch values[0] {
  case "find": result = "true"
  case "click":
    let error = AXUIElementPerformAction(element, kAXPressAction as CFString)
    if error != .success { print("ERR\t" + hex("native AXPress failed: \(error.rawValue)")); fflush(stdout); continue }
  case "type":
    let target = editableTarget(element)
    let targetRole = string(target, kAXRoleAttribute)
    if targetRole == "AXPopUpButton" && selectPopUp(target, value: values[13], root: root) {
      result = string(target, kAXValueAttribute)
      break
    }
    if targetRole == "AXComboBox" {
      let inputValue = comboInputValue(values[13])
      if paste(inputValue, into: target, app: app) && confirmComboBox(target, value: values[13], app: app) {
        result = string(target, kAXValueAttribute)
        break
      }
    } else if paste(values[13], into: target, app: app) {
      result = string(target, kAXValueAttribute)
      break
    }
    result = string(target, kAXValueAttribute)
    if targetRole == "AXComboBox" && (normalized(result) == normalized(values[13]) || confirmComboBox(target, value: values[13], app: app)) {
      result = string(target, kAXValueAttribute)
      break
    }
    print("ERR\t" + hex("native paste verification failed: expected " + values[13] + ", observed " + result)); fflush(stdout); continue
  case "read":
    result = string(element, kAXValueAttribute)
    if result.isEmpty { result = string(element, kAXTitleAttribute) }
    if result.trimmingCharacters(in: .whitespacesAndNewlines).hasSuffix(":") {
      let adjacent = adjacentValue(element)
      if !adjacent.isEmpty { result = adjacent }
    }
  default: print("ERR\t" + hex("unsupported native AX operation")); fflush(stdout); continue
  }
  print("OK\t" + hex(result))
  fflush(stdout)
}
"#;
