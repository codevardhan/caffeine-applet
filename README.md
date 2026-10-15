# Caffeine Applet for [COSMIC DE](https://system76.com/cosmic/)

A third-party applet for the [COSMIC desktop](https://system76.com/cosmic/) that prevents your system from going idle. Not official COSMIC software and not endorsed by System76. Left-click the coffee icon to toggle an inhibit lock that blocks idle and sleep, keeping your screen on and your machine awake. Right-click for a menu to inhibit for a set time.

Uses the logind D-Bus `Inhibit` interface directly — no child processes, no PID files, crash-safe by design. Works on any system running systemd-logind or elogind.

## Features

- **Left-click toggle**: Left-click the panel icon to start the configured default idle/sleep session, or end the current session. The default is indefinite. The closed-lid switch is independent.
- **Optional closed-lid control**: Opt in through the settings file to reveal a **Keep awake with lid closed** switch. It starts off on each launch and is independent of timed sessions. Acquisition errors appear in the popup.
- **Timed sessions**: Right-click to select a duration or run indefinitely. The selected row is highlighted and shows a live countdown, for example **15 minutes — 13 min left**. Duration presets are configurable.
- **Status icon**: The panel icon reflects the current state, a steaming cup when active and an empty cup when idle.
- **Auto-expiry**: Timed sessions release the inhibit lock on their own when time's up — no need to remember to turn it off.
- **Crash-safe**: Uses a file descriptor–based inhibit lock. If the applet crashes, the OS automatically releases the lock.
- **Minimal**: No background processes, no polling, near-zero resource usage.
- **Distro-agnostic**: Works with systemd-logind and elogind.

## Installation

### Flatpak (recommended)

Available via System76's official COSMIC Flatpak repository (not Flathub — this
applet doesn't have a standalone window/use outside a panel, which is outside
Flathub's scope).

```sh
flatpak remote-add --if-not-exists --user cosmic https://apt.pop-os.org/cosmic/cosmic.flatpakrepo
flatpak install --user cosmic com.github.codevardhan.caffeine-applet
```

If you have COSMIC Store installed, it should also be visible there under the
**COSMIC Applets** category, provided you have *both* the `flathub` and
`cosmic` remotes added at **user** scope. System-scope remotes currently
don't surface applets correctly in COSMIC Store
([pop-os/cosmic-store#477](https://github.com/pop-os/cosmic-store/issues/477)),
so if it isn't showing up, add both as `--user` remotes and it should appear.

### From source

A [justfile](./justfile) is included for the [casey/just](https://github.com/casey/just) command runner.

```sh
git clone https://github.com/codevardhan/caffeine-applet.git
cd caffeine-applet
just build-release
sudo just install
```

Then add the applet to a panel in **Settings → Desktop → Panel → Configure panel applets**.

### Uninstall

```sh
# Flatpak
flatpak uninstall --user com.github.codevardhan.caffeine-applet

# Native
sudo just uninstall
# or manually:
sudo rm /usr/bin/caffeine-applet
sudo rm /usr/share/applications/com.github.codevardhan.caffeine-applet.desktop
sudo rm /usr/share/icons/hicolor/scalable/apps/com.github.codevardhan.caffeine-applet*.svg
sudo rm /usr/share/metainfo/com.github.codevardhan.caffeine-applet.metainfo.xml
```

Log out and back in for COSMIC settings to update.

## Packaging

If packaging for a Linux distribution, vendor dependencies locally and build with vendored sources:

```sh
just vendor
just build-vendored
just rootdir=debian/caffeine-applet prefix=/usr install
```

## Translators

[Fluent](https://projectfluent.org/) is used for localization. Translation files are in the [i18n directory](./i18n). New translations may copy the [English (en) localization](./i18n/en), rename `en` to the desired [ISO 639-1 language code](https://en.wikipedia.org/wiki/List_of_ISO_639-1_codes), and provide translations for each message identifier.

## Contributing

Contributions are welcome! Feel free to open an issue or submit a pull request. For major changes, please open an issue first to discuss what you would like to change.

## Settings

On first launch the applet creates a RON settings file at:

```text
~/.config/cosmic/com.github.codevardhan.caffeine-applet/v1/settings
```

It respects `XDG_CONFIG_HOME`. For Flatpak, the file is normally under
`~/.var/app/com.github.codevardhan.caffeine-applet/config/cosmic/com.github.codevardhan.caffeine-applet/v1/settings`.

```ron
(
    duration_minutes: [15, 30, 60],
    default_duration_minutes: None,
    dangerous_enable_closed_lid_control: false,
)
```

Edit `duration_minutes` to choose your presets, for example `[10, 25, 50, 90]`.
Use one to eight unique, positive whole-minute values, in the desired menu
order. **Indefinitely** is always available. Reopen the popup to reload file
changes; existing timers continue with their original deadline. If you remove
a running duration from the presets, its countdown stays visible until that
session ends. Invalid files produce a popup error and keep the last valid
settings (or defaults at startup); the applet does not overwrite them.

`default_duration_minutes` selects the session started by left-clicking the
panel icon. `None` preserves indefinite sessions; `Some(25)` starts a 25-minute
session. Use positive whole minutes. It may be a duration outside the preset
list; its active countdown will still appear. Explicit menu choices always
use the duration you select. Settings are also reloaded before starting a
session with left-click.

`dangerous_enable_closed_lid_control` gates availability, not activation.
It defaults to `false`, hiding the lid switch. Setting it to `true` reveals the
switch but never starts a lid inhibitor automatically. The switch is a session
control and is not saved; every applet launch starts it off. Setting the config
flag back to `false` releases any active lid inhibitor on the next settings
reload. Opt in deliberately for your hardware and cooling setup.

## Closed-lid behavior

When the optional switch is on, it uses the same
`org.freedesktop.login1.Manager.Inhibit` interface as ordinary Caffeine
sessions, with `sleep:handle-lid-switch` in block mode. The low-level lid
inhibitor matters because logind ignores ordinary sleep inhibitors for lid
events by default. This works with systemd-logind or elogind; the service's
authorization policy still applies. Other power managers that handle lid
events themselves may require their own configuration. The switch reports
whether its inhibitor was acquired; hardware suspend behavior still needs to
be verified. The display may still blank when only this switch is enabled.

See [systemd's inhibitor documentation](https://systemd.io/INHIBITOR_LOCKS/).

## License

[MIT](./LICENSE)
