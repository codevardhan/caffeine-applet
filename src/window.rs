use std::{fs, path::PathBuf, process::Command};

// Mandatory COSMIC imports
use cosmic::app::Core;
use cosmic::iced::Task;
use cosmic::Element;

const ID: &str = "com.github.codevardhan.caffeine-applet";
const ON: &str = "com.github.codevardhan.caffeine-applet.On";
const OFF: &str = "com.github.codevardhan.caffeine-applet.Off";

#[derive(Default)]
pub struct CaffeineApplet {
    core: Core,
    enabled: bool,
}

#[derive(Clone, Debug)]
pub enum Message {
    ToggleCaffeine,
}

fn get_id_path() -> PathBuf {
    PathBuf::from("/tmp/caffeine-id.txt")
}

fn get_id() -> Option<String> {
    match fs::read_to_string(get_id_path()) {
        Ok(cookie) => Some(cookie),
        Err(_) => None,
    }
}

fn enable_caffeine() -> Result<(), String> {
    if let Some(_) = get_id() {
        return Err("Caffeine is currently enabled".to_string());
    }

    let child = Command::new("systemd-inhibit")
        .arg("--what=idle:sleep")
        .arg("--why=Caffeine session active")
        .arg("--mode=block")
        .arg("sleep")
        .arg("infinity")
        .spawn()
        .map_err(|e| e.to_string())?;

    let process_id = child.id();
    fs::write(get_id_path(), process_id.to_string()).map_err(|e| e.to_string())?;

    // println!("☕ Caffeine session enabled");
    Ok(())
}

fn disable_caffeine() -> Result<(), String> {
    if let Some(id) = get_id() {
        Command::new("kill")
            .arg(&id)
            .spawn()
            .map_err(|e| e.to_string())?;

        fs::remove_file(get_id_path()).map_err(|e| e.to_string())?;

        // println!("😴 Caffeine session disabled");
        Ok(())
    } else {
        Err("There's no caffeine session enabled".to_string())
    }
}

fn do_caffeine(is_active: bool) -> Result<(), String> {
    if is_active {
        enable_caffeine()
    } else {
        disable_caffeine()
    }
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
        // If a PID file exists, that means caffeine is already running
        let enabled = get_id().is_some();

        let window = CaffeineApplet { core, enabled };
        (window, Task::none())
    }

    fn update(&mut self, message: Self::Message) -> cosmic::Task<cosmic::Action<Self::Message>> {
        match message {
            Message::ToggleCaffeine => {
                let desired_state = !self.enabled;
                if let Err(err) = do_caffeine(desired_state) {
                    println!("Error toggling caffeine: {}", err);
                } else {
                    // Only update self.enabled if success
                    self.enabled = desired_state;
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<Self::Message> {
        self.core
            .applet
            .icon_button(if self.enabled { ON } else { OFF })
            .on_press_down(Message::ToggleCaffeine)
            .into()
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}
