#!/usr/bin/env python3
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import sys
import time

MACOS = sys.platform == "darwin"
WINDOWS = sys.platform == "win32"
if not (MACOS or WINDOWS):
    raise SystemExit("This check requires macOS or Windows")

repository = Path(__file__).resolve().parent.parent
output = Path(os.environ.get("SABINE_PAINT_CHECK_OUTPUT", repository / "paint-check"))
executable_suffix = ".exe" if WINDOWS else ""
paint_path = "accelerated IOSurface" if MACOS else "accelerated D3D12"
page_color = (0x33, 0x66, 0x99)
square_color = (0xFF, 0x99, 0x00)
tolerance = 40
failure_messages = (
    "accelerated texture import failed",
    "Could not share browser surfaces",
    "shared paint copy failed",
    "Metal is unavailable",
    "D3D11 device creation failed",
    "OpenSharedResource1 failed",
    "failed to create owned D3D12 shared texture",
)

PAGE = """<!doctype html>
<html>
<body style="margin:0;overflow:hidden;background:#336699">
<div id="square" style="position:fixed;top:420px;left:0;width:120px;height:120px;background:#ff9900"></div>
<script>
const square = document.getElementById("square");
const start = performance.now();
function frame(now) {
  const travel = window.innerWidth - 120;
  square.style.transform = `translateX(${((now - start) / 4) % travel}px)`;
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
</script>
</body>
</html>
"""

WINDOWS_PROCESSES = (
    "Get-CimInstance Win32_Process | ForEach-Object { "
    "\"$($_.ProcessId)`t$($_.ParentProcessId)`t"
    "$(([double]$_.KernelModeTime + [double]$_.UserModeTime) / 1e7)`t$($_.CommandLine)\" }"
)

WINDOWS_SCREENSHOT = """
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bitmap = [System.Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
$bitmap.Save($args[0], [System.Drawing.Imaging.ImageFormat]::Bmp)
"""


def run(*command, **options):
    print("$", " ".join(str(part) for part in command), flush=True)
    return subprocess.run(command, cwd=repository, check=True, **options)


def powershell(script, *arguments):
    return subprocess.run(
        ["powershell", "-NoProfile", "-Command", script, *arguments],
        capture_output=True, text=True, check=True,
    ).stdout


def prepare_host():
    run("cargo", "build", "-p", "sabine-cli")
    run("cargo", "build", "--release", "-p", "sabine-notes")
    prepared = run(
        repository / f"target/debug/sabine{executable_suffix}", "runtime", "prepare",
        capture_output=True, text=True,
    )
    (output / "host-build.log").write_text(prepared.stdout + prepared.stderr)
    host = prepared.stdout.strip().splitlines()[-1]
    if not Path(host).is_file():
        raise SystemExit(f"The native host was not built: {host}")
    return host


def write_app(root):
    (root / "ui").mkdir(parents=True)
    (root / "ui/index.html").write_text(PAGE)
    (root / "Sabine.toml").write_text(
        '[app]\nid = "dev.sabine.paint-check"\nname = "Sabine Paint Check"\n'
        'version = "0.1.0"\n\n[web]\nroot = "ui"\nentry = "ui/index.html"\n'
    )


def cpu_seconds(value):
    seconds = 0.0
    for part in value.split(":"):
        seconds = seconds * 60 + float(part)
    return seconds


def processes():
    if WINDOWS:
        rows = []
        for line in powershell(WINDOWS_PROCESSES).splitlines():
            pid, ppid, cpu, command = (line.split("\t", 3) + [""])[:4]
            rows.append((int(pid), int(ppid), float(cpu), command))
        return rows
    listing = subprocess.run(
        ["ps", "-A", "-o", "pid=,ppid=,time=,command="],
        capture_output=True, text=True, check=True,
    ).stdout
    rows = []
    for line in listing.splitlines():
        pid, ppid, cpu, command = line.split(None, 3)
        rows.append((int(pid), int(ppid), cpu_seconds(cpu), command))
    return rows


def app_processes(app_pid):
    rows = processes()
    window = next(
        (pid for pid, ppid, _, command in rows
         if ppid == app_pid and "--sabine-osr-host" in command),
        None,
    )
    browser = next(
        (pid for pid, ppid, _, command in rows
         if ppid == window and "--sabine-osr" in command and "--type=" not in command),
        None,
    )
    return {"window host": window, "Chromium browser": browser}


def cpu_usage(pids, seconds):
    def sample():
        rows = {pid: cpu for pid, _, cpu, _ in processes()}
        return {name: rows[pid] for name, pid in pids.items() if pid in rows}

    before = sample()
    time.sleep(seconds)
    after = sample()
    return {name: (after[name] - before[name]) / seconds * 100 for name in before if name in after}


def screenshot(name):
    bmp = output / f"{name}.bmp"
    if WINDOWS:
        powershell(WINDOWS_SCREENSHOT, str(bmp))
    else:
        png = output / f"{name}.png"
        subprocess.run(["screencapture", "-x", str(png)], check=True)
        subprocess.run(
            ["sips", "-s", "format", "bmp", str(png), "--out", str(bmp)],
            check=True, capture_output=True,
        )
    return read_bmp(bmp)


def read_bmp(path):
    data = path.read_bytes()
    offset, = struct.unpack_from("<I", data, 10)
    width, height = struct.unpack_from("<ii", data, 18)
    bits, = struct.unpack_from("<H", data, 28)
    channels = bits // 8
    stride = (width * channels + 3) & ~3
    rows = abs(height)
    pixels = []
    for row in range(rows):
        source_row = rows - 1 - row if height > 0 else row
        start = offset + source_row * stride
        pixels.append([
            (data[start + x * channels + 2], data[start + x * channels + 1], data[start + x * channels])
            for x in range(width)
        ])
    return pixels


def near(pixel, color):
    return all(abs(channel - target) <= tolerance for channel, target in zip(pixel, color))


def locate(pixels, color):
    count = 0
    total_x = 0
    for row in pixels:
        for x, pixel in enumerate(row):
            if near(pixel, color):
                count += 1
                total_x += x
    return count, (total_x / count if count else None)


def launch(root, host, log):
    environment = dict(os.environ, SABINE_TRACE="1", SABINE_HOST_PATH=host)
    options = (
        {"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP}
        if WINDOWS else {"start_new_session": True}
    )
    return subprocess.Popen(
        [repository / f"target/release/sabine-notes{executable_suffix}", "--system"],
        cwd=root, env=environment, stdout=log, stderr=subprocess.STDOUT, **options,
    )


def stop(app):
    if WINDOWS:
        subprocess.run(["taskkill", "/PID", str(app.pid), "/T", "/F"], capture_output=True)
        app.wait(timeout=15)
        return
    os.killpg(app.pid, signal.SIGTERM)
    try:
        app.wait(timeout=15)
    except subprocess.TimeoutExpired:
        os.killpg(app.pid, signal.SIGKILL)


def keychain_prompted(started):
    security_log = subprocess.run(
        ["log", "show", "--start", started, "--style", "compact",
         "--predicate", 'process == "securityd"'],
        capture_output=True, text=True,
    ).stdout
    (output / "securityd.log").write_text(security_log)
    return "displaying keychain prompt" in security_log


def main():
    if output.exists():
        shutil.rmtree(output)
    output.mkdir(parents=True)
    host = prepare_host()
    root = output / "app"
    write_app(root)
    log_path = output / "app.log"
    started = time.strftime("%Y-%m-%d %H:%M:%S")
    with log_path.open("w") as log:
        app = launch(root, host, log)
    failures = []
    report = []
    try:
        deadline = time.monotonic() + 180
        while "browser.first_paint" not in log_path.read_text(errors="replace"):
            if app.poll() is not None or time.monotonic() > deadline:
                raise SystemExit("The window did not paint its first frame\n" + log_path.read_text(errors="replace"))
            time.sleep(0.5)
        time.sleep(3)
        pids = app_processes(app.pid)
        for name, percent in cpu_usage(pids, 10).items():
            report.append(f"{name} CPU during animation: {percent:.1f}%")
        first = screenshot("first")
        time.sleep(1)
        second = screenshot("second")
    finally:
        stop(app)

    if MACOS and keychain_prompted(started):
        failures.append("macOS asked for the login keychain password while the app ran")

    log = log_path.read_text(errors="replace")
    accelerated = "first accelerated paint" in log
    report.append("Paint path: " + (paint_path if accelerated else "software"))
    if not accelerated:
        failures.append("Chromium delivered software frames, so the accelerated path did not run")
    for message in failure_messages:
        if message in log:
            failures.append(f"The log reports: {message}")

    page_pixels, _ = locate(first, page_color)
    first_count, first_x = locate(first, square_color)
    second_count, second_x = locate(second, square_color)
    report.append(f"Page-colored pixels on screen: {page_pixels}")
    report.append(f"Square pixels: {first_count} then {second_count}")
    if page_pixels < 50_000:
        failures.append("The page color is missing from the screen")
    if min(first_count, second_count) < 5_000:
        failures.append("The animated square is missing from the screen")
    elif abs(first_x - second_x) < 4:
        failures.append("The animated square stopped moving, so new frames are not reaching the screen")
    else:
        report.append(f"Square moved {abs(first_x - second_x):.0f} px between screenshots")

    (output / "report.txt").write_text("\n".join(report + failures) + "\n")
    print("\n".join(report))
    if failures:
        print("\n".join(failures), file=sys.stderr)
        raise SystemExit(1)


main()
