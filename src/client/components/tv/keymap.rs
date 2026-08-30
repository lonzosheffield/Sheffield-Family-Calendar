//! The Fire TV remote's key map (PLAN v2 **D8**, R-11).
//!
//! The whole remote is seven keys:
//!
//! | Remote | `KeyboardEvent.key` | Meaning on the kiosk |
//! | --- | --- | --- |
//! | D-pad up / down | `ArrowUp` / `ArrowDown` | move the cursor inside the current zone (on the profile rail that *is* "switch profile") |
//! | D-pad left / right | `ArrowLeft` / `ArrowRight` | cycle panels |
//! | D-pad centre / Select | `Enter` | activate: enter a profile's list, toggle an item, open the QR |
//! | Back | `Backspace` | leave the list, close an overlay, return to the default panel |
//! | Play/Pause | `MediaPlayPause` | toggle the phone-join QR overlay |
//!
//! **There is no `Escape`.** R-11: Fire TV remotes have no Escape key, so the
//! v1 design's "Esc closes the overlay" was unreachable from the sofa.
//! [`TvKey::from_key`] deliberately refuses to map it and
//! `tests/tv_tests.rs` asserts that refusal, so nobody quietly adds it back.
//!
//! Fire OS's WebView does not agree with every other browser about what a
//! remote emits, which is exactly why `?keys=1` exists: the owner points the
//! real remote at the real television and reads the real `key`/`code` off
//! the screen (Appendix A, step A5). Every alias this module accepts beyond
//! the seven canonical names is listed in [`TvKey::from_key`] with the reason.

/// One key press the kiosk understands.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum TvKey {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Back,
    PlayPause,
}

/// Every key in the map, in the order PURPLE §P3 T2.1 (d) enumerates them.
pub const TV_KEYS: [TvKey; 7] = [
    TvKey::Up,
    TvKey::Down,
    TvKey::Left,
    TvKey::Right,
    TvKey::Enter,
    TvKey::Back,
    TvKey::PlayPause,
];

impl TvKey {
    /// Map a browser `KeyboardEvent.key` value onto a remote key.
    ///
    /// Returns `None` for anything the kiosk does not act on — including
    /// `Escape` (R-11) — so an unmapped press falls through to the `?keys=1`
    /// log instead of doing something surprising.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "ArrowUp" => Some(TvKey::Up),
            "ArrowDown" => Some(TvKey::Down),
            "ArrowLeft" => Some(TvKey::Left),
            "ArrowRight" => Some(TvKey::Right),
            // Fire OS's WebView reports the D-pad centre as `Enter`; some
            // remotes and every USB keyboard also emit ` ` (Space) for the
            // default action of a focused control.
            "Enter" | " " | "Spacebar" => Some(TvKey::Enter),
            // `Backspace` is what Fire OS's WebView delivers for the remote's
            // Back button; `BrowserBack` and `GoBack` are the two names the
            // UI Events spec gives the same physical button, and Silk
            // (Branch B) uses `BrowserBack`.
            "Backspace" | "BrowserBack" | "GoBack" => Some(TvKey::Back),
            "MediaPlayPause" | "MediaPlay" | "MediaPause" => Some(TvKey::PlayPause),
            _ => None,
        }
    }

    /// The canonical `KeyboardEvent.key` name, for the `?keys=1` legend.
    pub fn canonical_key_name(self) -> &'static str {
        match self {
            TvKey::Up => "ArrowUp",
            TvKey::Down => "ArrowDown",
            TvKey::Left => "ArrowLeft",
            TvKey::Right => "ArrowRight",
            TvKey::Enter => "Enter",
            TvKey::Back => "Backspace",
            TvKey::PlayPause => "MediaPlayPause",
        }
    }

    /// What this key does, in words a parent reading `?keys=1` can check.
    pub fn describe(self) -> &'static str {
        match self {
            TvKey::Up => "previous profile / previous item",
            TvKey::Down => "next profile / next item",
            TvKey::Left => "previous panel",
            TvKey::Right => "next panel",
            TvKey::Enter => "open / toggle",
            TvKey::Back => "back",
            TvKey::PlayPause => "phone QR",
        }
    }
}

// ---------------------------------------------------------------------------
// `?keys=1` — the key-code debug overlay
// ---------------------------------------------------------------------------

/// How many presses the debug overlay keeps on screen.
pub const KEY_LOG_CAPACITY: usize = 8;

/// One row of the `?keys=1` overlay.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KeyLogEntry {
    /// `KeyboardEvent.key` exactly as the browser reported it.
    pub key: String,
    /// `KeyboardEvent.code` exactly as the browser reported it.
    pub code: String,
    /// What the kiosk did with it, or `None` if it was ignored.
    pub mapped: Option<TvKey>,
}

impl KeyLogEntry {
    pub fn new(key: impl Into<String>, code: impl Into<String>) -> Self {
        let key = key.into();
        let mapped = TvKey::from_key(&key);
        Self {
            key,
            code: code.into(),
            mapped,
        }
    }

    /// The action column of the overlay.
    pub fn action(&self) -> &'static str {
        match self.mapped {
            Some(key) => key.describe(),
            None => "ignored",
        }
    }
}

/// Push `entry` onto the bounded key log, dropping the oldest row.
pub fn push_key_log(log: &mut Vec<KeyLogEntry>, entry: KeyLogEntry) {
    log.push(entry);
    if log.len() > KEY_LOG_CAPACITY {
        let overflow = log.len() - KEY_LOG_CAPACITY;
        log.drain(0..overflow);
    }
}

/// Is the key-code debug overlay switched on for this page?
///
/// `query` is the raw `window.location.search`, with or without the leading
/// `?`. Only `keys=1` turns it on: `keys=0` and a bare `keys` do not, so a
/// stray bookmark cannot leave a debug HUD burned onto the family's
/// television.
pub fn keys_debug_enabled(query: &str) -> bool {
    query
        .trim_start_matches('?')
        .split('&')
        .any(|pair| pair == "keys=1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seven_remote_keys_map() {
        for key in TV_KEYS {
            assert_eq!(
                TvKey::from_key(key.canonical_key_name()),
                Some(key),
                "{} did not map back to itself",
                key.canonical_key_name()
            );
        }
    }

    #[test]
    fn escape_is_not_in_the_map() {
        // R-11: Fire TV remotes have no Escape key. Mapping it would let a
        // desktop test pass a path a child at the television cannot walk.
        assert_eq!(TvKey::from_key("Escape"), None);
        assert_eq!(TvKey::from_key("Esc"), None);
    }

    #[test]
    fn unknown_keys_fall_through_rather_than_guessing() {
        for key in ["F5", "a", "Tab", "ContextMenu", ""] {
            assert_eq!(TvKey::from_key(key), None, "{key} should not be mapped");
        }
    }

    #[test]
    fn the_back_button_is_accepted_under_all_three_spec_names() {
        for key in ["Backspace", "BrowserBack", "GoBack"] {
            assert_eq!(TvKey::from_key(key), Some(TvKey::Back), "{key}");
        }
    }

    #[test]
    fn the_key_log_is_bounded_and_keeps_the_newest_presses() {
        let mut log = Vec::new();
        for n in 0..(KEY_LOG_CAPACITY + 5) {
            push_key_log(&mut log, KeyLogEntry::new(format!("k{n}"), format!("c{n}")));
        }
        assert_eq!(log.len(), KEY_LOG_CAPACITY);
        assert_eq!(
            log.last().expect("a row").key,
            format!("k{}", KEY_LOG_CAPACITY + 4)
        );
        assert_eq!(log.first().expect("a row").key, "k5");
    }

    #[test]
    fn a_logged_press_records_whether_the_kiosk_acted_on_it() {
        let acted = KeyLogEntry::new("ArrowDown", "ArrowDown");
        assert_eq!(acted.mapped, Some(TvKey::Down));
        assert_eq!(acted.action(), "next profile / next item");

        let ignored = KeyLogEntry::new("Escape", "Escape");
        assert_eq!(ignored.mapped, None);
        assert_eq!(ignored.action(), "ignored");
    }

    #[test]
    fn only_keys_equals_one_turns_the_debug_overlay_on() {
        assert!(keys_debug_enabled("?keys=1"));
        assert!(keys_debug_enabled("keys=1"));
        assert!(keys_debug_enabled("?profile=2&keys=1"));
        assert!(!keys_debug_enabled(""));
        assert!(!keys_debug_enabled("?keys=0"));
        assert!(!keys_debug_enabled("?keys"));
        assert!(!keys_debug_enabled("?monkeys=1"));
    }
}
