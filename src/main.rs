mod window;

use crate::window::CaffeineApplet;

fn main() -> cosmic::iced::Result {
    cosmic::applet::run::<CaffeineApplet>(())?;

    Ok(())
}
