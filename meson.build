project(
    'wlsun',
    'c',
    version: '0.1.0',
    license: 'MIT',
    default_options: [
        'buildtype=debug',
        'c_std=c11',
        'warning_level=2',
        'werror=false',
    ],
)

add_project_arguments([
    '-Wno-unused',
    '-DWLR_USE_UNSTABLE',
    '-DENABLE_DMABUF',
], language: 'c')

wayland_client = dependency('wayland-client')

egl = dependency('egl')
glesv2 = dependency('glesv2')
wlroots = dependency('wlroots')

cc = meson.get_compiler('c')
math = cc.find_library('m', required : false)

sources = ['src/main.c']

subdir('protocol')

dependencies = [
    client_protos,
    egl,
    glesv2,
    math,
    wlroots,
]

executable(
    meson.project_name(),
    sources,
    dependencies: dependencies,
)
