use std::os::fd::OwnedFd;
use std::time::{Duration, Instant};

use crate::config::Settings;
use cosmic::app::Core;
use cosmic::cosmic_config::Config;
use cosmic::iced::window::Id;
use cosmic::iced::{Subscription, Task};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget::{list_column, mouse_area, settings, text, toggler};
use cosmic::Element;
use zbus::blocking::Connection;
use zbus::zvariant::OwnedFd as ZbusFd;

const ID: &str = "com.github.codevardhan.caffeine-applet";
const ON: &str = "com.github.codevardhan.caffeine-applet.On";
const OFF: &str = "com.github.codevardhan.caffeine-applet.Off";

/// How long a caffeine session should stay active for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaffeineDuration {
    Minutes15,
    Minutes30,
    Hour1,
    Indefinite,
}

impl CaffeineDuration {
    const ALL: [CaffeineDuration; 4] = [
        Self::Minutes15,
        Self::Minutes30,
        Self::Hour1,
        Self::Indefinite,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Minutes15 => "15 minutes",
            Self::Minutes30 => "30 minutes",
            Self::Hour1 => "1 hour",
            Self::Indefinite => "Indefinitely",
        }
    }

    /// `None` means the session never expires on its own.
    fn duration(self) -> Option<Duration> {
        match self {
            Self::Minutes15 => Some(Duration::from_secs(15 * 60)),
            Self::Minutes30 => Some(Duration::from_secs(30 * 60)),
            Self::Hour1 => Some(Duration::from_secs(60 * 60)),
            Self::Indefinite => None,
        }
    }
}

#[derive(Default)]
pub struct CaffeineApplet {
    core: Core,
    config: Option<Config>,
    settings: Settings,
    config_error: Option<String>,
    popup: Option<Id>,
    inhibit_fd: Option<OwnedFd>,
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
/// Returns an OwnedFd — the inhibit stays active as long as this fd is open.
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
        match Config::new(ID, 1) {
            Ok(config) => {
                window.config = Some(config);
                window.reload_settings();
            }
            Err(err) => window.config_error = Some(format!("Could not open settings: {err}")),
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
                    cosmic::task::message(cosmic::Action::Cosmic(cosmic::app::Action::Surface(
                        destroy_popup(id),
                    )))
                } else {
                    cosmic::task::message(cosmic::Action::Cosmic(cosmic::app::Action::Surface(
                        app_popup::<CaffeineApplet>(
                            move |state: &mut CaffeineApplet| {
                                let new_id = Id::unique();
                                state.popup = Some(new_id);
                                state.core.applet.get_popup_settings(
                                    state.core.main_window_id().unwrap(),
                                    new_id,
                                    None,
                                    None,
                                    None,
                                )
                            },
                            Some(Box::new(move |state: &CaffeineApplet| {
                                Element::from(
                                    state.core.applet.popup_container(state.menu_content()),
                                )
                                .map(cosmic::Action::App)
                            })),
                        ),
                    )))
                };
            }
            Message::Enable(choice) => {
                if !self.enable(choice) {
                    return Task::none();
                }
                if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Cosmic(
                        cosmic::app::Action::Surface(destroy_popup(id)),
                    ));
                }
            }
            Message::Disable => {
                self.disable();
                if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Cosmic(
                        cosmic::app::Action::Surface(destroy_popup(id)),
                    ));
                }
            }
            Message::ToggleInhibit => {
                if self.inhibit_fd.is_some() {
                    self.disable();
                } else {
                    self.enable(CaffeineDuration::Indefinite);
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
                        self.inhibit_fd = None;
                        self.active_choice = None;
                        self.deadline = None;
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
        let icon = if self.inhibit_fd.is_some() || self.lid_inhibit_fd.is_some() {
            ON
        } else {
            OFF
        };
        let suggested = self.core.applet.suggested_size(true);
        let padding = self.core.applet.suggested_padding(true);

        // Same icon element the applet's own `icon_button` builds, so the panel
        // renders it identically (symbolic recolor + suggested size).
        let icon_el = cosmic::widget::icon(
            cosmic::widget::icon::from_name(icon)
                .symbolic(true)
                .size(suggested.0)
                .into(),
        )
        .class(cosmic::theme::Svg::Custom(std::rc::Rc::new(|theme| {
            cosmic::iced_widget::svg::Style {
                color: Some(theme.cosmic().background.on.into()),
            }
        })))
        .width(cosmic::iced::Length::Fixed(suggested.0 as f32))
        .height(cosmic::iced::Length::Fixed(suggested.1 as f32));

        cosmic::widget::button::custom(
            cosmic::widget::layer_container(
                mouse_area(icon_el).on_right_press(Message::TogglePopup),
            )
            .center(cosmic::iced::Length::Fill),
        )
        .width(cosmic::iced::Length::Fixed(
            (suggested.0 + 2 * padding) as f32,
        ))
        .height(cosmic::iced::Length::Fixed(
            (suggested.1 + 2 * padding) as f32,
        ))
        .class(cosmic::theme::Button::AppletIcon)
        .on_press(Message::ToggleInhibit)
        .into()
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
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
            Err(err) => self.config_error = Some(format!("Could not load settings: {err}. Check the settings file; existing values are still in use.")),
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

    /// Acquire the inhibit lock (if not already held) and set the session.
    /// Returns `false` if the lock could not be acquired.
    fn enable(&mut self, choice: CaffeineDuration) -> bool {
        if self.inhibit_fd.is_none() {
            match acquire_inhibit() {
                Ok(fd) => self.inhibit_fd = Some(fd),
                Err(err) => {
                    eprintln!("Failed to acquire inhibit lock (is logind/elogind running?): {err}");
                    return false;
                }
            }
        }
        self.active_choice = Some(choice);
        self.deadline = choice.duration().map(|d| Instant::now() + d);
        true
    }

    /// Drop the fd, releasing the inhibit lock.
    fn disable(&mut self) {
        self.inhibit_fd = None;
        self.active_choice = None;
        self.deadline = None;
    }

    fn menu_content(&self) -> Element<'_, Message> {
        let mut list = list_column().padding(5).spacing(4);

        for choice in CaffeineDuration::ALL {
            let label = if self.active_choice == Some(choice) {
                format!("✓  {}", choice.label())
            } else {
                choice.label().to_string()
            };
            list = list
                .add(cosmic::applet::menu_button(text(label)).on_press(Message::Enable(choice)));
        }

        if self.inhibit_fd.is_some() {
            list = list.add(
                cosmic::applet::menu_button(text("End Caffeine session"))
                    .on_press(Message::Disable),
            );
        }

        if self.settings.dangerous_enable_closed_lid_control {
            list = list.add(settings::item(
                "Keep awake with lid closed",
                cosmic::widget::container(
                    toggler(self.lid_inhibit_fd.is_some()).on_toggle(Message::ToggleLidInhibit),
                ),
            ));
            if let Some(error) = &self.lid_inhibit_error {
                list = list.add(text(format!("Could not enable closed-lid mode: {error}")));
            }
        }
        if let Some(error) = &self.config_error {
            list = list.add(text(error));
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
