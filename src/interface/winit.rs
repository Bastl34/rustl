use winit::keyboard::KeyLocation;

use crate::input::{keyboard::Key, mouse::MouseButton};

pub fn winit_map_key(logical_key: &winit::keyboard::Key, physical_key: &winit::keyboard::PhysicalKey, location: winit::keyboard::KeyLocation) -> Key
{
    // https://github.com/rust-windowing/winit/blob/master/src/platform_impl/android/keycodes.rs

    match logical_key
    {
        winit::keyboard::Key::Named(key) =>
        {
            match key
            {
                winit::keyboard::NamedKey::Key11 => return Key::Key11,
                winit::keyboard::NamedKey::Key12 => return Key::Key12,

                winit::keyboard::NamedKey::Alt => return Key::Alt,
                winit::keyboard::NamedKey::AltGraph => return Key::AltGraph,
                winit::keyboard::NamedKey::CapsLock => return Key::CapsLock,
                winit::keyboard::NamedKey::Control => return Key::Control,
                winit::keyboard::NamedKey::Fn => return Key::Fn,
                winit::keyboard::NamedKey::FnLock => return Key::FnLock,
                winit::keyboard::NamedKey::NumLock => return Key::NumLock,
                winit::keyboard::NamedKey::ScrollLock => return Key::ScrollLock,
                winit::keyboard::NamedKey::Shift => return Key::Shift,
                winit::keyboard::NamedKey::Symbol => return Key::Symbol,
                winit::keyboard::NamedKey::SymbolLock => return Key::SymbolLock,
                winit::keyboard::NamedKey::Meta => return Key::Meta,
                winit::keyboard::NamedKey::Hyper => return Key::Hyper,
                winit::keyboard::NamedKey::Super => return Key::Super,
                winit::keyboard::NamedKey::Tab => return Key::Tab,
                winit::keyboard::NamedKey::Space => return Key::Space,
                winit::keyboard::NamedKey::ArrowDown => return Key::ArrowDown,
                winit::keyboard::NamedKey::ArrowLeft => return Key::ArrowLeft,
                winit::keyboard::NamedKey::ArrowRight => return Key::ArrowRight,
                winit::keyboard::NamedKey::ArrowUp => return Key::ArrowUp,
                winit::keyboard::NamedKey::End => return Key::End,
                winit::keyboard::NamedKey::Home => return Key::Home,
                winit::keyboard::NamedKey::PageDown => return Key::PageDown,
                winit::keyboard::NamedKey::PageUp => return Key::PageUp,
                winit::keyboard::NamedKey::Backspace => return Key::Backspace,
                winit::keyboard::NamedKey::Clear => return Key::Clear,
                winit::keyboard::NamedKey::Copy => return Key::Copy,
                winit::keyboard::NamedKey::CrSel => return Key::CrSel,
                winit::keyboard::NamedKey::Cut => return Key::Cut,
                winit::keyboard::NamedKey::Delete => return Key::Delete,
                winit::keyboard::NamedKey::EraseEof => return Key::EraseEof,
                winit::keyboard::NamedKey::ExSel => return Key::ExSel,
                winit::keyboard::NamedKey::Insert => return Key::Insert,
                winit::keyboard::NamedKey::Paste => return Key::Paste,
                winit::keyboard::NamedKey::Redo => return Key::Redo,
                winit::keyboard::NamedKey::Undo => return Key::Undo,
                winit::keyboard::NamedKey::Accept => return Key::Accept,
                winit::keyboard::NamedKey::Again => return Key::Again,
                winit::keyboard::NamedKey::Attn => return Key::Attn,
                winit::keyboard::NamedKey::Cancel => return Key::Cancel,
                winit::keyboard::NamedKey::ContextMenu => return Key::ContextMenu,
                winit::keyboard::NamedKey::Escape => return Key::Escape,
                winit::keyboard::NamedKey::Execute => return Key::Execute,
                winit::keyboard::NamedKey::Find => return Key::Find,
                winit::keyboard::NamedKey::Help => return Key::Help,
                winit::keyboard::NamedKey::Pause => return Key::Pause,
                winit::keyboard::NamedKey::Play => return Key::Play,
                winit::keyboard::NamedKey::Props => return Key::Props,
                winit::keyboard::NamedKey::Select => return Key::Select,
                winit::keyboard::NamedKey::ZoomIn => return Key::ZoomIn,
                winit::keyboard::NamedKey::ZoomOut => return Key::ZoomOut,
                winit::keyboard::NamedKey::BrightnessDown => return Key::BrightnessDown,
                winit::keyboard::NamedKey::BrightnessUp => return Key::BrightnessUp,
                winit::keyboard::NamedKey::Eject => return Key::Eject,
                winit::keyboard::NamedKey::LogOff => return Key::LogOff,
                winit::keyboard::NamedKey::Power => return Key::Power,
                winit::keyboard::NamedKey::PowerOff => return Key::PowerOff,
                winit::keyboard::NamedKey::PrintScreen => return Key::PrintScreen,
                winit::keyboard::NamedKey::Hibernate => return Key::Hibernate,
                winit::keyboard::NamedKey::Standby => return Key::Standby,
                winit::keyboard::NamedKey::WakeUp => return Key::WakeUp,
                winit::keyboard::NamedKey::AllCandidates => return Key::AllCandidates,
                winit::keyboard::NamedKey::Alphanumeric => return Key::Alphanumeric,
                winit::keyboard::NamedKey::CodeInput => return Key::CodeInput,
                winit::keyboard::NamedKey::Compose => return Key::Compose,
                winit::keyboard::NamedKey::Convert => return Key::Convert,
                winit::keyboard::NamedKey::FinalMode => return Key::FinalMode,
                winit::keyboard::NamedKey::GroupFirst => return Key::GroupFirst,
                winit::keyboard::NamedKey::GroupLast => return Key::GroupLast,
                winit::keyboard::NamedKey::GroupNext => return Key::GroupNext,
                winit::keyboard::NamedKey::GroupPrevious => return Key::GroupPrevious,
                winit::keyboard::NamedKey::ModeChange => return Key::ModeChange,
                winit::keyboard::NamedKey::NextCandidate => return Key::NextCandidate,
                winit::keyboard::NamedKey::NonConvert => return Key::NonConvert,
                winit::keyboard::NamedKey::PreviousCandidate => return Key::PreviousCandidate,
                winit::keyboard::NamedKey::Process => return Key::Process,
                winit::keyboard::NamedKey::SingleCandidate => return Key::SingleCandidate,
                winit::keyboard::NamedKey::HangulMode => return Key::HangulMode,
                winit::keyboard::NamedKey::HanjaMode => return Key::HanjaMode,
                winit::keyboard::NamedKey::JunjaMode => return Key::JunjaMode,
                winit::keyboard::NamedKey::Eisu => return Key::Eisu,
                winit::keyboard::NamedKey::Hankaku => return Key::Hankaku,
                winit::keyboard::NamedKey::Hiragana => return Key::Hiragana,
                winit::keyboard::NamedKey::HiraganaKatakana => return Key::HiraganaKatakana,
                winit::keyboard::NamedKey::KanaMode => return Key::KanaMode,
                winit::keyboard::NamedKey::KanjiMode => return Key::KanjiMode,
                winit::keyboard::NamedKey::Katakana => return Key::Katakana,
                winit::keyboard::NamedKey::Romaji => return Key::Romaji,
                winit::keyboard::NamedKey::Zenkaku => return Key::Zenkaku,
                winit::keyboard::NamedKey::ZenkakuHankaku => return Key::ZenkakuHankaku,
                winit::keyboard::NamedKey::Soft1 => return Key::Soft1,
                winit::keyboard::NamedKey::Soft2 => return Key::Soft2,
                winit::keyboard::NamedKey::Soft3 => return Key::Soft3,
                winit::keyboard::NamedKey::Soft4 => return Key::Soft4,
                winit::keyboard::NamedKey::ChannelDown => return Key::ChannelDown,
                winit::keyboard::NamedKey::ChannelUp => return Key::ChannelUp,
                winit::keyboard::NamedKey::Close => return Key::Close,
                winit::keyboard::NamedKey::MailForward => return Key::MailForward,
                winit::keyboard::NamedKey::MailReply => return Key::MailReply,
                winit::keyboard::NamedKey::MailSend => return Key::MailSend,
                winit::keyboard::NamedKey::MediaClose => return Key::MediaClose,
                winit::keyboard::NamedKey::MediaFastForward => return Key::MediaFastForward,
                winit::keyboard::NamedKey::MediaPause => return Key::MediaPause,
                winit::keyboard::NamedKey::MediaPlay => return Key::MediaPlay,
                winit::keyboard::NamedKey::MediaPlayPause => return Key::MediaPlayPause,
                winit::keyboard::NamedKey::MediaRecord => return Key::MediaRecord,
                winit::keyboard::NamedKey::MediaRewind => return Key::MediaRewind,
                winit::keyboard::NamedKey::MediaStop => return Key::MediaStop,
                winit::keyboard::NamedKey::MediaTrackNext => return Key::MediaTrackNext,
                winit::keyboard::NamedKey::MediaTrackPrevious => return Key::MediaTrackPrevious,
                winit::keyboard::NamedKey::New => return Key::New,
                winit::keyboard::NamedKey::Open => return Key::Open,
                winit::keyboard::NamedKey::Print => return Key::Print,
                winit::keyboard::NamedKey::Save => return Key::Save,
                winit::keyboard::NamedKey::SpellCheck => return Key::SpellCheck,
                winit::keyboard::NamedKey::AudioBalanceLeft => return Key::AudioBalanceLeft,
                winit::keyboard::NamedKey::AudioBalanceRight => return Key::AudioBalanceRight,
                winit::keyboard::NamedKey::AudioBassBoostDown => return Key::AudioBassBoostDown,
                winit::keyboard::NamedKey::AudioBassBoostToggle => return Key::AudioBassBoostToggle,
                winit::keyboard::NamedKey::AudioBassBoostUp => return Key::AudioBassBoostUp,
                winit::keyboard::NamedKey::AudioFaderFront => return Key::AudioFaderFront,
                winit::keyboard::NamedKey::AudioFaderRear => return Key::AudioFaderRear,
                winit::keyboard::NamedKey::AudioSurroundModeNext => return Key::AudioSurroundModeNext,
                winit::keyboard::NamedKey::AudioTrebleDown => return Key::AudioTrebleDown,
                winit::keyboard::NamedKey::AudioTrebleUp => return Key::AudioTrebleUp,
                winit::keyboard::NamedKey::AudioVolumeDown => return Key::AudioVolumeDown,
                winit::keyboard::NamedKey::AudioVolumeUp => return Key::AudioVolumeUp,
                winit::keyboard::NamedKey::AudioVolumeMute => return Key::AudioVolumeMute,
                winit::keyboard::NamedKey::MicrophoneToggle => return Key::MicrophoneToggle,
                winit::keyboard::NamedKey::MicrophoneVolumeDown => return Key::MicrophoneVolumeDown,
                winit::keyboard::NamedKey::MicrophoneVolumeUp => return Key::MicrophoneVolumeUp,
                winit::keyboard::NamedKey::MicrophoneVolumeMute => return Key::MicrophoneVolumeMute,
                winit::keyboard::NamedKey::SpeechCorrectionList => return Key::SpeechCorrectionList,
                winit::keyboard::NamedKey::SpeechInputToggle => return Key::SpeechInputToggle,
                winit::keyboard::NamedKey::LaunchApplication1 => return Key::LaunchApplication1,
                winit::keyboard::NamedKey::LaunchApplication2 => return Key::LaunchApplication2,
                winit::keyboard::NamedKey::LaunchCalendar => return Key::LaunchCalendar,
                winit::keyboard::NamedKey::LaunchContacts => return Key::LaunchContacts,
                winit::keyboard::NamedKey::LaunchMail => return Key::LaunchMail,
                winit::keyboard::NamedKey::LaunchMediaPlayer => return Key::LaunchMediaPlayer,
                winit::keyboard::NamedKey::LaunchMusicPlayer => return Key::LaunchMusicPlayer,
                winit::keyboard::NamedKey::LaunchPhone => return Key::LaunchPhone,
                winit::keyboard::NamedKey::LaunchScreenSaver => return Key::LaunchScreenSaver,
                winit::keyboard::NamedKey::LaunchSpreadsheet => return Key::LaunchSpreadsheet,
                winit::keyboard::NamedKey::LaunchWebBrowser => return Key::LaunchWebBrowser,
                winit::keyboard::NamedKey::LaunchWebCam => return Key::LaunchWebCam,
                winit::keyboard::NamedKey::LaunchWordProcessor => return Key::LaunchWordProcessor,
                winit::keyboard::NamedKey::BrowserBack => return Key::BrowserBack,
                winit::keyboard::NamedKey::BrowserFavorites => return Key::BrowserFavorites,
                winit::keyboard::NamedKey::BrowserForward => return Key::BrowserForward,
                winit::keyboard::NamedKey::BrowserHome => return Key::BrowserHome,
                winit::keyboard::NamedKey::BrowserRefresh => return Key::BrowserRefresh,
                winit::keyboard::NamedKey::BrowserSearch => return Key::BrowserSearch,
                winit::keyboard::NamedKey::BrowserStop => return Key::BrowserStop,
                winit::keyboard::NamedKey::AppSwitch => return Key::AppSwitch,
                winit::keyboard::NamedKey::Call => return Key::Call,
                winit::keyboard::NamedKey::Camera => return Key::Camera,
                winit::keyboard::NamedKey::CameraFocus => return Key::CameraFocus,
                winit::keyboard::NamedKey::EndCall => return Key::EndCall,
                winit::keyboard::NamedKey::GoBack => return Key::GoBack,
                winit::keyboard::NamedKey::GoHome => return Key::GoHome,
                winit::keyboard::NamedKey::HeadsetHook => return Key::HeadsetHook,
                winit::keyboard::NamedKey::LastNumberRedial => return Key::LastNumberRedial,
                winit::keyboard::NamedKey::Notification => return Key::Notification,
                winit::keyboard::NamedKey::MannerMode => return Key::MannerMode,
                winit::keyboard::NamedKey::VoiceDial => return Key::VoiceDial,
                winit::keyboard::NamedKey::TV => return Key::TV,
                winit::keyboard::NamedKey::TV3DMode => return Key::TV3DMode,
                winit::keyboard::NamedKey::TVAntennaCable => return Key::TVAntennaCable,
                winit::keyboard::NamedKey::TVAudioDescription => return Key::TVAudioDescription,
                winit::keyboard::NamedKey::TVAudioDescriptionMixDown => return Key::TVAudioDescriptionMixDown,
                winit::keyboard::NamedKey::TVAudioDescriptionMixUp => return Key::TVAudioDescriptionMixUp,
                winit::keyboard::NamedKey::TVContentsMenu => return Key::TVContentsMenu,
                winit::keyboard::NamedKey::TVDataService => return Key::TVDataService,
                winit::keyboard::NamedKey::TVInput => return Key::TVInput,
                winit::keyboard::NamedKey::TVInputComponent1 => return Key::TVInputComponent1,
                winit::keyboard::NamedKey::TVInputComponent2 => return Key::TVInputComponent2,
                winit::keyboard::NamedKey::TVInputComposite1 => return Key::TVInputComposite1,
                winit::keyboard::NamedKey::TVInputComposite2 => return Key::TVInputComposite2,
                winit::keyboard::NamedKey::TVInputHDMI1 => return Key::TVInputHDMI1,
                winit::keyboard::NamedKey::TVInputHDMI2 => return Key::TVInputHDMI2,
                winit::keyboard::NamedKey::TVInputHDMI3 => return Key::TVInputHDMI3,
                winit::keyboard::NamedKey::TVInputHDMI4 => return Key::TVInputHDMI4,
                winit::keyboard::NamedKey::TVInputVGA1 => return Key::TVInputVGA1,
                winit::keyboard::NamedKey::TVMediaContext => return Key::TVMediaContext,
                winit::keyboard::NamedKey::TVNetwork => return Key::TVNetwork,
                winit::keyboard::NamedKey::TVNumberEntry => return Key::TVNumberEntry,
                winit::keyboard::NamedKey::TVPower => return Key::TVPower,
                winit::keyboard::NamedKey::TVRadioService => return Key::TVRadioService,
                winit::keyboard::NamedKey::TVSatellite => return Key::TVSatellite,
                winit::keyboard::NamedKey::TVSatelliteBS => return Key::TVSatelliteBS,
                winit::keyboard::NamedKey::TVSatelliteCS => return Key::TVSatelliteCS,
                winit::keyboard::NamedKey::TVSatelliteToggle => return Key::TVSatelliteToggle,
                winit::keyboard::NamedKey::TVTerrestrialAnalog => return Key::TVTerrestrialAnalog,
                winit::keyboard::NamedKey::TVTerrestrialDigital => return Key::TVTerrestrialDigital,
                winit::keyboard::NamedKey::TVTimer => return Key::TVTimer,
                winit::keyboard::NamedKey::AVRInput => return Key::AVRInput,
                winit::keyboard::NamedKey::AVRPower => return Key::AVRPower,
                winit::keyboard::NamedKey::ColorF0Red => return Key::ColorF0Red,
                winit::keyboard::NamedKey::ColorF1Green => return Key::ColorF1Green,
                winit::keyboard::NamedKey::ColorF2Yellow => return Key::ColorF2Yellow,
                winit::keyboard::NamedKey::ColorF3Blue => return Key::ColorF3Blue,
                winit::keyboard::NamedKey::ColorF4Grey => return Key::ColorF4Grey,
                winit::keyboard::NamedKey::ColorF5Brown => return Key::ColorF5Brown,
                winit::keyboard::NamedKey::ClosedCaptionToggle => return Key::ClosedCaptionToggle,
                winit::keyboard::NamedKey::Dimmer => return Key::Dimmer,
                winit::keyboard::NamedKey::DisplaySwap => return Key::DisplaySwap,
                winit::keyboard::NamedKey::DVR => return Key::DVR,
                winit::keyboard::NamedKey::Exit => return Key::Exit,
                winit::keyboard::NamedKey::FavoriteClear0 => return Key::FavoriteClear0,
                winit::keyboard::NamedKey::FavoriteClear1 => return Key::FavoriteClear1,
                winit::keyboard::NamedKey::FavoriteClear2 => return Key::FavoriteClear2,
                winit::keyboard::NamedKey::FavoriteClear3 => return Key::FavoriteClear3,
                winit::keyboard::NamedKey::FavoriteRecall0 => return Key::FavoriteRecall0,
                winit::keyboard::NamedKey::FavoriteRecall1 => return Key::FavoriteRecall1,
                winit::keyboard::NamedKey::FavoriteRecall2 => return Key::FavoriteRecall2,
                winit::keyboard::NamedKey::FavoriteRecall3 => return Key::FavoriteRecall3,
                winit::keyboard::NamedKey::FavoriteStore0 => return Key::FavoriteStore0,
                winit::keyboard::NamedKey::FavoriteStore1 => return Key::FavoriteStore1,
                winit::keyboard::NamedKey::FavoriteStore2 => return Key::FavoriteStore2,
                winit::keyboard::NamedKey::FavoriteStore3 => return Key::FavoriteStore3,
                winit::keyboard::NamedKey::Guide => return Key::Guide,
                winit::keyboard::NamedKey::GuideNextDay => return Key::GuideNextDay,
                winit::keyboard::NamedKey::GuidePreviousDay => return Key::GuidePreviousDay,
                winit::keyboard::NamedKey::Info => return Key::Info,
                winit::keyboard::NamedKey::InstantReplay => return Key::InstantReplay,
                winit::keyboard::NamedKey::Link => return Key::Link,
                winit::keyboard::NamedKey::ListProgram => return Key::ListProgram,
                winit::keyboard::NamedKey::LiveContent => return Key::LiveContent,
                winit::keyboard::NamedKey::Lock => return Key::Lock,
                winit::keyboard::NamedKey::MediaApps => return Key::MediaApps,
                winit::keyboard::NamedKey::MediaAudioTrack => return Key::MediaAudioTrack,
                winit::keyboard::NamedKey::MediaLast => return Key::MediaLast,
                winit::keyboard::NamedKey::MediaSkipBackward => return Key::MediaSkipBackward,
                winit::keyboard::NamedKey::MediaSkipForward => return Key::MediaSkipForward,
                winit::keyboard::NamedKey::MediaStepBackward => return Key::MediaStepBackward,
                winit::keyboard::NamedKey::MediaStepForward => return Key::MediaStepForward,
                winit::keyboard::NamedKey::MediaTopMenu => return Key::MediaTopMenu,
                winit::keyboard::NamedKey::NavigateIn => return Key::NavigateIn,
                winit::keyboard::NamedKey::NavigateNext => return Key::NavigateNext,
                winit::keyboard::NamedKey::NavigateOut => return Key::NavigateOut,
                winit::keyboard::NamedKey::NavigatePrevious => return Key::NavigatePrevious,
                winit::keyboard::NamedKey::NextFavoriteChannel => return Key::NextFavoriteChannel,
                winit::keyboard::NamedKey::NextUserProfile => return Key::NextUserProfile,
                winit::keyboard::NamedKey::OnDemand => return Key::OnDemand,
                winit::keyboard::NamedKey::Pairing => return Key::Pairing,
                winit::keyboard::NamedKey::PinPDown => return Key::PinPDown,
                winit::keyboard::NamedKey::PinPMove => return Key::PinPMove,
                winit::keyboard::NamedKey::PinPToggle => return Key::PinPToggle,
                winit::keyboard::NamedKey::PinPUp => return Key::PinPUp,
                winit::keyboard::NamedKey::PlaySpeedDown => return Key::PlaySpeedDown,
                winit::keyboard::NamedKey::PlaySpeedReset => return Key::PlaySpeedReset,
                winit::keyboard::NamedKey::PlaySpeedUp => return Key::PlaySpeedUp,
                winit::keyboard::NamedKey::RandomToggle => return Key::RandomToggle,
                winit::keyboard::NamedKey::RcLowBattery => return Key::RcLowBattery,
                winit::keyboard::NamedKey::RecordSpeedNext => return Key::RecordSpeedNext,
                winit::keyboard::NamedKey::RfBypass => return Key::RfBypass,
                winit::keyboard::NamedKey::ScanChannelsToggle => return Key::ScanChannelsToggle,
                winit::keyboard::NamedKey::ScreenModeNext => return Key::ScreenModeNext,
                winit::keyboard::NamedKey::Settings => return Key::Settings,
                winit::keyboard::NamedKey::SplitScreenToggle => return Key::SplitScreenToggle,
                winit::keyboard::NamedKey::STBInput => return Key::STBInput,
                winit::keyboard::NamedKey::STBPower => return Key::STBPower,
                winit::keyboard::NamedKey::Subtitle => return Key::Subtitle,
                winit::keyboard::NamedKey::Teletext => return Key::Teletext,
                winit::keyboard::NamedKey::VideoModeNext => return Key::VideoModeNext,
                winit::keyboard::NamedKey::Wink => return Key::Wink,
                winit::keyboard::NamedKey::ZoomToggle => return Key::ZoomToggle,

                winit::keyboard::NamedKey::Enter =>
                {
                    if location == KeyLocation::Numpad
                    {
                        return Key::NumpadEnter;
                    }
                    else
                    {
                        return Key::Enter;
                    }
                },

                winit::keyboard::NamedKey::F1 => return Key::F1,
                winit::keyboard::NamedKey::F2 => return Key::F2,
                winit::keyboard::NamedKey::F3 => return Key::F3,
                winit::keyboard::NamedKey::F4 => return Key::F4,
                winit::keyboard::NamedKey::F5 => return Key::F5,
                winit::keyboard::NamedKey::F6 => return Key::F6,
                winit::keyboard::NamedKey::F7 => return Key::F7,
                winit::keyboard::NamedKey::F8 => return Key::F8,
                winit::keyboard::NamedKey::F9 => return Key::F9,
                winit::keyboard::NamedKey::F10 => return Key::F10,
                winit::keyboard::NamedKey::F11 => return Key::F11,
                winit::keyboard::NamedKey::F12 => return Key::F12,
                winit::keyboard::NamedKey::F13 => return Key::F13,
                winit::keyboard::NamedKey::F14 => return Key::F14,
                winit::keyboard::NamedKey::F15 => return Key::F15,
                winit::keyboard::NamedKey::F16 => return Key::F16,
                winit::keyboard::NamedKey::F17 => return Key::F17,
                winit::keyboard::NamedKey::F18 => return Key::F18,
                winit::keyboard::NamedKey::F19 => return Key::F19,
                winit::keyboard::NamedKey::F20 => return Key::F20,
                winit::keyboard::NamedKey::F21 => return Key::F21,
                winit::keyboard::NamedKey::F22 => return Key::F22,
                winit::keyboard::NamedKey::F23 => return Key::F23,
                winit::keyboard::NamedKey::F24 => return Key::F24,
                winit::keyboard::NamedKey::F25 => return Key::F25,
                winit::keyboard::NamedKey::F26 => return Key::F26,
                winit::keyboard::NamedKey::F27 => return Key::F27,
                winit::keyboard::NamedKey::F28 => return Key::F28,
                winit::keyboard::NamedKey::F29 => return Key::F29,
                winit::keyboard::NamedKey::F30 => return Key::F30,
                winit::keyboard::NamedKey::F31 => return Key::F31,
                winit::keyboard::NamedKey::F32 => return Key::F32,
                winit::keyboard::NamedKey::F33 => return Key::F33,
                winit::keyboard::NamedKey::F34 => return Key::F34,
                winit::keyboard::NamedKey::F35 => return Key::F35,
                _ => {},
            };
        },
        winit::keyboard::Key::Character(char) =>
        {
            let char = char.to_ascii_lowercase();

            if      char == "a" { return Key::A; }
            else if char == "b" { return Key::B; }
            else if char == "c" { return Key::C; }
            else if char == "d" { return Key::D; }
            else if char == "e" { return Key::E; }
            else if char == "f" { return Key::F; }
            else if char == "g" { return Key::G; }
            else if char == "h" { return Key::H; }
            else if char == "i" { return Key::I; }
            else if char == "j" { return Key::J; }
            else if char == "k" { return Key::K; }
            else if char == "l" { return Key::L; }
            else if char == "m" { return Key::M; }
            else if char == "n" { return Key::N; }
            else if char == "o" { return Key::O; }
            else if char == "p" { return Key::P; }
            else if char == "q" { return Key::Q; }
            else if char == "r" { return Key::R; }
            else if char == "s" { return Key::S; }
            else if char == "t" { return Key::T; }
            else if char == "u" { return Key::U; }
            else if char == "v" { return Key::V; }
            else if char == "w" { return Key::W; }
            else if char == "x" { return Key::X; }
            else if char == "y" { return Key::Y; }
            else if char == "z" { return Key::Z; }

            else if char == "," { return Key::Comma; }
            else if char == "." { return Key::Period; }
            else if char == "`" { return Key::Grave; }
            else if char == "-" { return Key::Minus; }
            else if char == "=" { return Key::Equals; }
            else if char == "[" { return Key::LeftBracket; }
            else if char == "]" { return Key::RightBracket; }
            else if char == "\\" { return Key::Backslash; }
            else if char == ";" { return Key::Semicolon; }
            else if char == "'" { return Key::Apostrophe; }
            else if char == "/" { return Key::Slash; }
            else if char == "@" { return Key::At; }
            else if char == "+" { return Key::Plus; }
            else if char == "*" { return Key::Star; }
            else if char == "#" { return Key::Pound; }

            if location == KeyLocation::Numpad
            {
                     if char == "1" { return Key::Numpad1; }
                else if char == "2" { return Key::Numpad2; }
                else if char == "3" { return Key::Numpad3; }
                else if char == "4" { return Key::Numpad4; }
                else if char == "5" { return Key::Numpad5; }
                else if char == "6" { return Key::Numpad6; }
                else if char == "7" { return Key::Numpad7; }
                else if char == "8" { return Key::Numpad8; }
                else if char == "9" { return Key::Numpad9; }
                else if char == "0" { return Key::Numpad0; }

                else if char == "/" { return Key::NumpadDivide; }
                else if char == "*" { return Key::NumpadMultiply; }
                else if char == "-" { return Key::NumpadSubtract; }
                else if char == "+" { return Key::NumpadAdd; }
                else if char == "." { return Key::NumpadDecimal; }
                else if char == "," { return Key::NumpadComma; }
                else if char == "=" { return Key::NumpadEquals; }
                else if char == "(" { return Key::NumpadLeftParen; }
                else if char == ")" { return Key::NumpadRightParen; }
            }
            else
            {
                     if char == "1" { return Key::Key1; }
                else if char == "2" { return Key::Key2; }
                else if char == "3" { return Key::Key3; }
                else if char == "4" { return Key::Key4; }
                else if char == "5" { return Key::Key5; }
                else if char == "6" { return Key::Key6; }
                else if char == "7" { return Key::Key7; }
                else if char == "8" { return Key::Key8; }
                else if char == "9" { return Key::Key9; }
                else if char == "0" { return Key::Key0; }
            }
        },
        winit::keyboard::Key::Unidentified(_) => {},
        winit::keyboard::Key::Dead(_) => {},
    };

    Key::Unknown
}

pub fn winit_map_mouse_button(button: &winit::event::MouseButton) -> MouseButton
{
    match button
    {
        winit::event::MouseButton::Left => MouseButton::Left,
        winit::event::MouseButton::Right => MouseButton::Right,
        winit::event::MouseButton::Middle => MouseButton::Middle,
        winit::event::MouseButton::Back => MouseButton::Back,
        winit::event::MouseButton::Forward => MouseButton::Forward,
        winit::event::MouseButton::Other(other) =>
        {
            match other
            {
                1 => MouseButton::Other1,
                2 => MouseButton::Other2,
                3 => MouseButton::Other3,
                4 => MouseButton::Other4,
                5 => MouseButton::Other5,
                6 => MouseButton::Other6,
                7 => MouseButton::Other7,
                8 => MouseButton::Other8,
                _ => MouseButton::Unkown
            }
        },
    }
}