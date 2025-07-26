use gilrs::{Gilrs, Button, EventType, Axis};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use crate::webui::handlers::GamepadStatus;

lazy_static::lazy_static! {
    static ref GAMEPAD_STATE: Arc<Mutex<GamepadState>> = Arc::new(Mutex::new(GamepadState::default()));
}

#[derive(Default)]
struct GamepadState {
    connected: bool,
    name: Option<String>,
    buttons: Vec<bool>,
    axes: Vec<f32>,
}

pub fn get_gamepad_status() -> GamepadStatus {
    let state = GAMEPAD_STATE.lock().unwrap();
    GamepadStatus {
        connected: state.connected,
        name: state.name.clone(),
        buttons: state.buttons.clone(),
        axes: state.axes.clone(),
    }
}

pub fn start_gamepad_thread(tx: mpsc::Sender<GamepadEvent>) {
    std::thread::spawn(move || {
        let mut gilrs = match Gilrs::new() {
            Ok(g) => g,
            Err(e) => {
                eprintln!("Failed to initialize gamepad support: {}", e);
                return;
            }
        };
        
        loop {
            while let Some(event) = gilrs.next_event() {
                let gamepad = gilrs.gamepad(event.id);
                
                // Update state
                {
                    let mut state = GAMEPAD_STATE.lock().unwrap();
                    state.connected = gamepad.is_connected();
                    state.name = Some(gamepad.name().to_string());
                    
                    // Update button states
                    state.buttons = vec![
                        gamepad.is_pressed(Button::South),      // A/X
                        gamepad.is_pressed(Button::East),       // B/Circle
                        gamepad.is_pressed(Button::West),       // X/Square
                        gamepad.is_pressed(Button::North),      // Y/Triangle
                        gamepad.is_pressed(Button::LeftTrigger),
                        gamepad.is_pressed(Button::RightTrigger),
                        gamepad.is_pressed(Button::LeftTrigger2),
                        gamepad.is_pressed(Button::RightTrigger2),
                        gamepad.is_pressed(Button::Select),
                        gamepad.is_pressed(Button::Start),
                        gamepad.is_pressed(Button::Mode),
                        gamepad.is_pressed(Button::LeftThumb),
                        gamepad.is_pressed(Button::RightThumb),
                        gamepad.is_pressed(Button::DPadUp),
                        gamepad.is_pressed(Button::DPadDown),
                        gamepad.is_pressed(Button::DPadLeft),
                        gamepad.is_pressed(Button::DPadRight),
                    ];
                    
                    // Update axes
                    state.axes = vec![
                        gamepad.value(Axis::LeftStickX),
                        gamepad.value(Axis::LeftStickY),
                        gamepad.value(Axis::RightStickX),
                        gamepad.value(Axis::RightStickY),
                        gamepad.value(Axis::LeftZ),
                        gamepad.value(Axis::RightZ),
                    ];
                }
                
                // Convert to our event type
                let gamepad_event = match event.event {
                    EventType::ButtonPressed(button, _) => {
                        GamepadEvent::ButtonPressed(map_button(button))
                    }
                    EventType::ButtonReleased(button, _) => {
                        GamepadEvent::ButtonReleased(map_button(button))
                    }
                    EventType::AxisChanged(axis, value, _) => {
                        GamepadEvent::AxisChanged(map_axis(axis), value)
                    }
                    EventType::Connected => GamepadEvent::Connected,
                    EventType::Disconnected => GamepadEvent::Disconnected,
                    _ => continue,
                };
                
                // Send event through channel
                if let Err(_) = tx.blocking_send(gamepad_event) {
                    // Receiver dropped, exit thread
                    break;
                }
            }
            
            std::thread::sleep(std::time::Duration::from_millis(16)); // ~60fps
        }
    });
}

#[derive(Debug, Clone)]
pub enum GamepadEvent {
    ButtonPressed(GamepadButton),
    ButtonReleased(GamepadButton),
    AxisChanged(GamepadAxis, f32),
    Connected,
    Disconnected,
}

#[derive(Debug, Clone, Copy)]
pub enum GamepadButton {
    A, B, X, Y,
    LeftBumper, RightBumper,
    LeftTrigger, RightTrigger,
    Select, Start, Mode,
    LeftStick, RightStick,
    DPadUp, DPadDown, DPadLeft, DPadRight,
}

#[derive(Debug, Clone, Copy)]
pub enum GamepadAxis {
    LeftStickX, LeftStickY,
    RightStickX, RightStickY,
    LeftTrigger, RightTrigger,
}

fn map_button(button: Button) -> GamepadButton {
    match button {
        Button::South => GamepadButton::A,
        Button::East => GamepadButton::B,
        Button::West => GamepadButton::X,
        Button::North => GamepadButton::Y,
        Button::LeftTrigger => GamepadButton::LeftBumper,
        Button::RightTrigger => GamepadButton::RightBumper,
        Button::LeftTrigger2 => GamepadButton::LeftTrigger,
        Button::RightTrigger2 => GamepadButton::RightTrigger,
        Button::Select => GamepadButton::Select,
        Button::Start => GamepadButton::Start,
        Button::Mode => GamepadButton::Mode,
        Button::LeftThumb => GamepadButton::LeftStick,
        Button::RightThumb => GamepadButton::RightStick,
        Button::DPadUp => GamepadButton::DPadUp,
        Button::DPadDown => GamepadButton::DPadDown,
        Button::DPadLeft => GamepadButton::DPadLeft,
        Button::DPadRight => GamepadButton::DPadRight,
        _ => GamepadButton::A, // Default
    }
}

fn map_axis(axis: Axis) -> GamepadAxis {
    match axis {
        Axis::LeftStickX => GamepadAxis::LeftStickX,
        Axis::LeftStickY => GamepadAxis::LeftStickY,
        Axis::RightStickX => GamepadAxis::RightStickX,
        Axis::RightStickY => GamepadAxis::RightStickY,
        Axis::LeftZ => GamepadAxis::LeftTrigger,
        Axis::RightZ => GamepadAxis::RightTrigger,
        _ => GamepadAxis::LeftStickX, // Default
    }
}