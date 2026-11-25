use policy::browser::BROWSER;

#[test]
fn the_session_bus_is_among_what_never_reaches_it() {
  assert!(BROWSER.session_variables.iter().any(|name| name == "DBUS_SESSION_BUS_ADDRESS"));
  assert!(BROWSER.detached_arguments.iter().any(|arg| arg == "--password-store=basic"));
}
