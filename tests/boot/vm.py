"""Drive an AiOS test image in QEMU: serial shell, monitor, screenshots, keys.

Used by tests/boot, tests/injection and tests/latency. Only the *test* image
has the root shell on ttyS0 that this relies on.
"""
import os
import re
import socket
import subprocess
import time
import uuid

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
KVM = os.access("/dev/kvm", os.W_OK)
SLOW = 1 if KVM else 10  # timeouts are scaled by 10 without KVM (plan 4.1)

# QEMU sendkey names for characters typed into the VM.
KEYS = {" ": "spc", "-": "minus", "=": "equal", ".": "dot", ",": "comma", "/": "slash",
        ";": "semicolon", "'": "apostrophe", "[": "bracket_left", "]": "bracket_right",
        "\\": "backslash", "\n": "ret", "\t": "tab"}
SHIFTED = {":": "semicolon", "_": "minus", "+": "equal", "?": "slash", '"': "apostrophe",
           "!": "1", "@": "2", "#": "3", "$": "4", "%": "5", "&": "7", "*": "8", "(": "9", ")": "0",
           "<": "comma", ">": "dot"}


class VM:
    def __init__(self, image, name="vm", hostfwd="", log_dir=None, snapshot=True, disk_if="virtio"):
        self.image = image
        self.name = name
        self.dir = os.path.join(REPO, "build", "vm-" + name)
        os.makedirs(self.dir, exist_ok=True)
        self.serial_path = os.path.join(self.dir, "serial.sock")
        self.monitor_path = os.path.join(self.dir, "monitor.sock")
        self.log_path = os.path.join(log_dir or os.path.join(REPO, "logs"), f"serial-{name}.log")
        self.hostfwd = hostfwd
        self.snapshot = snapshot
        self.disk_if = disk_if
        self.proc = None
        self.buf = b""
        self.t0 = None

    def start(self):
        for p in (self.serial_path, self.monitor_path):
            if os.path.exists(p):
                os.unlink(p)
        env = dict(os.environ, SERIAL="unix:" + self.serial_path, MONITOR=self.monitor_path,
                   SNAPSHOT="1" if self.snapshot else "0", HOSTFWD=self.hostfwd, DISK_IF=self.disk_if)
        self.t0 = time.monotonic()
        self.proc = subprocess.Popen([os.path.join(REPO, "scripts", "22-run-qemu.sh"), self.image],
                                     env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        deadline = time.time() + 30
        while not os.path.exists(self.serial_path):
            if self.proc.poll() is not None or time.time() > deadline:
                raise RuntimeError("qemu did not start: " + self.proc.stderr.read().decode())
            time.sleep(0.1)
        self.serial = socket.socket(socket.AF_UNIX)
        self.serial.connect(self.serial_path)
        self.serial.settimeout(0.5)
        self.log = open(self.log_path, "wb")
        return self

    def _pump(self):
        try:
            data = self.serial.recv(65536)
        except socket.timeout:
            return False
        if data:
            self.buf += data
            self.log.write(data)
            self.log.flush()
        return bool(data)

    def wait_for(self, pattern, timeout):
        """Wait until `pattern` (bytes regex) appears in serial output after
        the current read position; returns the match."""
        rx = re.compile(pattern)
        deadline = time.time() + timeout * SLOW
        while True:
            m = rx.search(self.buf)
            if m:
                self.buf = self.buf[m.end():]
                return m
            if time.time() > deadline:
                raise TimeoutError(f"{pattern!r} not seen within {timeout * SLOW}s; tail: {self.buf[-400:]!r}")
            self._pump()

    def write(self, s):
        self.serial.sendall(s.encode())

    def login(self, timeout=120):
        """Wait for the autologin root shell on ttyS0 and make it script-friendly."""
        self.wait_for(rb"root@aios[^\n]*[#$] ?", timeout)
        self.write("stty -echo; export PS1='' TERM=dumb; unset PROMPT_COMMAND\n")
        time.sleep(1)
        self.run("true")

    def run(self, cmd, timeout=60, check=True):
        tag = uuid.uuid4().hex[:8]
        # Separate lines so a command ending in `&` stays valid shell.
        self.write(f"echo B{tag}\n{cmd}\necho E{tag} $?\n")
        self.wait_for(f"B{tag}\r?\n".encode(), timeout)
        m = self.wait_for(rf"([\s\S]*?)E{tag} (\d+)".encode(), timeout)
        out = m.group(1).decode(errors="replace").replace("\r", "").strip()
        code = int(m.group(2))
        if check and code != 0:
            raise RuntimeError(f"`{cmd}` exited {code}: {out}")
        return out if check else (code, out)

    def monitor(self, cmd):
        s = socket.socket(socket.AF_UNIX)
        s.connect(self.monitor_path)
        s.settimeout(5)
        s.recv(4096)
        s.sendall((cmd + "\n").encode())
        time.sleep(0.3)
        try:
            out = s.recv(65536)
        except socket.timeout:
            out = b""
        s.close()
        return out.decode(errors="replace")

    def screenshot(self, png_path):
        """Screen dump through the monitor, saved as PNG. Returns the number
        of distinct colours (a blank screen has very few)."""
        from PIL import Image
        ppm = os.path.join(self.dir, "shot.ppm")
        if os.path.exists(ppm):
            os.unlink(ppm)
        self.monitor(f"screendump {ppm}")
        for _ in range(50):
            if os.path.exists(ppm) and os.path.getsize(ppm) > 0:
                break
            time.sleep(0.1)
        time.sleep(0.3)
        img = Image.open(ppm).convert("RGB")
        os.makedirs(os.path.dirname(png_path), exist_ok=True)
        img.save(png_path)
        return len(img.getcolors(1 << 24) or [])

    def sendkey(self, key):
        self.monitor(f"sendkey {key}")

    def type(self, text, delay=0.05):
        for ch in text:
            if ch.isalpha() and ch.isupper():
                key = "shift-" + ch.lower()
            elif ch.isalnum():
                key = ch
            elif ch in KEYS:
                key = KEYS[ch]
            elif ch in SHIFTED:
                key = "shift-" + SHIFTED[ch]
            else:
                raise ValueError(f"cannot type {ch!r}")
            self.sendkey(key)
            time.sleep(delay * SLOW)

    def stop(self):
        if self.proc and self.proc.poll() is None:
            try:
                self.monitor("quit")
            except OSError:
                pass
            try:
                self.proc.wait(10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
        if getattr(self, "log", None):
            self.log.close()
