# Caffeine Applet for [COSMIC DE](https://system76.com/cosmic/)

A simple COSMIC applet that prevents your system from going idle using a logind D-Bus inhibit lock. Perfect for keeping your machine awake on demand!

## Features

- **Toggle Caffeine:** Click the applet icon to open a popup with a toggler to enable or disable idle/sleep inhibition.
- **Lid Switch Prevention:** Optionally prevent sleep when closing the laptop lid.
- **Crash-safe:** Uses a logind file descriptor lock — if the applet crashes or is killed, the inhibit is automatically released by the kernel.
- **Distro-agnostic:** Works on any system running systemd-logind or elogind.
- **Minimal:** No child processes, no PID files, no temp files. Just a single D-Bus call.
- **Built with COSMIC:** Integrates into your COSMIC panel as a native applet.

## Installation

A [justfile](./justfile) is included by default for the [casey/just][just] command runner.

- `just` builds the application with the default `just build-release` recipe
- `just run` builds and runs the application
- `just install` installs the project into the system
- `just vendor` creates a vendored tarball
- `just build-vendored` compiles with vendored dependencies from that tarball
- `just check` runs clippy on the project to check for linter warnings
- `just check-json` can be used by IDEs that support LSP

### Flatpak (In Progress)

<!-- ```sh
flatpak install flathub com.github.codevardhan.caffeine-applet
``` -->

## Usage

Once the applet is added to your panel:

1. Click the coffee-cup icon to open the popup.
2. Toggle **Caffeine** on to prevent your system from going idle or sleeping.
3. Toggle **Prevent lid sleep** to also keep the system awake when closing the laptop lid.
4. Click the icon again to dismiss the popup.

A full coffee cup means caffeine is active; an empty cup means normal idle behavior.

## How It Works

The applet calls `org.freedesktop.login1.Manager.Inhibit` over D-Bus to acquire an inhibit lock. This returns a file descriptor — as long as it's held open, the system won't idle or sleep. Dropping the fd (toggling off, closing the applet, or a crash) releases the lock immediately.

## Translators

[Fluent][fluent] is used for localization of the software. Fluent's translation files are found in the [i18n directory](./i18n). New translations may copy the [English (en) localization](./i18n/en) of the project, rename `en` to the desired [ISO 639-1 language code][iso-codes], and then translations can be provided for each [message identifier][fluent-guide]. If no translation is necessary, the message may be omitted.

## Packaging

If packaging for a Linux distribution, vendor dependencies locally with the `vendor` rule, and build with the vendored sources using the `build-vendored` rule. When installing files, use the `rootdir` and `prefix` variables to change installation paths.

```sh
just vendor
just build-vendored
just rootdir=debian/caffeine-applet prefix=/usr install
```

It is recommended to build a source tarball with the vendored dependencies, which can typically be done by running `just vendor` on the host system before it enters the build environment.

## Developers

Developers should install [rustup][rustup] and configure their editor to use [rust-analyzer][rust-analyzer]. To improve compilation times, disable LTO in the release profile, install the [mold][mold] linker, and configure [sccache][sccache] for use with Rust. The [mold][mold] linker will only improve link times if LTO is disabled.

[fluent]: https://projectfluent.org/
[fluent-guide]: https://projectfluent.org/fluent/guide/hello.html
[iso-codes]: https://en.wikipedia.org/wiki/List_of_ISO_639-1_codes
[just]: https://github.com/casey/just
[rustup]: https://rustup.rs/
[rust-analyzer]: https://rust-analyzer.github.io/
[mold]: https://github.com/rui314/mold
[sccache]: https://github.com/mozilla/sccache

## Contributing

Contributions are welcome! Feel free to open an issue or submit a pull request on GitHub. For major changes, please open an issue first to discuss what you would like to change.