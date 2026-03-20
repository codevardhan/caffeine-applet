use std::os::fd::OwnedFd;

use cosmic::app::Core;
use cosmic::iced::Task;
use cosmic::Element;
use zbus::blocking::Connection;
use zbus::zvariant::OwnedFd as ZbusFd;

const ID: &str = "com.github.codevardhan.caffeine-applet";
const ON: &str = "com.github.codevardhan.caffeine-applet.On";
const OFF: &str = "com.github.codevardhan.caffeine-applet.Off";

pub struct CaffeineApplet {
    core: Core,
    inhibit_fd: Option<OwnedFd>,
}

impl Default for CaffeineApplet {
    fn default() -> Self {
        Self {
            core: Core::default(),
            inhibit_fd: None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    ToggleCaffeine,
}

/// Ask logind for an idle+sleep inhibit lock.
/// Returns an OwnedFd — the inhibit stays active as long as this fd is open.
fn acquire_inhibit() -> Result<OwnedFd, Box<dyn std::error::Error>> {
    let conn = Connection::system()?;
    let reply: ZbusFd = conn.call_method(
        Some("org.freedesktop.login1"),
        "/org/freedesktop/login1",
        Some("org.freedesktop.login1.Manager"),
        "Inhibit",
        &("idle:sleep", "Caffeine Applet", "Caffeine session active", "block"),
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
        let window = CaffeineApplet {
            core,
            inhibit_fd: None,
        };
        (window, Task::none())
    }

    fn update(&mut self, message: Self::Message) -> cosmic::Task<cosmic::Action<Self::Message>> {
        match message {
            Message::ToggleCaffeine => {
                if self.inhibit_fd.is_some() {
                    // Drop the fd → releases the inhibit lock
                    self.inhibit_fd = None;
                } else {
                    match acquire_inhibit() {
                        Ok(fd) => self.inhibit_fd = Some(fd),
                        Err(err) => eprintln!("Failed to acquire inhibit lock (is logind/elogind running?): {err}"),
                    }
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let icon = if self.inhibit_fd.is_some() { ON } else { OFF };
        self.core
            .applet
            .icon_button(icon)
            .on_press_down(Message::ToggleCaffeine)
            .into()
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}