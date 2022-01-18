# wluma

A tool for wlroots-based compositors that automatically adjusts screen brightness based on the screen contents and amount of ambient light around you.

## Idea

The app will automatically brighten the screen when you are looking at a dark window (such as a fullscreen terminal) and darken the screen when you are looking at a bright window (such as web browser). The algorithm takes into consideration the amount of ambient light around you, so the same window can be brighter during the day than during the night.

With permission of [Lumen](https://github.com/anishathalye/lumen)'s author (the project that inspired me to create this app), I'm reusing a demo GIF:

![demo](https://user-images.githubusercontent.com/1177900/82347130-8bd22b80-99f7-11ea-8545-0d311240a30d.gif)

## Usage

Simply launch `wluma` and continue adjusting your screen brightness as you usually do - the app will learn your preferences.

## Performance

The app has minimal impact on system resources and battery life even though it is able to monitor screen contents several times a second. This is achieved by using [export-dmabuf](https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-export-dmabuf-unstable-v1.xml) Wayland protocol to get access to the screen contents and doing computations entirely on GPU using Vulkan API.

## Installation

On Arch Linux you can use [wluma](https://aur.archlinux.org/packages/wluma/) or [wluma-git](https://aur.archlinux.org/packages/wluma-git/) packages.

On NixOS you can use [wluma](https://search.nixos.org/packages?channel=unstable&show=wluma&from=0&size=50&sort=relevance&type=packages&query=wluma) package.

Alternatively, download the [release artifact](https://github.com/maximbaz/wluma/releases) (it is linked against latest available `glibc` and might not work on your machine) or build the app yourself, and then install it via `sudo make install`.

## Build

[![CI](https://github.com/maximbaz/wluma/actions/workflows/ci.yml/badge.svg)](https://github.com/maximbaz/wluma/actions/workflows/ci.yml)

If you want to build the app yourself, make sure you use latest stable Rust, otherwise you might get compilation errors! Using `rustup` is perhaps the easiest.

Then simply run `make build`.

## Permissions

In order to access backlight devices, `wluma` must either run as `root`, or preferrably instead you should add your user to `video` group (and possibly reboot thereafter).

## Configuration

The `config.toml` in repository represents default config values. To change them, copy the file into `$XDG_CONFIG_HOME/wluma/config.toml` and adjust as desired.

### ALS

Choose whether to use a real IIO-based ambient light sensor (`[als.iio]`), a webcam-based simulation (`[als.webcam]`), a time-based simulation (`[als.time]`) or disable it altogether (`[als.none]`).

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
