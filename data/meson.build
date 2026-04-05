desktop_file = i18n.merge_file(
        input: 'ru.mark.vrxx.desktop.in',
       output: 'ru.mark.vrxx.desktop',
         type: 'desktop',
       po_dir: '../po',
      install: true,
  install_dir: get_option('datadir') / 'applications'
)

desktop_utils = find_program('desktop-file-validate', required: false)
if desktop_utils.found()
  test('Validate desktop file', desktop_utils, args: [desktop_file])
endif

appstream_file = i18n.merge_file(
        input: 'ru.mark.vrxx.metainfo.xml.in',
       output: 'ru.mark.vrxx.metainfo.xml',
       po_dir: '../po',
      install: true,
  install_dir: get_option('datadir') / 'metainfo'
)

appstreamcli = find_program('appstreamcli', required: false, disabler: true)
test('Validate appstream file', appstreamcli,
     args: ['validate', '--no-net', '--explain', appstream_file])

install_data('ru.mark.vrxx.gschema.xml',
  install_dir: get_option('datadir') / 'glib-2.0' / 'schemas'
)

compile_schemas = find_program('glib-compile-schemas', required: false, disabler: true)
test('Validate schema file',
     compile_schemas,
     args: ['--strict', '--dry-run', meson.current_source_dir()])


service_conf = configuration_data()
service_conf.set('bindir', get_option('prefix') / get_option('bindir'))
configure_file(
  input: 'ru.mark.vrxx.service.in',
  output: 'ru.mark.vrxx.service',
  configuration: service_conf,
  install_dir: get_option('datadir') / 'dbus-1' / 'services'
)

install_data('ru.mark.vrxx.daemon.conf',
  install_dir: get_option('datadir') / 'dbus-1' / 'system.d'
)

configure_file(
  input: 'ru.mark.vrxx.daemon.service.in',
  output: 'ru.mark.vrxx.daemon.service',
  configuration: service_conf,
  install_dir: get_option('datadir') / 'dbus-1' / 'system-services'
)

install_data('ru.mark.vrxx.policy',
  install_dir: get_option('datadir') / 'polkit-1' / 'actions'
)

subdir('icons')
