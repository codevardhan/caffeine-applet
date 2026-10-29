use std::borrow::Cow;
use std::os::fd::OwnedFd;
use std::time::{Duration, Instant};

use crate::config::{Settings, SettingsStore};
use crate::fl;
use cosmic::app::Core;
use cosmic::cosmic_theme::Spacing;
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Subscription, Task};
use cosmic::widget::{divider, mouse_area, settings, text, toggler, Column};
use cosmic::Element;
use zbus::blocking::connection::Builder as ConnectionBuilder;
use zbus::blocking::Connection;
use zbus::zvariant::OwnedFd as ZbusFd;

const ID: &str = "com.github.codevardhan.caffeine-applet";
const ON: &str = "com.github.codevardhan.caffeine-applet.On";
const OFF: &str = "com.github.codevardhan.caffeine-applet.Off";

/// How long a caffeine session should stay active for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaffeineDuration {
    Minutes(u32),
    Indefinite,
}

impl CaffeineDuration {
    /// Rust picks the unit; Fluent picks the plural form. Languages with more
    /// than English's two plural categories are handled by the `.ftl` alone.
    fn label(self) -> String {
        match self {
            Self::Minutes(minutes) if minutes % 60 == 0 => {
                let hours: u32 = minutes / 60;
                fl!("duration-hours", hours = hours)
            }
            Self::Minutes(minutes) => fl!("duration-minutes", minutes = minutes),
            Self::Indefinite => fl!("duration-indefinite"),
        }
    }

    fn duration(self) -> Option<Duration> {
        match self {
            Self::Minutes(minutes) => Some(Duration::from_secs(u64::from(minutes) * 60)),
            Self::Indefinite => None,
        }
    }
}

/// Round up partial minutes so an active timer never reads zero early.
fn countdown(deadline: Instant, now: Instant) -> String {
    let remaining = deadline.saturating_duration_since(now);
    let seconds = remaining.as_secs();
    let minutes =
        seconds / 60 + u64::from(!seconds.is_multiple_of(60) || remaining.subsec_nanos() != 0);
    fl!("countdown-minutes", minutes = minutes)
}

/// Inline diagnostic text, padded to line up with the rows above it.
fn error_row<'a>(message: impl Into<Cow<'a, str>> + 'a) -> Element<'a, Message> {
    cosmic::applet::padded_control(text::body(message)).into()
}

#[derive(Default)]
pub struct CaffeineApplet {
    core: Core,
    config: Option<SettingsStore>,
    settings: Settings,
    config_error: Option<String>,
    popup: Option<Id>,
    inhibit_fd: Option<OwnedFd>,
    screensaver_inhibit: Option<ScreenSaverInhibit>,
    inhibit_error: Option<String>,
    lid_inhibit_fd: Option<OwnedFd>,
    lid_inhibit_error: Option<String>,
    active_choice: Option<CaffeineDuration>,
    deadline: Option<Instant>,
}

#[derive(Clone, Debug)]
pub enum Message {
    PopupClosed(Id),
    TogglePopup,
    Enable(CaffeineDuration),
    Disable,
    ToggleInhibit,
    ToggleLidInhibit(bool),
    Tick,
}

/// Ask logind for an idle+sleep inhibit lock.
/// Returns an `OwnedFd` — the inhibit stays active as long as this fd is open.
fn acquire_inhibit() -> Result<OwnedFd, Box<dyn std::error::Error>> {
    let conn = Connection::system()?;
    let reply: ZbusFd = conn
        .call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "Inhibit",
            &(
                "idle:sleep",
                "Caffeine Applet",
                "Caffeine session active",
                "block",
            ),
        )?
        .body()
        .deserialize()?;

    Ok(reply.into())
}

/// Prevent logind from suspending the system or reacting to a lid close.
fn acquire_lid_inhibit() -> Result<OwnedFd, Box<dyn std::error::Error>> {
    let conn = Connection::system()?;
    let reply: ZbusFd = conn
        .call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "Inhibit",
            &(
                "sleep:handle-lid-switch",
                "Caffeine Applet",
                "Keep awake with lid closed",
                "block",
            ),
        )?
        .body()
        .deserialize()?;

    Ok(reply.into())
}

const SCREENSAVER_NAME: &str = "org.freedesktop.ScreenSaver";
/// cosmic-idle serves the interface at both paths; other desktops serve one.
const SCREENSAVER_PATHS: [&str; 2] = ["/org/freedesktop/ScreenSaver", "/ScreenSaver"];
/// zbus defaults to *no* method timeout, and these calls run on the UI thread,
/// so a wedged screensaver service would otherwise hang the whole panel.
const SCREENSAVER_TIMEOUT: Duration = Duration::from_secs(5);

/// A live `org.freedesktop.ScreenSaver` inhibit.
///
/// Owns its bus connection on purpose: cosmic-idle watches `NameOwnerChanged`
/// and discards every inhibitor belonging to a unique bus name that disappears,
/// so closing this connection releases the inhibit even if `UnInhibit` fails or
/// we are killed before `Drop` runs. Deliberately not `Clone` — a clone's drop
/// would retire a cookie the original still believes it holds.
#[derive(Debug)]
struct ScreenSaverInhibit {
    conn: Connection,
    cookie: u32,
    /// Whichever of `SCREENSAVER_PATHS` answered, so `UnInhibit` goes back to
    /// the same object.
    path: &'static str,
}

impl Drop for ScreenSaverInhibit {
    fn drop(&mut self) {
        // Best effort: `conn` is dropped immediately after this returns and the
        // resulting disconnect releases the inhibit anyway.
        if let Err(err) = self.conn.call_method(
            Some(SCREENSAVER_NAME),
            self.path,
            Some(SCREENSAVER_NAME),
            "UnInhibit",
            &(self.cookie,),
        ) {
            eprintln!(
                "Failed to release the screensaver inhibit (closing the bus connection instead): {err}"
            );
        }
    }
}

/// Ask the session's screensaver service to suppress blanking, locking and sleep.
///
/// On COSMIC this is the call that actually does the work: cosmic-idle knows
/// nothing about logind inhibitors, but one screensaver inhibit makes it tear
/// down the idle timers behind screen-off, auto-lock and auto-suspend alike.
fn acquire_screensaver_inhibit() -> Result<ScreenSaverInhibit, Box<dyn std::error::Error>> {
    let conn = ConnectionBuilder::session()?
        .method_timeout(SCREENSAVER_TIMEOUT)
        .build()?;

    let mut last_err = None;
    for path in SCREENSAVER_PATHS {
        match conn.call_method(
            Some(SCREENSAVER_NAME),
            path,
            Some(SCREENSAVER_NAME),
            "Inhibit",
            &("Caffeine Applet", "Caffeine session active"),
        ) {
            Ok(reply) => {
                let cookie: u32 = reply.body().deserialize()?;
                return Ok(ScreenSaverInhibit { conn, cookie, path });
            }
            Err(err) => last_err = Some(err),
        }
    }
    Err(last_err.expect("SCREENSAVER_PATHS is never empty").into())
}

impl cosmic::Application for CaffeineApplet {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(
        core: Core,
        _flags: Self::Flags,
    ) -> (Self, cosmic::Task<cosmic::Action<Self::Message>>) {
        let mut window = CaffeineApplet {
            core,
            ..Default::default()
        };
        match SettingsStore::new(ID, 1) {
            Ok(config) => {
                window.config = Some(config);
                window.reload_settings();
            }
            Err(err) => {
                window.config_error = Some(fl!("error-open-settings", error = err));
            }
        }
        (window, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Self::Message) -> cosmic::Task<cosmic::Action<Self::Message>> {
        match message {
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::TogglePopup => {
                if self.popup.is_none() {
                    self.reload_settings();
                }
                return if let Some(id) = self.popup.take() {
                    destroy_popup(id)
                } else {
                    let new_id = Id::unique();
                    self.popup = Some(new_id);
                    get_popup(self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    ))
                };
            }
            Message::Enable(choice) => {
                if !self.enable(choice) {
                    return Task::none();
                }
                if let Some(id) = self.popup.take() {
                    return destroy_popup(id);
                }
            }
            Message::Disable => {
                self.disable();
                if let Some(id) = self.popup.take() {
                    return destroy_popup(id);
                }
            }
            Message::ToggleInhibit => {
                if self.is_active() {
                    self.disable();
                } else {
                    self.reload_settings();
                    self.enable(self.default_choice());
                }
            }
            Message::ToggleLidInhibit(enabled) => {
                if enabled {
                    if self.settings.dangerous_enable_closed_lid_control
                        && self.lid_inhibit_fd.is_none()
                    {
                        self.finish_lid_inhibit(acquire_lid_inhibit());
                    }
                } else {
                    self.lid_inhibit_fd = None;
                    self.lid_inhibit_error = None;
                }
            }
            Message::Tick => {
                if let Some(deadline) = self.deadline {
                    if Instant::now() >= deadline {
                        self.disable();
                    }
                }
            }
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        if self.deadline.is_some() {
            cosmic::iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick)
        } else {
            Subscription::none()
        }
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let icon = if self.is_active() || self.lid_inhibit_fd.is_some() {
            ON
        } else {
            OFF
        };
        // `icon_button` gives us the exact same geometry, symbolic recolor and
        // `Button::AppletIcon` styling the official applets use. The button only
        // captures left-click, so right-click falls through to the `mouse_area`.
        mouse_area(
            self.core
                .applet
                .icon_button(icon)
                .on_press(Message::ToggleInhibit),
        )
        .on_right_press(Message::TogglePopup)
        .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        self.core.applet.popup_container(self.menu_content()).into()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

impl CaffeineApplet {
    fn reload_settings(&mut self) {
        let Some(config) = &self.config else { return };
        match Settings::load(config) {
            Ok(settings) => {
                if !settings.dangerous_enable_closed_lid_control {
                    self.lid_inhibit_fd = None;
                    self.lid_inhibit_error = None;
                }
                self.settings = settings;
                self.config_error = None;
            }
            Err(err) => {
                self.config_error = Some(fl!("error-load-settings", error = err));
            }
        }
    }

    fn default_choice(&self) -> CaffeineDuration {
        self.settings
            .default_duration_minutes
            .map_or(CaffeineDuration::Indefinite, CaffeineDuration::Minutes)
    }

    fn choices(&self) -> Vec<CaffeineDuration> {
        let mut choices: Vec<_> = self
            .settings
            .duration_minutes
            .iter()
            .copied()
            .map(CaffeineDuration::Minutes)
            .collect();
        // Editing presets must not hide a session that is already running.
        if let Some(choice @ CaffeineDuration::Minutes(_)) = self.active_choice {
            if !choices.contains(&choice) {
                choices.insert(0, choice);
            }
        }
        choices.push(CaffeineDuration::Indefinite);
        choices
    }

    fn choice_label(&self, choice: CaffeineDuration, now: Instant) -> String {
        if self.active_choice != Some(choice) {
            return choice.label();
        }
        match self.deadline {
            Some(deadline) => fl!(
                "choice-active-timed",
                label = choice.label(),
                remaining = countdown(deadline, now)
            ),
            None => fl!("choice-active-indefinite", label = choice.label()),
        }
    }

    fn finish_lid_inhibit(&mut self, result: Result<OwnedFd, Box<dyn std::error::Error>>) {
        match result {
            Ok(fd) => {
                self.lid_inhibit_fd = Some(fd);
                self.lid_inhibit_error = None;
            }
            Err(err) => {
                let error = err.to_string();
                eprintln!("Failed to keep the system awake with lid closed: {error}");
                self.lid_inhibit_error = Some(error);
            }
        }
    }

    /// True while a Caffeine session holds at least one of its main inhibitors.
    /// The two are acquired and released together, so in practice they agree;
    /// the `||` keeps the UI honest if that ever stops being true.
    fn is_active(&self) -> bool {
        self.inhibit_fd.is_some() || self.screensaver_inhibit.is_some()
    }

    fn finish_screensaver_inhibit(
        &mut self,
        result: Result<ScreenSaverInhibit, Box<dyn std::error::Error>>,
    ) {
        match result {
            Ok(inhibit) => {
                self.screensaver_inhibit = Some(inhibit);
                self.inhibit_error = None;
            }
            Err(err) => {
                eprintln!("Failed to inhibit the screensaver (is cosmic-idle running?): {err}");
                self.inhibit_error = Some(fl!("error-screensaver", error = err.to_string()));
            }
        }
    }

    /// Acquire the inhibit locks (if not already held) and set the session.
    /// Returns `false` if the session could not be started.
    ///
    /// Two mechanisms, acquired and released together:
    ///   * logind `idle:sleep` — portable, blocks suspend requested from
    ///     elsewhere, and is the only thing that works off COSMIC. Fatal.
    ///   * `org.freedesktop.ScreenSaver` — the only thing cosmic-idle listens
    ///     to, so it is what actually stops blanking, locking and suspending on
    ///     COSMIC. Optional: other desktops may not offer it and a logind-only
    ///     session is still worth having, so a failure degrades with a warning
    ///     rather than aborting.
    fn enable(&mut self, choice: CaffeineDuration) -> bool {
        if !self.is_active() {
            match acquire_inhibit() {
                Ok(fd) => {
                    self.inhibit_fd = Some(fd);
                    self.inhibit_error = None;
                }
                Err(err) => {
                    eprintln!("Failed to acquire inhibit lock (is logind/elogind running?): {err}");
                    self.inhibit_error = Some(fl!("error-start-session", error = err.to_string()));
                    return false;
                }
            }
            self.finish_screensaver_inhibit(acquire_screensaver_inhibit());
        }
        self.active_choice = Some(choice);
        self.deadline = choice.duration().map(|d| Instant::now() + d);
        true
    }

    /// Drop both inhibitors. `ScreenSaverInhibit::drop` sends `UnInhibit`;
    /// closing its connection is the backstop if that call fails.
    fn disable(&mut self) {
        self.inhibit_fd = None;
        self.screensaver_inhibit = None;
        self.inhibit_error = None;
        self.active_choice = None;
        self.deadline = None;
    }

    fn menu_content(&self) -> Element<'_, Message> {
        let Spacing {
            space_none,
            space_xxs,
            space_s,
            ..
        } = cosmic::theme::spacing();

        // Each inner `Vec` is one logical group. Empty groups are dropped below,
        // then survivors are joined with one divider between each adjacent pair.
        // That makes a leading, trailing or doubled divider structurally
        // impossible, whichever conditional sections happen to be present.
        let mut sections: Vec<Vec<Element<'_, Message>>> = Vec::new();

        // 1. Duration presets: a radio-like group. Always non-empty, because
        //    `choices()` always appends `Indefinitely`.
        let now = Instant::now();
        sections.push(
            self.choices()
                .into_iter()
                .map(|choice| {
                    let selected = self.active_choice == Some(choice);
                    cosmic::applet::menu_button(text::body(self.choice_label(choice, now)))
                        // `ListItem` now carries its own corner radii; `[0.0; 4]`
                        // is what it hardcoded before, and matches `AppletMenu`'s
                        // `radius_0` so the selected row keeps the same geometry.
                        .class(if selected {
                            cosmic::theme::Button::ListItem([0.0; 4])
                        } else {
                            cosmic::theme::Button::AppletMenu
                        })
                        .selected(selected)
                        .on_press(Message::Enable(choice))
                        .into()
                })
                .collect(),
        );

        // 2. Terminal action. Identical `menu_button` geometry to the durations
        //    above, so without a rule it reads as one more duration option.
        if self.is_active() {
            sections.push(vec![cosmic::applet::menu_button(text::body(fl!(
                "end-session"
            )))
            .on_press(Message::Disable)
            .into()]);
        }

        // 3. A persistent setting rather than a one-shot action, and a
        //    `padded_control` with no hover affordance. The rule signals that
        //    this row does not behave like the buttons above it.
        if self.settings.dangerous_enable_closed_lid_control {
            sections.push(vec![cosmic::applet::padded_control(settings::item(
                fl!("lid-control"),
                cosmic::widget::container(
                    toggler(self.lid_inhibit_fd.is_some()).on_toggle(Message::ToggleLidInhibit),
                ),
            ))
            .into()]);
        }

        // 4. Diagnostics, kept as one section so two simultaneous errors are not
        //    split by a rule. Pushed unconditionally; the filter below drops it
        //    when empty, so the guards are never duplicated.
        let mut errors: Vec<Element<'_, Message>> = Vec::new();
        if let Some(error) = &self.inhibit_error {
            errors.push(error_row(error.as_str()));
        }
        if self.settings.dangerous_enable_closed_lid_control {
            if let Some(error) = &self.lid_inhibit_error {
                errors.push(error_row(fl!("error-lid-control", error = error.as_str())));
            }
        }
        if let Some(error) = &self.config_error {
            errors.push(error_row(error.as_str()));
        }
        sections.push(errors);

        // `menu_button`/`padded_control` already carry the horizontal menu
        // padding, so the column only needs the vertical inset.
        let mut list = Column::new().padding([space_xxs, space_none]);
        // `.filter()` MUST precede `.enumerate()` so the index counts
        // *surviving* sections; reversed, an empty section would take index 0
        // and let a leading divider through.
        for (index, section) in sections.into_iter().filter(|s| !s.is_empty()).enumerate() {
            if index > 0 {
                list = list.push(
                    cosmic::applet::padded_control(divider::horizontal::default())
                        .padding([space_xxs, space_s]),
                );
            }
            list = list.extend(section);
        }
        list.into()
    }
}

// Tests live in tests/unit/window.rs.
// #[path] lets Rust load them from there while keeping access to the
// private functions/fields above via `use super::*`.
#[cfg(test)]
#[path = "../tests/unit/window.rs"]
mod tests;
