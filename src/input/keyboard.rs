#![allow(dead_code)]

use strum::IntoEnumIterator;
use strum_macros::{EnumIter, Display, FromRepr};

use super::press_state::{PressState, PressStateType, is_pressed_by_state};

#[derive(EnumIter, Debug, PartialEq, Clone, Copy, Display, FromRepr)]
pub enum Key
{
    // OK
    Key1 = 0,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Key0,

    // OK
    Key11,
    Key12,

    // OK
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    // OK
    Comma,
    Period,
    Grave,
    Minus,
    Equals,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Apostrophe,
    Slash,
    At,
    Plus,
    Star,
    Pound,

    // OK
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    F25,
    F26,
    F27,
    F28,
    F29,
    F30,
    F31,
    F32,
    F33,
    F34,
    F35,

    // OK
    Alt,
    AltGraph,
    CapsLock,
    Control,
    Fn,
    FnLock,
    NumLock,
    ScrollLock,
    Shift,
    Symbol,
    SymbolLock,
    Meta,
    Hyper,
    Super,
    Enter,
    Tab,
    Space,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    End,
    Home,
    PageDown,
    PageUp,
    Backspace,
    Clear,
    Copy,
    CrSel,
    Cut,
    Delete,
    EraseEof,
    ExSel,
    Insert,
    Paste,
    Redo,
    Undo,
    Accept,
    Again,
    Attn,
    Cancel,
    ContextMenu,
    Escape,
    Execute,
    Find,
    Help,
    Pause,
    Play,
    Props,
    Select,
    ZoomIn,
    ZoomOut,
    BrightnessDown,
    BrightnessUp,
    Eject,
    LogOff,
    Power,
    PowerOff,
    PrintScreen,
    Hibernate,
    Standby,
    WakeUp,
    AllCandidates,
    Alphanumeric,
    CodeInput,
    Compose,
    Convert,
    FinalMode,
    GroupFirst,
    GroupLast,
    GroupNext,
    GroupPrevious,
    ModeChange,
    NextCandidate,
    NonConvert,
    PreviousCandidate,
    Process,
    SingleCandidate,
    HangulMode,
    HanjaMode,
    JunjaMode,
    Eisu,
    Hankaku,
    Hiragana,
    HiraganaKatakana,
    KanaMode,
    KanjiMode,
    Katakana,
    Romaji,
    Zenkaku,
    ZenkakuHankaku,
    Soft1,
    Soft2,
    Soft3,
    Soft4,
    ChannelDown,
    ChannelUp,
    Close,
    MailForward,
    MailReply,
    MailSend,
    MediaClose,
    MediaFastForward,
    MediaPause,
    MediaPlay,
    MediaPlayPause,
    MediaRecord,
    MediaRewind,
    MediaStop,
    MediaTrackNext,
    MediaTrackPrevious,
    New,
    Open,
    Print,
    Save,
    SpellCheck,
    AudioBalanceLeft,
    AudioBalanceRight,
    AudioBassBoostDown,
    AudioBassBoostToggle,
    AudioBassBoostUp,
    AudioFaderFront,
    AudioFaderRear,
    AudioSurroundModeNext,
    AudioTrebleDown,
    AudioTrebleUp,
    AudioVolumeDown,
    AudioVolumeUp,
    AudioVolumeMute,
    MicrophoneToggle,
    MicrophoneVolumeDown,
    MicrophoneVolumeUp,
    MicrophoneVolumeMute,
    SpeechCorrectionList,
    SpeechInputToggle,
    LaunchApplication1,
    LaunchApplication2,
    LaunchCalendar,
    LaunchContacts,
    LaunchMail,
    LaunchMediaPlayer,
    LaunchMusicPlayer,
    LaunchPhone,
    LaunchScreenSaver,
    LaunchSpreadsheet,
    LaunchWebBrowser,
    LaunchWebCam,
    LaunchWordProcessor,
    BrowserBack,
    BrowserFavorites,
    BrowserForward,
    BrowserHome,
    BrowserRefresh,
    BrowserSearch,
    BrowserStop,
    AppSwitch,
    Call,
    Camera,
    CameraFocus,
    EndCall,
    GoBack,
    GoHome,
    HeadsetHook,
    LastNumberRedial,
    Notification,
    MannerMode,
    VoiceDial,
    TV,
    TV3DMode,
    TVAntennaCable,
    TVAudioDescription,
    TVAudioDescriptionMixDown,
    TVAudioDescriptionMixUp,
    TVContentsMenu,
    TVDataService,
    TVInput,
    TVInputComponent1,
    TVInputComponent2,
    TVInputComposite1,
    TVInputComposite2,
    TVInputHDMI1,
    TVInputHDMI2,
    TVInputHDMI3,
    TVInputHDMI4,
    TVInputVGA1,
    TVMediaContext,
    TVNetwork,
    TVNumberEntry,
    TVPower,
    TVRadioService,
    TVSatellite,
    TVSatelliteBS,
    TVSatelliteCS,
    TVSatelliteToggle,
    TVTerrestrialAnalog,
    TVTerrestrialDigital,
    TVTimer,
    AVRInput,
    AVRPower,
    ColorF0Red,
    ColorF1Green,
    ColorF2Yellow,
    ColorF3Blue,
    ColorF4Grey,
    ColorF5Brown,
    ClosedCaptionToggle,
    Dimmer,
    DisplaySwap,
    DVR,
    Exit,
    FavoriteClear0,
    FavoriteClear1,
    FavoriteClear2,
    FavoriteClear3,
    FavoriteRecall0,
    FavoriteRecall1,
    FavoriteRecall2,
    FavoriteRecall3,
    FavoriteStore0,
    FavoriteStore1,
    FavoriteStore2,
    FavoriteStore3,
    Guide,
    GuideNextDay,
    GuidePreviousDay,
    Info,
    InstantReplay,
    Link,
    ListProgram,
    LiveContent,
    Lock,
    MediaApps,
    MediaAudioTrack,
    MediaLast,
    MediaSkipBackward,
    MediaSkipForward,
    MediaStepBackward,
    MediaStepForward,
    MediaTopMenu,
    NavigateIn,
    NavigateNext,
    NavigateOut,
    NavigatePrevious,
    NextFavoriteChannel,
    NextUserProfile,
    OnDemand,
    Pairing,
    PinPDown,
    PinPMove,
    PinPToggle,
    PinPUp,
    PlaySpeedDown,
    PlaySpeedReset,
    PlaySpeedUp,
    RandomToggle,
    RcLowBattery,
    RecordSpeedNext,
    RfBypass,
    ScanChannelsToggle,
    ScreenModeNext,
    Settings,
    SplitScreenToggle,
    STBInput,
    STBPower,
    Subtitle,
    Teletext,
    VideoModeNext,
    Wink,
    ZoomToggle,

    //OK
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,

    // OK
    NumpadAdd,
    NumpadDivide,
    NumpadDecimal,
    NumpadComma,
    NumpadEnter,
    NumpadEquals,
    NumpadMultiply,
    NumpadSubtract,
    NumpadLeftParen,
    NumpadRightParen,

    Unknown
}

pub fn get_keys_as_string_vec() -> Vec<String>
{
    let key_vec: Vec<Key> = Key::iter().collect::<Vec<_>>();
    key_vec.iter().map(|key| { key.to_string() }).collect::<Vec<_>>()
}

#[derive(EnumIter, Debug, PartialEq, Display)]
pub enum Modifier
{
    LeftShift = 0,
    RightShift,
    LeftCtrl,
    RightCtrl,
    LeftAlt,
    RightAlt,
    LeftLogo,
    RightLogo
}

pub struct Keyboard
{
    keys: Vec<PressState>,
    modifiers: Vec<PressState>,
}

impl Keyboard
{
    pub fn new() -> Self
    {
        let key_vec = Key::iter().collect::<Vec<_>>();
        let modifiers_vec = Modifier::iter().collect::<Vec<_>>();

        let key_states = key_vec.iter().map(|_key| { PressState::new() }).collect::<Vec<_>>();
        let mod_states = modifiers_vec.iter().map(|_key| { PressState::new() }).collect::<Vec<_>>();

        Self
        {
            keys: key_states,
            modifiers: mod_states,
        }
    }

    pub fn set_key(&mut self, key: Key, status: bool, engine_frame: u64)
    {
        self.keys[key as usize].update(status, engine_frame);
    }

    pub fn set_modifier(&mut self, modifier: Modifier, status: bool, engine_frame: u64)
    {
        self.modifiers[modifier as usize].update(status, engine_frame);
    }

    pub fn update_states(&mut self)
    {
        for key in &mut self.keys
        {
            key.update_state();
        }

        for modifier in &mut self.modifiers
        {
            modifier.update_state();
        }
    }

    pub fn reset(&mut self)
    {
        for key in &mut self.keys
        {
            key.reset(true);
        }

        for modifier in &mut self.modifiers
        {
            modifier.reset(true);
        }
    }

    pub fn is_any_key_holding(&self) -> bool
    {
        for key in &self.keys
        {
            if key.holding()
            {
                return true;
            }
        }

        for modifier in &self.modifiers
        {
            if modifier.holding()
            {
                return true;
            }
        }

        false
    }

    pub fn is_holding(&self, key: Key) -> bool
    {
        self.keys[key as usize].holding()
    }

    pub fn is_holding_by_keys(&self, keys: &Vec<Key>) -> bool
    {
        for key in keys
        {
            if self.keys[*key as usize].holding()
            {
                return true;
            }
        }

        false
    }

    pub fn is_holding_modifier(&self, modifier: Modifier) -> bool
    {
        self.modifiers[modifier as usize].holding()
    }

    pub fn get_pressed_state(&mut self, key: Key, wait_until_key_release: bool, ignore_long_press: bool) -> PressStateType
    {
        self.keys[key as usize].pressed(wait_until_key_release, ignore_long_press)
    }

    pub fn get_pressed_state_modifier(&mut self, modifier: Modifier, wait_until_key_release: bool, ignore_long_press: bool) -> PressStateType
    {
        self.modifiers[modifier as usize].pressed(wait_until_key_release, ignore_long_press)
    }

    pub fn is_pressed(&mut self, key: Key) -> bool
    {
        let state = self.keys[key as usize].pressed(true, false);
        is_pressed_by_state(state)
    }

    pub fn is_pressed_no_wait(&mut self, key: Key) -> bool
    {
        let state = self.keys[key as usize].pressed(false, false);
        is_pressed_by_state(state)
    }

    pub fn is_pressed_by_keys(&mut self, keys: &Vec<Key>) -> bool
    {
        let mut result = false;
        for key in keys
        {
            if self.keys[*key as usize].holding()
            {
                let state = self.keys[*key as usize].pressed(true, false);
                result =  is_pressed_by_state(state) || result;
            }
        }

        result
    }

    pub fn is_pressed_modifier(&mut self, modifier: Modifier) -> bool
    {
        let state = self.modifiers[modifier as usize].pressed(true, false);
        is_pressed_by_state(state)
    }

    pub fn has_input(&self) -> bool
    {
        self.is_any_key_holding()
    }
}