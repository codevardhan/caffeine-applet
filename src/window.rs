use std::{fs, path::PathBuf, process::Command};

// Mandatory COSMIC imports
use cosmic::app::Core;
use cosmic::iced::Task;
use cosmic::Element;

use cosmic::widget::{button, svg};

const ID: &str = "com.example.CaffeineApplet";

static COFFEE_EMPTY: &[u8] = include_bytes!("../assets/coffee-empty.svg");
static COFFEE_FULL: &[u8] = include_bytes!("../assets/coffee-full.svg");

#[derive(Default)]
pub struct CaffeineApplet {
    core: Core,
    is_enabled: bool,
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

    println!("☕ Caffeine session enabled");
    Ok(())
}

fn disable_caffeine() -> Result<(), String> {
    if let Some(id) = get_id() {
        Command::new("kill")
            .arg(&id)
            .spawn()
            .map_err(|e| e.to_string())?;

        fs::remove_file(get_id_path()).map_err(|e| e.to_string())?;

        println!("😴 Caffeine session disabled");
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

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<cosmic::app::Message<Self::Message>>) {
        // If a PID file exists, that means caffeine is already running
        let is_enabled = get_id().is_some();

        let window = CaffeineApplet { core, is_enabled };
        (window, Task::none())
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::app::Message<Self::Message>> {
        match message {
            Message::ToggleCaffeine => {
                let desired_state = !self.is_enabled;
                if let Err(err) = do_caffeine(desired_state) {
                    println!("Error toggling caffeine: {}", err);
                } else {
                    // Only update self.is_enabled if success
                    self.is_enabled = desired_state;
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<Self::Message> {
        let raw_svg = if self.is_enabled {
            COFFEE_FULL
        } else {
            COFFEE_EMPTY
        };

        let handle = cosmic::widget::svg::Handle::from_memory(raw_svg.to_vec());
        let icon = svg(handle).width(24).height(24);

        button::custom(icon)
            .on_press(Message::ToggleCaffeine)
            .into()
    }
}
