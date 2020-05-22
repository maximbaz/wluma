project(
    'wluma',
    'c',
    version: '1.1.0',
    license: 'MIT',
    default_options: [
        'buildtype=release',
        'c_std=c11',
        'warning_level=2',
        'werror=false',
    ],
)

add_project_arguments([
    '-Wno-unused',
    '-Wno-unused-result',
    '-D_FORTIFY_SOURCE=2',
    '-DWLR_USE_UNSTABLE',
    '-DENABLE_DMABUF',
], language: 'c')

wayland_client = dependency('wayland-client')

vulkan = dependency('vulkan')

cc = meson.get_compiler('c')
math = cc.find_library('m', required : false)

sources = ['src/main.c']

subdir('protocol')

dependencies = [
    client_protos,
    vulkan,
    math,
]

executable(
    meson.project_name(),
    sources,
    dependencies: dependencies,
)
