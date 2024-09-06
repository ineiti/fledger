/// A configuration to set up a WebRTC connection.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// The signalling server, defaults to "ws://localhost:8765"
    signal_server: Option<String>,
    /// The STUN server, defaults to "stun:stun.l.google.com:19302"
    stun_server: Option<HostLogin>,
    /// The TURN server, by default none
    turn_server: Option<HostLogin>,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            signal_server: Some("ws://localhost:8765".into()),
            stun_server: Some(HostLogin {
                url: "stun:stun.l.google.com:19302".into(),
                login: None,
            }),
            turn_server: None,
        }
    }
}

impl ConnectionConfig {
    pub fn new(
        signal_server: Option<String>,
        stun_server: Option<HostLogin>,
        turn_server: Option<HostLogin>,
    ) -> Self {
        Self {
            signal_server,
            stun_server,
            turn_server,
        }
    }

    /// Returns a ConnectionConfig with only the url of the signalling server set.
    pub fn from_signal(url: &str) -> Self {
        Self {
            signal_server: Some(url.into()),
            stun_server: None,
            turn_server: None,
        }
    }

    /// Returns the signalling server, or the default if none set.
    pub fn signal(&self) -> String {
        self.signal_server
            .as_ref()
            .unwrap_or(&"ws://localhost:8765".into())
            .clone()
    }

    /// Returns the STUN server, or the default if none set.
    pub fn stun(&self) -> HostLogin {
        self.stun_server
            .as_ref()
            .unwrap_or(&HostLogin::from_url("stun:stun.l.google.com:19302"))
            .clone()
    }

    /// Returns an Option to the turn server, as there is no default available publicly.
    pub fn turn(&self) -> Option<HostLogin> {
        self.turn_server.clone()
    }
}

/// A URL with an optional username/password.
#[derive(Debug, Clone)]
pub struct HostLogin {
    /// The URL for the host
    pub url: String,
    /// An optional username/password to authenticate to the host
    pub login: Option<Login>,
}

impl HostLogin {
    pub fn from_url(url: &str) -> Self {
        Self {
            url: url.into(),
            login: None,
        }
    }
}

/// A login with a username/password.
#[derive(Debug, Clone)]
pub struct Login {
    /// The username
    pub user: String,
    /// The password
    pub pass: String,
}

