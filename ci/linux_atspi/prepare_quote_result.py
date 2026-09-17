"""Drive Meridian to a completed quotation through libatspi (not the adapter
under test), leaving the result dialog open for
aws_demo_linux_live_result_extraction."""
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402


def frame():
    desktop = Atspi.get_desktop(0)
    for _ in range(60):
        for i in range(desktop.get_child_count()):
            app = desktop.get_child_at_index(i)
            for j in range(app.get_child_count() if app else 0):
                window = app.get_child_at_index(j)
                if window and "Meridian" in (window.get_name() or ""):
                    return window
        time.sleep(0.5)
    raise SystemExit("Meridian window not found")


def nodes(root):
    found, index = [root], 0
    while index < len(found):
        node = found[index]
        index += 1
        for k in range(node.get_child_count()):
            child = node.get_child_at_index(k)
            if child:
                found.append(child)
    return found


def by_id(root, element_id):
    for node in nodes(root):
        node.clear_cache_single()
        if (node.get_attributes() or {}).get("id") == element_id:
            return node
    raise SystemExit(f"#{element_id} not found")


def press(root, element_id):
    Atspi.Action.do_action(by_id(root, element_id), 0)
    time.sleep(0.4)


def popup_cells():
    desktop = Atspi.get_desktop(0)
    for i in range(desktop.get_child_count()):
        app = desktop.get_child_at_index(i)
        for j in range(app.get_child_count()):
            window = app.get_child_at_index(j)
            if window.get_role_name() == "window":
                cells = [n for n in nodes(window) if n.get_role_name() == "table cell"]
                if cells:
                    return cells
    return []


def select(root, element_id, value):
    Atspi.Action.do_action(by_id(root, element_id), 0)
    time.sleep(0.5)
    cell = next(c for c in popup_cells() if c.get_name() == value)
    names = [Atspi.Action.get_action_name(cell, i) for i in range(Atspi.Action.get_n_actions(cell))]
    Atspi.Action.do_action(cell, names.index("activate"))
    time.sleep(0.5)


root = frame()
press(root, "new-quote")
press(root, "load-demo-customer")
for _ in range(4):
    press(root, "next-button")
select(root, "public-liability-limit", "£2,000,000")
for _ in range(3):
    press(root, "next-button")
press(root, "calculate-premium")
time.sleep(3)
print("quotation result prepared")
