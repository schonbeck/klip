class Klip < Formula
  desc "Simple Wayland screenshot tool: select, annotate, copy or save"
  homepage "https://github.com/schonbeck/klip"
  url "https://github.com/schonbeck/klip/releases/download/v0.1.0/klip-0.1.0.tar.gz"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  license "MIT"

  depends_on "pkgconf" => :build
  depends_on "rust" => :build
  depends_on "cairo"
  depends_on "gdk-pixbuf"
  depends_on "glib"
  depends_on "graphene"
  depends_on "gtk4"
  depends_on :linux
  depends_on "pango"

  def install
    ENV["KLIP_GIT_HASH"] = "v#{version}"
    system "cargo", "install", *std_cargo_args
    (share/"applications").install "data/io.github.schonbeck.klip.desktop"
    (share/"icons/hicolor/scalable/apps").install "data/io.github.schonbeck.klip.svg"
  end

  def caveats
    <<~EOS
      klip needs a Wayland desktop with xdg-desktop-portal (GNOME, KDE, ...).
      Run `klip` once: it stays in the background (tray icon, Ctrl+Alt+P)
      and starts at login. `klip --settings` changes the shortcut, save folder
      and autostart.
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/klip --version")
  end
end
