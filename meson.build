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
], language: 'c')


wayland_client = dependency('wayland-client')
wayland_protocols = dependency('wayland-protocols')

sources = ['src/main.c']

subdir('protocol')

dependencies = [
    client_protos,
	wayland_client,
]

executable(
    meson.project_name(),
    sources,
    dependencies: dependencies,
)
