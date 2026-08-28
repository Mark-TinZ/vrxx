# meson.build
#
# Copyright 2026 Mark
#
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# SPDX-License-Identifier: MPL-2.0

application_id = 'ru.mark.vrxx'

scalable_dir = 'hicolor' / 'scalable' / 'apps'
install_data(
  scalable_dir / ('@0@.svg').format(application_id),
  install_dir: get_option('datadir') / 'icons' / scalable_dir
)

symbolic_dir = 'hicolor' / 'symbolic' / 'apps'
install_data(
  symbolic_dir / ('@0@-symbolic.svg').format(application_id),
  install_dir: get_option('datadir') / 'icons' / symbolic_dir
)

# Устанавливаем symbolic иконки в hicolor
foreach icon_info : [
  ['key5-symbolic.svg', 'key5-symbolic'],
  ['preferences-system-network-proxy-symbolic.svg', 'preferences-system-network-proxy-symbolic'],
  ['funnel-symbolic.svg', 'funnel-symbolic'],
  ['cogged-wheel-symbolic.svg', 'emblem-system-symbolic'],
]
  install_data(
    'custom/symbolic/actions/' + icon_info[0],
    install_dir: get_option('datadir') / 'icons' / 'hicolor' / 'symbolic' / 'apps',
    rename: icon_info[1] + '.svg'
  )
endforeach