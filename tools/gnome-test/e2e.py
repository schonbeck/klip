"""End-to-end klip test on headless GNOME, driven through Mutter RemoteDesktop
(virtual keyboard/pointer + clipboard access) and the Screenshot portal."""
import os, subprocess, sys, time, glob, random
from gi.repository import Gio, GLib
bus = Gio.bus_get_sync(Gio.BusType.SESSION)
OUT = os.path.expanduser("~/proof"); os.makedirs(OUT, exist_ok=True)
def call(dest, path, iface, method, args=None, rtype=None, fds=None):
    if fds is not None:
        return bus.call_with_unix_fd_list_sync(dest, path, iface, method, args, rtype, 0, -1, fds, None)
    return bus.call_sync(dest, path, iface, method, args, rtype, 0, -1, None)
RD = "org.gnome.Mutter.RemoteDesktop"
sess = call(RD, "/org/gnome/Mutter/RemoteDesktop", RD, "CreateSession").unpack()[0]
SI = RD + ".Session"
call(RD, sess, SI, "Start")
try:
    call(RD, sess, SI, "EnableClipboard", GLib.Variant("(a{sv})", ({},)))
except Exception as e: print("EnableClipboard:", e)
def pump(sec):
    end = time.time() + sec; ctx = GLib.MainContext.default()
    while time.time() < end:
        ctx.iteration(False); time.sleep(0.02)
def key(sym):
    for st in (True, False):
        call(RD, sess, SI, "NotifyKeyboardKeysym", GLib.Variant("(ub)", (sym, st))); pump(0.05)
BTN_LEFT = 272
SCREEN_H = 1080  # the virtual monitor
def move_to(x, y):
    # No absolute motion without a screencast stream. Go to the bottom-left
    # corner first (the top-left one is GNOME's hot corner), then move up,
    # then right.
    for dx, dy in ((-10000.0, 10000.0), (0.0, float(y - (SCREEN_H - 1))), (float(x), 0.0)):
        call(RD, sess, SI, "NotifyPointerMotionRelative", GLib.Variant("(dd)", (dx, dy))); pump(0.05)
def drag(x0, y0, x1, y1, steps=8):
    move_to(x0, y0)
    call(RD, sess, SI, "NotifyPointerButton", GLib.Variant("(ib)", (BTN_LEFT, True))); pump(0.1)
    for i in range(1, steps + 1):
        call(RD, sess, SI, "NotifyPointerMotionRelative", GLib.Variant("(dd)", ((x1 - x0) / steps, (y1 - y0) / steps))); pump(0.04)
    pump(0.1)
    call(RD, sess, SI, "NotifyPointerButton", GLib.Variant("(ib)", (BTN_LEFT, False))); pump(0.3)
def shot(name):
    token = "t%d" % random.randint(0, 1 << 30)
    sender = bus.get_unique_name()[1:].replace(".", "_")
    path = f"/org/freedesktop/portal/desktop/request/{sender}/{token}"
    res = {}
    sid = bus.signal_subscribe(None, "org.freedesktop.portal.Request", "Response", path, None, 0,
        lambda *a: res.update(a[5].unpack()[1]))
    call("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.Screenshot",
        "Screenshot", GLib.Variant("(sa{sv})", ("", {"handle_token": GLib.Variant("s", token), "interactive": GLib.Variant("b", False)})))
    end = time.time() + 15
    while "uri" not in res and time.time() < end: pump(0.1)
    bus.signal_unsubscribe(sid)
    src = res.get("uri", "")[7:]
    if src: os.replace(src, f"{OUT}/{name}.png"); print("shot", name)
    else: print("shot FAILED", name)
def clip_types():
    r = subprocess.run(["wl-paste", "--list-types"], capture_output=True, text=True, timeout=5)
    return (r.stdout + r.stderr).split()
def clip_read(name):
    try:
        ret, fdl = bus.call_with_unix_fd_list_sync(RD, sess, SI, "SelectionRead", GLib.Variant("(s)", ("image/png",)), None, 0, -1, None, None)
        fd = fdl.get(ret.unpack()[0])
        data = b""; f = os.fdopen(fd, "rb", buffering=0); os.set_blocking(fd, False)
        end = time.time() + 5
        while time.time() < end:
            pump(0.05)
            try:
                chunk = f.read(65536)
            except BlockingIOError: continue
            if chunk == b"": break
            if chunk: data += chunk
        open(f"{OUT}/{name}.png", "wb").write(data); print("mutter SelectionRead image/png:", len(data), "bytes, PNG" if data[:4] == b"\x89PNG" else "NOT PNG")
    except Exception as e: print("SelectionRead failed:", e)
def klip(sel, log, args=()):
    env = dict(os.environ, KLIP_DEBUG="1", KLIP_TEST_SELECT=sel)
    return subprocess.Popen([os.path.expanduser("~/klip/target/release/klip"), *args], env=env, stdout=open(log, "w"), stderr=subprocess.STDOUT)
KEYSYMS = {"esc": 0xff1b, "ret": 0xff0d, "shift": 0xffe1, "ctrl": 0xffe3, "super": 0xffeb, "alt": 0xffe9, "tab": 0xff09, "space": 0x20, "down": 0xff54, "up": 0xff52}
def combo(names):
    syms = [KEYSYMS.get(n) or ord(n) for n in names]
    for s in syms:
        call(RD, sess, SI, "NotifyKeyboardKeysym", GLib.Variant("(ub)", (s, True))); pump(0.05)
    for s in reversed(syms):
        call(RD, sess, SI, "NotifyKeyboardKeysym", GLib.Variant("(ub)", (s, False))); pump(0.05)
ESC, RET = 0xff1b, 0xff0d
pump(1)
import json
seq = json.loads(sys.argv[1])
procs = []
for a in seq:
    c = a[0]
    if c == "klip": procs.append(klip(a[1], a[2], a[3] if len(a) > 3 else ())); print("started klip", a[1:])
    elif c == "combo": combo(a[1]); print("combo", "+".join(a[1]))
    elif c == "click": drag(a[1], a[2], a[1], a[2], steps=1); print("click", a[1:3])
    elif c == "sh": print("sh:", subprocess.run(a[1], shell=True, capture_output=True, text=True).stdout.strip())
    elif c == "wait": pump(a[1])
    elif c == "key":
        k = a[1]
        sym = {"esc": ESC, "ret": RET, "shift": 0xffe1}.get(k) or ord(k)
        key(sym); print("key", k)
    elif c == "ctrlkey":
        call(RD, sess, SI, "NotifyKeyboardKeysym", GLib.Variant("(ub)", (0xffe3, True))); pump(0.05)
        key(ord(a[1])); pump(0.05)
        call(RD, sess, SI, "NotifyKeyboardKeysym", GLib.Variant("(ub)", (0xffe3, False))); pump(0.05); print("ctrl+" + a[1])
    elif c == "drag": drag(*a[1:5]); print("drag", a[1:5])
    elif c == "shot": shot(a[1])
    elif c == "clip": clip_read(a[1])
    elif c == "status": print("klip procs:", [p.poll() for p in procs], "saved:", glob.glob(os.path.expanduser("~/**/klip-*.png"), recursive=True))
    elif c == "copytext":
        call(RD, sess, SI, "SetSelection", GLib.Variant("(a{sv})", ({"mime-types": GLib.Variant("as", ["text/plain;charset=utf-8"])},))); print("RD took clipboard ownership")
if not os.environ.get("E2E_KEEP"):
    for p in procs:
        if p.poll() is None: p.terminate()
call(RD, sess, SI, "Stop")
