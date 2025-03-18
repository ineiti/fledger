use regex::Regex;
use serde::{Deserialize, Serialize};

/// A configuration to set up a WebRTC connection.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// The signalling server, defaults to "ws://localhost:8765"
    pub signal_server: String,
    /// The STUN server, defaults to "stun:stun.l.google.com:19302"
    pub stun_server: Option<HostLogin>,
    /// The TURN server, by default none
    pub turn_server: Option<HostLogin>,
}

#[derive(Serialize, Deserialize)]
pub struct IceServer {
    pub urls: String,
    pub username: Option<String>,
    pub credential: Option<String>,
}

impl ConnectionConfig {
    pub fn new(
        signal_server: String,
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
            signal_server: url.into(),
            stun_server: None,
            turn_server: None,
        }
    }

    /// Returns the signalling server, or the default if none set.
    pub fn signal(&self) -> String {
        self.signal_server.clone()
    }

    pub fn servers(&self) -> Vec<IceServer> {
        vec![&self.stun_server, &self.turn_server]
            .into_iter()
            .filter_map(|s| s.clone())
            .map(|s| Self::get_ice_server(s))
            .collect::<Vec<_>>()
    }

    fn get_ice_server(host: HostLogin) -> IceServer {
        let username = host.login.clone().map(|l| l.user);
        let credential = host.login.clone().map(|l| l.pass);
        IceServer {
            urls: host.url,
            username,
            credential,
        }
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

    pub fn from_login_url(url: &str) -> anyhow::Result<Self> {
        let re =
            Regex::new(r"^(?P<user>[^:]+):(?P<pass>[^@]+)@turn:(?P<domain>[^:]+):(?P<port>\d+)$")?;
        if let Some(caps) = re.captures(url) {
            let user = caps.name("user").unwrap().as_str().to_string();
            let pass = caps.name("pass").unwrap().as_str().to_string();
            let domain = caps.name("domain").unwrap().as_str().to_string();
            let port = caps.name("port").unwrap().as_str().to_string();
            Ok(Self {
                url: format!("turn:{}:{}", domain, port),
                login: Some(Login { user, pass }),
            })
        } else {
            Err(anyhow::anyhow!("Invalid URL format"))
        }
    }
}

/// A login with a username/password.
#[derive(Debug, Clone, PartialEq)]
pub struct Login {
    /// The username
    pub user: String,
    /// The password
    pub pass: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_login_url_valid() {
        let url = "user:pass@turn:example.com:3478";
        let host_login = HostLogin::from_login_url(url).unwrap();
        assert_eq!(host_login.url, "turn:example.com:3478");
        assert_eq!(
            host_login.login,
            Some(Login {
                user: "user".into(),
                pass: "pass".into()
            })
        );
    }

    #[test]
    fn test_from_login_url_invalid_format() {
        let url = "invalid_format";
        let result = HostLogin::from_login_url(url);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_login_url_missing_user() {
        let url = ":pass@turn:example.com:3478";
        let result = HostLogin::from_login_url(url);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_login_url_missing_pass() {
        let url = "user:@turn:example.com:3478";
        let result = HostLogin::from_login_url(url);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_login_url_missing_domain() {
        let url = "user:pass@turn::3478";
        let result = HostLogin::from_login_url(url);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_login_url_missing_port() {
        let url = "user:pass@turn:example.com:";
        let result = HostLogin::from_login_url(url);
        assert!(result.is_err());
    }
}
