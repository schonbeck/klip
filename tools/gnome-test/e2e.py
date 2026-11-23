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
def klip(sel, log):
    env = dict(os.environ, KLIP_DEBUG="1", KLIP_TEST_SELECT=sel)
    return subprocess.Popen([os.path.expanduser("~/klip/target/release/klip")], env=env, stdout=open(log, "w"), stderr=subprocess.STDOUT)
ESC, RET = 0xff1b, 0xff0d
pump(1)
import json
seq = json.loads(sys.argv[1])
procs = []
for a in seq:
    c = a[0]
    if c == "klip": procs.append(klip(a[1], a[2])); print("started klip", a[1])
    elif c == "wait": pump(a[1])
    elif c == "key": key({"esc": ESC, "ret": RET, "shift": 0xffe1}[a[1]]); print("key", a[1])
    elif c == "shot": shot(a[1])
    elif c == "clip": clip_read(a[1])
    elif c == "status": print("klip procs:", [p.poll() for p in procs], "saved:", glob.glob(os.path.expanduser("~/**/klip-*.png"), recursive=True))
    elif c == "copytext":
        call(RD, sess, SI, "SetSelection", GLib.Variant("(a{sv})", ({"mime-types": GLib.Variant("as", ["text/plain;charset=utf-8"])},))); print("RD took clipboard ownership")
for p in procs:
    if p.poll() is None: p.terminate()
call(RD, sess, SI, "Stop")
