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

Alternatively, download the [release artifact](https://github.com/maximbaz/wluma/releases) or build the app yourself, and then install it via `sudo make install`.

## Build

[![CI](https://github.com/maximbaz/wluma/actions/workflows/ci.yml/badge.svg)](https://github.com/maximbaz/wluma/actions/workflows/ci.yml)

If you want to build the app yourself, make sure you use latest stable Rust, otherwise you might get compilation errors! Using `rustup` is perhaps the easiest.

Then simply run `make build`.

## Permissions

In order to access backlight devices, `wluma` must either run as `root`, or preferrably instead you should add your user to `video` group (and possibly reboot thereafter).

## Configuration

The `config.toml` in repository represents default config values. To change them, copy the file into `$XDG_CONFIG_HOME/wluma/config.toml` and adjust as desired.

### ALS

Choose whether to use a real IIO-based ambient light sensor (`[als.iio]`), a time-based simulation (`[als.time]`) or disable it altogether (`[als.none]`).

`[als.iio]` contains a `thresholds` field, which comes with good default values. It is there to convert a generally exponential lux values into a linear scale to improve the prediction algorithm in `wluma`. A value of `[100, 200]` would mean that a raw lux value of `0..100` would get converted to `0`, a value of `100..200` would get converted to `1`, and `200+` would get converted to `2`.

`[als.time]` contains a `time_to_lux` mapping, which allows you to express how bright or dark it gets as the day passes by. This mode is primarily meant to let people who don't have a real ALS to try the app and get some meaningful results. Use linear smooth lux values, not raw ones - a range of `0..5` is recommended. A mapping of `{ 3 = 1, 7 = 2, 21 = 0 }` means that from `00:00` until `02:59` a value would be `0`, from `03:00` until `06:59` the value would be `2`, from `07:00` until `20:59` the value would be `2`, and finally between `21:00` and `23:59` the value would again be `0`.

## Run

To run the app, simply launch `wluma` or use the provided systemd user service.

## Caveats

- Current drivers do not support importing images with custom DRM modifiers, this work [is being done in mesa](https://gitlab.freedesktop.org/mesa/mesa/-/merge_requests/1466). Until then, the only workaround is to use `WLR_DRM_NO_MODIFIERS=1` from wlroots.

## Relevant projects

- [lumen](https://github.com/anishathalye/lumen): project that inspired me to create this app
