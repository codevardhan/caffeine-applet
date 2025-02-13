# Caffeine Applet for [COSMIC DE](https://system76.com/cosmic/)

A simple COSMIC applet that prevents your system from going idle by creating a systemd-inhibit lock session. Perfect for keeping your machine awake on demand!

## Features

- Toggle Caffeine: Click the applet's icon to enable or disable an inhibit session.
- Minimal: Only uses a tiny amount of memory and CPU.
- Built with COSMIC: Integrates into your cosmic desktop or panel.


## Installation

- Clone & Enter Project Directory:

```bash
git clone https://github.com/codevardhan/caffeine-applet.git
cd caffeine-applet
```

- Make Install Script Executable:

```bash
chmod +x install.sh
```

- Run Installation:

```bash
./install.sh
```

- Go to Desktop → Panel → Configure panel applets → Add Applet to add this to COSMIC panel.


## Usage

Once the applet is running:
- Click the coffee-cup icon to toggle between awake (caffeine mode) and idle (normal).
A PID file (/tmp/caffeine-id.txt) is maintained to track the systemd-inhibit session.
- If the PID file already exists, it means a caffeine session is running (and you won’t be able to start a new one without stopping it first).

## Contributing

Contributions are welcome! Feel free to open an issue or submit a pull request on GitHub. For major changes, please open an issue first to discuss what you would like to change.