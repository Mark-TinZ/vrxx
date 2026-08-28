# meson.build
#
# Copyright 2026 Mark
#
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# SPDX-License-Identifier: MPL-2.0

pkgdatadir = get_option('prefix') / get_option('datadir') / meson.project_name()
gnome = import('gnome')

gnome.compile_resources('vrxx',
  'vrxx.gresource.xml',
  gresource_bundle: true,
  install: true,
  install_dir: pkgdatadir,
)

conf = configuration_data()
conf.set_quoted('VERSION', meson.project_version())
conf.set_quoted('GETTEXT_PACKAGE', 'vrxx')
conf.set_quoted('LOCALEDIR', get_option('prefix') / get_option('localedir'))
conf.set_quoted('PKGDATADIR', pkgdatadir)

configure_file(
    input: 'config.rs.in',
    output: 'config.rs',
    configuration: conf
)

# Подключаем папку с custom icons
custom_icons_dir = meson.project_source_root() / 'data' / 'icons' / 'custom'
run_command(
  'cp', '-r', custom_icons_dir, meson.project_build_root() / 'data' / 'icons' / 'custom',
  check: false
)

cargo_bin  = find_program('cargo')
cargo_opt  = [ '--manifest-path', meson.project_source_root() / 'Cargo.toml' ]
cargo_opt += [ '--target-dir', meson.project_build_root()  / 'src' ]
config_rs_path = meson.project_build_root() / 'src' / 'config.rs'

if get_option('buildtype') == 'release'
  cargo_opt += [ '--release' ]
  rust_target = 'release'
else
  rust_target = 'debug'
endif

sh = find_program('sh')

cargo_build = custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: meson.project_name(),
  console: true,
  install: true,
  install_dir: get_option('bindir'),
  command: [
    sh, '-c',
    'VRXX_CONFIG_RS_PATH="@0@" @1@ build @2@ && cp "@3@" "@4@"'.format(
      config_rs_path,
      cargo_bin.full_path(),
      ' '.join(cargo_opt),
      meson.project_build_root() / 'src' / rust_target / meson.project_name(),
      '@OUTPUT@'
    )
  ]
)

