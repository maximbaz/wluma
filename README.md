# wluma

A tool for wlroots-based compositors that automatically adjusts screen brightness based on the screen contents and amount of ambient light around you.

## Idea

The app will automatically brighten the screen when you are looking at a dark window (such as a fullscreen terminal) and darken the screen when you are looking at a bright window (such as web browser). The algorithm takes into consideration the amount of ambient light around you, so the same window can be brighter during the day than during the night.

With permission of [Lumen](https://github.com/anishathalye/lumen)'s author (the project that inspired me to create this app), I'm reusing a demo GIF:

![demo](https://user-images.githubusercontent.com/1177900/82347130-8bd22b80-99f7-11ea-8545-0d311240a30d.gif)

## Usage

Simply launch `wluma` and continue adjusting your screen brightness as you usually do - the app will learn your preferences.

`wluma` will not do anything on the first launch! You have to adjust the brightness by hand a few times, in different environment and/or with different screen contents, that way `wluma` will learn your preferences and only then it will begin to automatically change your screen brightness for you.

## Performance

The app has minimal impact on system resources and battery life even though it is able to monitor screen contents several times a second. This is achieved by using [export-dmabuf](https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-export-dmabuf-unstable-v1.xml) Wayland protocol to get access to the screen contents and doing computations entirely on GPU using Vulkan API.

## Installation

<a href="https://repology.org/project/wluma/versions">
  <img src="https://repology.org/badge/vertical-allrepos/wluma.svg" alt="Packaging status" align="right">
</a>

Use one of the available packages and methods below:

- Alpine Linux: [wluma](https://pkgs.alpinelinux.org/packages?name=wluma) (from Alpine Edge; it will be available in stable branches since Alpine v3.16)
- Arch Linux: [wluma](https://aur.archlinux.org/packages/wluma/) or [wluma-git](https://aur.archlinux.org/packages/wluma-git/)
- NixOS: [wluma](https://search.nixos.org/packages?channel=unstable&show=wluma&from=0&size=50&sort=relevance&type=packages&query=wluma)
- Pre-compiled [Github release artifact](https://github.com/maximbaz/wluma/releases) (it is linked against Vulkan ICD loader, which you must install, and the latest available `glibc`, which might not work on your machine if your version is too old)
- Build the app yourself using the instructions below and install it via `sudo make install`

## Build

[![CI](https://github.com/maximbaz/wluma/actions/workflows/ci.yml/badge.svg)](https://github.com/maximbaz/wluma/actions/workflows/ci.yml)

If you want to build the app yourself, make sure you use latest stable Rust, otherwise you might get compilation errors! Using `rustup` is perhaps the easiest. Ubuntu needs the following dependencies: `sudo apt-get -y install v4l-utils libv4l-dev libudev-dev libvulkan-dev libdbus-1-dev`.

Then simply run `make build`.

## Permissions

In order to access backlight devices, `wluma` must either:

- have direct driver access: install the supplied `90-wluma-backlight.rules` udev rule, add your user to the `video` group and reboot (fastest)
- run on a system that uses `elogind` or `systemd-logind` (they provide a safe interface for unprivileged users to control device's brightness through `dbus`, no configuration necessary)
- run as `root` (not recommended)

## Configuration

The `config.toml` in repository represents default config values. To change them, copy the file into `$XDG_CONFIG_HOME/wluma/config.toml` and adjust as desired.

### ALS

Choose whether to use a real IIO-based ambient light sensor (`[als.iio]`), a webcam-based simulation (`[als.webcam]`), a time-based simulation (`[als.time]`), reading from the output of a command (`[als.cmd]`), or disable it altogether (`[als.none]`).

Each of them contains a `thresholds` field, which comes with good default values. It is there to convert generally exponential lux values into a linear scale to improve the prediction algorithm in `wluma`. Keys are the raw values from ambient light sensor (maximal value depends on the implementation), values are arbitrary "profiles". `wluma` will predict the best screen brightness according to the data learned within the same ALS profile.

### Displays

Multiple outputs are supported, using `backlight` (common for internal laptop screens) and `ddcutil` (for external screens).

Each output is identified by compositor using model, manufacturer and serial number (e.g.`eDP-1 'Sharp Corporation 0x14A8 0x00000000' (eDP-1)`.

The `name` field in the output config will be matched as a substring, so you are free to put simply `eDP-1`, or a serial number (if you have two identical external screens). It is your responsibility to make sure that the values you use match **uniquely** to one output only.

The `capturer` field will determine how screen contents will be captured. Currently supported values are `wlroots` (works only on wlroots-based Wayland compositors) and `none` (ignores screen contents and predicts brightness only based on ALS).

_Tip:_ run `wluma` with `RUST_LOG=debug` to see how your outputs are being identified, so that you can choose an appropriate `name` configuration value.

## Run

To run the app, simply launch `wluma` or use the provided systemd user service.

## Debugging

To enable logging, set environment variable `RUST_LOG` to one of these values: `error`, `warn`, `info`, `debug`, `trace`.

For more complex selectors, see [env_logger's documentation](https://docs.rs/env_logger/latest/env_logger/#enabling-logging).

## Known issues (help wanted!)

Help is wanted and much appreciated! If you want to implement some of these, feel free to open an issue and I'll provide more details and try to help you along the way.

- Support for frames with custom DRM modifiers (e.g. multi-planar frames) is currently not implemented. This was recently [implemented in mesa](https://gitlab.freedesktop.org/mesa/mesa/-/merge_requests/1466) and can finally be added to `wluma`. Until then, a workaround is to export `WLR_DRM_NO_MODIFIERS=1` before launching your wlroots-based compositor.
- Changing screen resolution while `wluma` is running is not supported yet, and should crash the app. Workaround: restart `wluma` after changing resolution.
- Plugging in a screen while `wluma` is running. Workaround: restart `wluma`.

## Relevant projects

- [lumen](https://github.com/anishathalye/lumen): project that inspired me to create this app
