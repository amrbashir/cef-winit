# cef-winit

Standalone Winit + CEF native child-window prototype.

CEF downloads are cached under `.cache/cef` via `.cargo/config.toml`.

## Windows

```powershell
cargo run
```

## macOS

Build the `.app` bundle from this directory:

```sh
cargo install cef --version 148.4.0 --features build-util --bin bundle-cef-app
bundle-cef-app cef-winit -o target/bundle
open target/bundle/cef-winit.app
```

## Linux

Linux native child embedding requires X11/XWayland.

```sh
WINIT_UNIX_BACKEND=x11 cargo run --features linux-x11
```

On Debian/WSL, install CEF's runtime dependencies with:

```sh
sudo apt update
sudo apt install -y libnspr4 libnss3 libatk1.0-0t64 libatk-bridge2.0-0t64 libasound2t64 libatspi2.0-0t64
```

On Arch, useful runtime packages are:

```sh
sudo pacman -S --needed nss nspr libxkbcommon-x11 at-spi2-core alsa-lib gtk3 xorg-xwayland
```
