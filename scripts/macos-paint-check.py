#!/usr/bin/env python3
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import sys
import time

if sys.platform != "darwin":
    raise SystemExit("This check requires macOS")

repository = Path(__file__).resolve().parent.parent
output = Path(os.environ.get("SABINE_PAINT_CHECK_OUTPUT", repository / "macos-paint"))
page_color = (0x33, 0x66, 0x99)
square_color = (0xFF, 0x99, 0x00)
tolerance = 40

PAGE = """<!doctype html>
<html>
<body style="margin:0;overflow:hidden;background:#336699">
<div id="square" style="position:fixed;top:120px;left:0;width:120px;height:120px;background:#ff9900"></div>
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


def run(*command, **options):
    print("$", " ".join(str(part) for part in command), flush=True)
    return subprocess.run(command, cwd=repository, check=True, **options)


def prepare_host():
    run("cargo", "build", "-p", "sabine-cli", "-p", "sabine-notes")
    prepared = run(
        repository / "target/debug/sabine", "runtime", "prepare",
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


def processes():
    listing = subprocess.run(
        ["ps", "-A", "-o", "pid=,ppid=,time=,command="],
        capture_output=True, text=True, check=True,
    ).stdout
    rows = []
    for line in listing.splitlines():
        pid, ppid, cpu, command = line.split(None, 3)
        rows.append((int(pid), int(ppid), cpu, command))
    return rows


def cpu_seconds(value):
    seconds = 0.0
    for part in value.split(":"):
        seconds = seconds * 60 + float(part)
    return seconds


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
        return {name: cpu_seconds(rows[pid]) for name, pid in pids.items() if pid in rows}

    before = sample()
    time.sleep(seconds)
    after = sample()
    return {name: (after[name] - before[name]) / seconds * 100 for name in before if name in after}


def screenshot(name):
    png = output / f"{name}.png"
    bmp = output / f"{name}.bmp"
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


def main():
    if output.exists():
        shutil.rmtree(output)
    output.mkdir(parents=True)
    host = prepare_host()
    root = output / "app"
    write_app(root)
    log_path = output / "app.log"
    environment = dict(os.environ, SABINE_TRACE="1", SABINE_HOST_PATH=host)
    with log_path.open("w") as log:
        app = subprocess.Popen(
            [repository / "target/debug/sabine-notes", "--system"],
            cwd=root, env=environment, stdout=log, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
    failures = []
    report = []
    try:
        deadline = time.monotonic() + 180
        while "browser.first_paint" not in log_path.read_text():
            if app.poll() is not None or time.monotonic() > deadline:
                raise SystemExit("The window did not paint its first frame\n" + log_path.read_text())
            time.sleep(0.5)
        time.sleep(3)
        pids = app_processes(app.pid)
        for name, percent in cpu_usage(pids, 10).items():
            report.append(f"{name} CPU during animation: {percent:.1f}%")
        first = screenshot("first")
        time.sleep(1)
        second = screenshot("second")
    finally:
        os.killpg(app.pid, signal.SIGTERM)
        try:
            app.wait(timeout=15)
        except subprocess.TimeoutExpired:
            os.killpg(app.pid, signal.SIGKILL)

    log = log_path.read_text()
    accelerated = "first accelerated paint" in log
    report.append("Paint path: " + ("accelerated IOSurface" if accelerated else "software"))
    if not accelerated:
        failures.append("Chromium delivered software frames, so the accelerated path did not run")
    for message in (
        "accelerated texture import failed",
        "Could not share browser surfaces",
        "shared paint copy failed",
        "Metal is unavailable",
    ):
        if message in log:
            failures.append(f"The log reports: {message}")

    page_pixels, _ = locate(first, page_color)
    first_count, first_x = locate(first, square_color)
    second_count, second_x = locate(second, square_color)
    report.append(f"Page-colored pixels on screen: {page_pixels}")
    report.append(f"Square pixels: {first_count} then {second_count}")
    if page_pixels < 50_000:
        failures.append("The page color is missing from the screen")
    if not first_x or not second_x:
        failures.append("The animated square is missing from the screen")
    elif abs(first_x - second_x) < 4:
        failures.append("The animated square stopped moving, so new frames are not reaching the screen")
    else:
        report.append(f"Square moved {abs(first_x - second_x):.0f} px in one second")

    (output / "report.txt").write_text("\n".join(report + failures) + "\n")
    print("\n".join(report))
    if failures:
        print("\n".join(failures), file=sys.stderr)
        raise SystemExit(1)


main()
