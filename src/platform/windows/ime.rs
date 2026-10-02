//! Small input-routing state. Native Edit/Windows still own text, conversion and candidate UI.
#[derive(Clone, Copy, Debug)]
pub struct State {
    composing: bool,
    candidates: u32,
    claimed_keys: u8,
}
impl State {
    pub const EMPTY: Self = Self {
        composing: false,
        candidates: 0,
        claimed_keys: 0,
    };
    pub fn active(self) -> bool {
        self.composing || self.candidates != 0
    }
    pub fn start(&mut self) {
        self.composing = true;
    }
    pub fn end(&mut self) {
        self.composing = false;
    }
    pub fn candidates_open(&mut self, mask: u32) {
        self.candidates |= mask;
    }
    pub fn candidates_close(&mut self, mask: u32) {
        self.candidates &= !mask;
    }
    pub fn claim_key(&mut self, key: u32) {
        self.claimed_keys |= key_bit(key);
    }
    pub fn key_up(&mut self, key: u32) {
        self.claimed_keys &= !key_bit(key);
    }
    pub fn claimed(self, key: u32) -> bool {
        self.claimed_keys & key_bit(key) != 0
    }
    pub fn key_down(&mut self, key: u32, native_active: bool) {
        if self.active() || native_active {
            self.claim_key(key);
        }
    }
}
fn key_bit(key: u32) -> u8 {
    match key {
        0x0d => 1,  // Enter
        0x1b => 2,  // Escape
        0x26 => 4,  // Up
        0x28 => 8,  // Down
        0x74 => 16, // F5
        0x51 => 32, // Q (Ctrl+Q is interpreted by the window)
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confirm_key_remains_owned_until_release_even_after_composition_ends() {
        let mut state = State::EMPTY;
        state.start();
        state.key_down(0x0d, false); // Before TranslateMessage.
        state.end(); // IME may end before the edit sees the same key or its auto-repeat.
        assert!(!state.active());
        assert!(state.claimed(0x0d));
        state.key_up(0x0d);
        assert!(!state.claimed(0x0d));
    }
    #[test]
    fn space_or_mouse_commit_does_not_require_an_extra_enter() {
        let mut state = State::EMPTY;
        state.start();
        state.key_down(0x20, false);
        state.end();
        assert!(!state.claimed(0x0d));
        state.claim_key(0x0d); // WM_IME_KEYDOWN may arrive without a start notification.
        assert!(state.claimed(0x0d));
    }
    #[test]
    fn candidate_lists_and_navigation_are_independent_of_composition() {
        let mut state = State::EMPTY;
        state.candidates_open(3);
        state.key_down(0x28, false);
        state.candidates_close(1);
        assert!(state.active());
        state.candidates_close(2);
        assert!(!state.active());
        assert!(state.claimed(0x28));
        state.key_up(0x28);
        assert!(!state.claimed(0x28));
        state.key_down(0x1b, true); // IMM fallback when notifications are absent.
        assert!(state.claimed(0x1b));
    }
}
