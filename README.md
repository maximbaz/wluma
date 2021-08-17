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

Alternatively, build using `make build` and install via `sudo make install`.

## Permissions

In order to access backlight devices, `wluma` must either run as `root`, or preferrably instead you should add your user to `video` group (and possibly reboot thereafter).

## Run

To run the app, simply launch `wluma` or use the provided systemd user service.

## Caveats

- Current drivers do not support importing images with custom DRM modifiers, this work [is being done in mesa](https://gitlab.freedesktop.org/mesa/mesa/-/merge_requests/1466). Until then, the only workaround is to use `WLR_DRM_NO_MODIFIERS=1` from wlroots.

## Relevant projects

- [lumen](https://github.com/anishathalye/lumen): project that inspired me to create this app
