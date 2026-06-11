# cef_winit

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
bundle-cef-app cef_winit -o target/bundle
open target/bundle/cef_winit.app
```

## Linux

Linux native child embedding requires X11/XWayland.

```sh
LD_LIBRARY_PATH=.cache/cef:$LD_LIBRARY_PATH \
cargo run --features linux-x11
```

On Arch, useful runtime packages are:

```sh
sudo pacman -S --needed nss nspr libxkbcommon-x11 at-spi2-core alsa-lib gtk3 xorg-xwayland
```
