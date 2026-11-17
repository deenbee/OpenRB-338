//! The clipboard for copying and pasting patterns, automation, and tracks.

use orb_engine::song::Device;

/// A relative track event in a copied range of bars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelEvent {
    /// Relative tick offset from the start of the copied range (0 <= rel_tick < bars * 768).
    pub rel_tick: i32,
    pub ctrl: u8,
    pub value: u8,
}

/// What the clipboard currently holds.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Clipboard {
    /// In Pattern mode: the raw bytes of a single pattern (98 bytes for 303, 194 for drums).
    Pattern {
        device: Device,
        pattern: u8,
        bytes: Vec<u8>,
    },
    /// In Song mode: events across the 9 tracks for `bars` bars.
    SongEvents {
        bars: i32,
        /// Filter that was applied during copy.
        filter: CopyFilter,
        /// Relative events per track 0..8.
        tracks: [Vec<RelEvent>; 9],
        description: String,
    },
}

/// The filter used when copying in Song mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyFilter {
    /// Only pattern selection (ctrl == 1) and on/off (ctrl == 0) of the selected machine.
    MachinePattern(usize),
    /// Only knobs (ctrl >= 2) of the selected machine.
    MachineKnobs(usize),
    /// Both pattern and knobs of the selected machine.
    MachineAll(usize),
    /// Pattern selection (ctrl <= 1) of all 4 machines (tracks 1..=4).
    AllPatterns,
    /// Mixer & FX automation (tracks 0, 5, 6, 7, 8).
    MixerFx,
}
