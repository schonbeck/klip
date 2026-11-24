# Take a non-interactive screenshot via the portal; print the file URI.
import sys, random
from gi.repository import Gio, GLib
bus = Gio.bus_get_sync(Gio.BusType.SESSION)
token = "t%d" % random.randint(0, 1 << 30)
sender = bus.get_unique_name()[1:].replace(".", "_")
path = f"/org/freedesktop/portal/desktop/request/{sender}/{token}"
loop = GLib.MainLoop()
def on_resp(conn, snd, obj, iface, sig, params):
    code, res = params.unpack()
    print(code, res.get("uri"))
    loop.quit()
bus.signal_subscribe(None, "org.freedesktop.portal.Request", "Response", path, None, 0, on_resp)
bus.call_sync("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop",
    "org.freedesktop.portal.Screenshot", "Screenshot",
    GLib.Variant("(sa{sv})", ("", {"handle_token": GLib.Variant("s", token), "interactive": GLib.Variant("b", False)})),
    None, 0, -1, None)
GLib.timeout_add_seconds(20, lambda: (print("timeout"), loop.quit()))
loop.run()
