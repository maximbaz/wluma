# wluma

A tool for wlroots-based compositors that automatically adjusts screen brightness based on the screen contents and amount of ambient light around you.

## Idea

The app will automatically brighten the screen when you are looking at a dark window (such as a fullscreen terminal) and darken the screen when you are looking at a bright window (such as web browser). The algorithm takes into consideration the amount of ambient light around you, so the same window can be brighter during the day than during the night.

With permission of [Lumen](https://github.com/anishathalye/lumen)'s author (the project that inspired me to create this app), I'm reusing a demo GIF:

![demo](https://user-images.githubusercontent.com/1177900/82347130-8bd22b80-99f7-11ea-8545-0d311240a30d.gif)

## Usage

Simply launch `wluma` and continue adjusting your screen brightness as you usually do - the app will learn your preferences.

## Performance

The app has minimal impact on system resources and battery life even though it is able to monitor screen contents several times a second. This is achieved by using [export-dmabuf](https://github.com/swaywm/wlr-protocols/blob/master/unstable/wlr-export-dmabuf-unstable-v1.xml) Wayland protocol to get access to the screen contents and doing computations entirely on GPU using Vulkan API.

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

## Debugging

To enable logging, set environment variable `RUST_LOG` to one of these values: `error`, `warn`, `info`, `debug`, `trace`.

### ALS

Choose whether to use a real IIO-based ambient light sensor (`[als.iio]`), a webcam-based simulation (`[als.webcam]`), a time-based simulation (`[als.time]`) or disable it altogether (`[als.none]`).

`[als.iio]` contains a `thresholds` field, which comes with good default values. It is there to convert a generally exponential lux values into a linear scale to improve the prediction algorithm in `wluma`. A value of `[100, 200]` would mean that a raw lux value of `0..100` would get converted to `0`, a value of `100..200` would get converted to `1`, and `200+` would get converted to `2`.

`[als.webcam]` contains a `video` field corresponding to your device (e.g. `0` for `/dev/video0`), as well as a `thresholds` field, just like in `[als.iio]`, which maps "perceived lightness" percentage (0..100) calculated from the webcam frame to a smaller subset of values (default value is recommended).

`[als.time]` contains a `time_to_lux` mapping, which allows you to express how bright or dark it gets as the day passes by. This mode is primarily meant to let people who don't have a real ALS to try the app and get some meaningful results. Use linear smooth lux values, not raw ones - a range of `0..5` is recommended. A mapping of `{ 3 = 1, 7 = 2, 21 = 0 }` means that from `00:00` until `02:59` a value would be `0`, from `03:00` until `06:59` the value would be `2`, from `07:00` until `20:59` the value would be `2`, and finally between `21:00` and `23:59` the value would again be `0`.

## Run

To run the app, simply launch `wluma` or use the provided systemd user service.

## Known issues (help wanted!)

Help is wanted and much appreciated! If you want to implement some of these, feel free to open an issue and I'll provide more details and try to help you along the way.

- Support for frames with custom DRM modifiers (e.g. multi-planar frames) is currently not implemented. This was recently [implemented in mesa](https://gitlab.freedesktop.org/mesa/mesa/-/merge_requests/1466) and can finally be added to `wluma`. Until then, a workaround is to export `WLR_DRM_NO_MODIFIERS=1` before launching your wlroots-based compositor.
- Changing screen resolution while `wluma` is running is not supported yet, and should crash the app. Workaround: restart `wluma` after changing resolution.
- Selecting screen is not implemented yet, on start `wluma` will pick one screen at random and use it. If the screen disappears (e.g. you launch `wluma` on laptop, then connect a docking station and disable internal screen), it should crash the app. Workaround: restart `wluma` after changing screens.

## Relevant projects

- [lumen](https://github.com/anishathalye/lumen): project that inspired me to create this app
