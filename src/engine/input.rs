#[repr(usize)]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum KeyCode {
    A = 0, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
    Space, Enter, Escape, Backspace, Tab,
    ShiftLeft, ShiftRight, ControlLeft, ControlRight, AltLeft, AltRight,
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight, MetaLeft, MetaRight,
    MAX
}

#[repr(usize)]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum MouseButton {
    Left = 0, Middle = 1, Right = 2, Back = 3, Forward = 4, MAX
}

pub struct Input {
    pub current_keys: [bool; KeyCode::MAX as usize],
    pub previous_keys: [bool; KeyCode::MAX as usize],

    pub current_mouse_buttons: [bool; MouseButton::MAX as usize],
    pub previous_mouse_buttons: [bool; MouseButton::MAX as usize],

    pub mouse_x: f32,
    pub mouse_y: f32,
    pub mouse_dx: f32,
    pub mouse_dy: f32,
    pub mouse_wheel_delta: f32,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            current_keys: [false; KeyCode::MAX as usize],
            previous_keys: [false; KeyCode::MAX as usize],
            current_mouse_buttons: [false; MouseButton::MAX as usize],
            previous_mouse_buttons: [false; MouseButton::MAX as usize],
            mouse_x: 0.0, mouse_y: 0.0, mouse_dx: 0.0, mouse_dy: 0.0, mouse_wheel_delta: 0.0,
        }
    }
}

impl Input {
    #[inline(always)] pub fn pressed(&self, key: KeyCode) -> bool { self.current_keys[key as usize] }
    #[inline(always)] pub fn just_pressed(&self, key: KeyCode) -> bool { self.current_keys[key as usize] && !self.previous_keys[key as usize] }
    #[inline(always)] pub fn just_released(&self, key: KeyCode) -> bool { !self.current_keys[key as usize] && self.previous_keys[key as usize] }

    #[inline(always)] pub fn mouse_pressed(&self, btn: MouseButton) -> bool { self.current_mouse_buttons[btn as usize] }
    #[inline(always)] pub fn mouse_just_pressed(&self, btn: MouseButton) -> bool { self.current_mouse_buttons[btn as usize] && !self.previous_mouse_buttons[btn as usize] }
    #[inline(always)] pub fn mouse_just_released(&self, btn: MouseButton) -> bool { !self.current_mouse_buttons[btn as usize] && self.previous_mouse_buttons[btn as usize] }
}
