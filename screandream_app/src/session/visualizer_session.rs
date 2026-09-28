#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioAccessState {
    NotDetermined,
    Granted,
    Denied,
    Revoked,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionState {
    Idle,
    RequestingAccess,
    Starting,
    Active,
    AudioUnavailable,
    AccessDenied,
    AccessRevoked,
    Stopping,
    Ended,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FullScreenExitChoice {
    ContinueWindowed,
    EndSession,
}

#[derive(Debug)]
pub struct VisualizerSession {
    pub state: SessionState,
    pub audio_access_state: AudioAccessState,
    pub full_screen: bool,
}

impl Default for VisualizerSession {
    fn default() -> Self {
        Self {
            state: SessionState::Idle,
            audio_access_state: AudioAccessState::NotDetermined,
            full_screen: false,
        }
    }
}

impl VisualizerSession {
    pub fn request_access(&mut self) {
        self.state = SessionState::RequestingAccess;
    }

    pub fn set_access(&mut self, access: AudioAccessState) {
        self.audio_access_state = access;
        self.state = match access {
            AudioAccessState::Granted => SessionState::Starting,
            AudioAccessState::Denied | AudioAccessState::Unavailable => SessionState::AccessDenied,
            AudioAccessState::Revoked => SessionState::AccessRevoked,
            AudioAccessState::NotDetermined => SessionState::RequestingAccess,
        };
    }

    pub fn activate(&mut self) {
        if self.audio_access_state == AudioAccessState::Granted {
            self.state = SessionState::Active;
            self.full_screen = true;
        }
    }

    pub fn set_audio_available(&mut self, usable: bool) {
        if matches!(
            self.state,
            SessionState::Active | SessionState::AudioUnavailable
        ) {
            self.state = if usable {
                SessionState::Active
            } else {
                SessionState::AudioUnavailable
            };
        }
    }

    pub fn revoke_access(&mut self) {
        self.audio_access_state = AudioAccessState::Revoked;
        self.state = SessionState::AccessRevoked;
        self.full_screen = false;
    }

    pub fn stop(&mut self) {
        self.state = SessionState::Stopping;
        self.full_screen = false;
        self.state = SessionState::Ended;
    }

    pub fn exit_full_screen(&mut self, choice: FullScreenExitChoice) {
        self.full_screen = false;
        if choice == FullScreenExitChoice::EndSession {
            self.stop();
        }
    }
}
