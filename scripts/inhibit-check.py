import ctypes, json, os, subprocess, sys, time, urllib.request
from ctypes import wintypes
from pathlib import Path
import websocket

repository = Path(__file__).resolve().parent.parent
output = repository / "inhibit-check"
output.mkdir(exist_ok=True)
user32 = ctypes.WinDLL("user32", use_last_error=True)

PAGE = """<!doctype html><html><body style="margin:0;background:#336699">
<textarea id="t" style="width:600px;height:300px">hello</textarea>
<script>
window.keys = [];
for (const type of ["keydown", "keyup"]) addEventListener(type, e => keys.push([type, e.key, e.code, e.keyCode, e.metaKey, e.altKey, e.ctrlKey, e.shiftKey, e.repeat]), true);
document.getElementById("t").focus();
</script></body></html>"""

def run(*command):
    print("$", " ".join(map(str, command)), flush=True)
    return subprocess.run(command, cwd=repository, check=True, capture_output=True, text=True)

run("cargo", "build", "-p", "sabine-cli")
run("cargo", "build", "--release", "-p", "sabine-notes", "-p", "sabine-service")
host = run(repository / "target/debug/sabine.exe", "runtime", "prepare").stdout.strip().splitlines()[-1]
root = output / "app"
(root / "ui").mkdir(parents=True, exist_ok=True)
(root / "ui/index.html").write_text(PAGE)
(root / "Sabine.toml").write_text('[app]\nid = "dev.sabine.inhibit-check"\nname = "Inhibit Check"\nversion = "0.1.0"\n\n[web]\nroot = "ui"\nentry = "ui/index.html"\n')
log_path = output / "app.log"
log = log_path.open("w")
app = subprocess.Popen([repository / "target/release/sabine-notes.exe", "--system"], cwd=root,
    env=dict(os.environ, SABINE_TRACE="1", SABINE_HOST_PATH=host, SABINE_ENV="development"),
    stdout=log, stderr=subprocess.STDOUT, creationflags=subprocess.CREATE_NEW_PROCESS_GROUP)

def evaluate(expression):
    targets = json.load(urllib.request.urlopen("http://127.0.0.1:9222/json"))
    page = next(t for t in targets if t["type"] == "page")
    ws = websocket.create_connection(page["webSocketDebuggerUrl"], suppress_origin=True)
    ws.send(json.dumps({"id": 1, "method": "Runtime.evaluate", "params": {"expression": expression, "awaitPromise": True, "returnByValue": True}}))
    while True:
        reply = json.loads(ws.recv())
        if reply.get("id") == 1:
            ws.close()
            return reply["result"].get("result", {}).get("value")

def find_window():
    found = []
    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def visit(hwnd, _):
        buffer = ctypes.create_unicode_buffer(256)
        user32.GetWindowTextW(hwnd, buffer, 256)
        if buffer.value == "Inhibit Check" and user32.IsWindowVisible(hwnd):
            found.append(hwnd)
        return True
    user32.EnumWindows(visit, 0)
    return found[0] if found else None

KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP = 0x1, 0x2
VK = {"LWin": 0x5B, "Alt": 0xA4, "Ctrl": 0xA2, "Tab": 0x09, "Esc": 0x1B, "R": 0x52, "A": 0x41, "X": 0x58}
EXTENDED = {"LWin"}

trace = []

def describe(hwnd):
    title = ctypes.create_unicode_buffer(256)
    cls = ctypes.create_unicode_buffer(256)
    user32.GetWindowTextW(hwnd, title, 256)
    user32.GetClassNameW(hwnd, cls, 256)
    pid = wintypes.DWORD()
    user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    return f"{hwnd} {title.value!r} {cls.value!r} pid={pid.value}"

def key(name, up=False):
    vk = VK[name]
    scan = user32.MapVirtualKeyW(vk, 0)
    flags = (KEYEVENTF_EXTENDEDKEY if name in EXTENDED else 0) | (KEYEVENTF_KEYUP if up else 0)
    user32.keybd_event(vk, scan, flags, 0)
    time.sleep(0.08)
    trace.append(f"{name} {'up' if up else 'down'} -> {describe(user32.GetForegroundWindow())}")

def combo(text):
    names = text.split("+")
    for name in names: key(name)
    for name in reversed(names): key(name, up=True)
    time.sleep(0.4)

report = {}
try:
    deadline = time.monotonic() + 240
    while "browser.first_paint" not in log_path.read_text(errors="replace"):
        if app.poll() is not None or time.monotonic() > deadline:
            raise SystemExit("no first paint\n" + log_path.read_text(errors="replace"))
        time.sleep(0.5)
    time.sleep(2)
    hwnd = find_window()
    report["hwnd"] = hwnd
    key("Alt"); key("Alt", up=True)
    user32.SetForegroundWindow(hwnd)
    time.sleep(0.5)
    report["foreground_before"] = user32.GetForegroundWindow() == hwnd
    report["app_pid"] = app.pid
    report["window"] = describe(hwnd)
    report["enable"] = evaluate('window.sabine.window.inhibitShortcuts(true).then(() => "ok", e => "error: " + e.message)')
    time.sleep(0.5)
    report["foreground_after_enable"] = describe(user32.GetForegroundWindow())
    evaluate("keys.length = 0")
    for text in ["LWin", "Alt+Tab", "Ctrl+Esc", "LWin+R", "Ctrl+A", "X"]:
        user32.SetForegroundWindow(hwnd)
        time.sleep(0.3)
        trace.append(f"== {text} from {describe(user32.GetForegroundWindow())}")
        combo(text)
        trace.append(f"page keys {evaluate('JSON.stringify(keys.splice(0))')}")
    report["trace"] = trace
    report["keys"] = evaluate("keys.splice(0)")
    report["text"] = evaluate('document.getElementById("t").value')
    report["disable"] = evaluate('window.sabine.window.inhibitShortcuts(false).then(() => "ok", e => "error: " + e.message)')
    combo("Alt+Tab")
    report["keys_after_disable"] = evaluate("keys.splice(0)")
finally:
    subprocess.run(["taskkill", "/PID", str(app.pid), "/T", "/F"], capture_output=True)
    (output / "report.json").write_text(json.dumps(report, indent=1))
    print("\n".join(line for line in log_path.read_text(errors="replace").splitlines() if "SABINE_HOOK" in line or "SABINE_DIAG" in line or "SABINE_SLOW" in line or "trace" in line[:40]))
    print(json.dumps(report, indent=1))
