use lyre_core::IceServerConfig;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IceServerConfigError {
    #[error("ICE server entry must not be blank: `{value}`")]
    BlankEntry { value: String },
    #[error("ICE server entry contains a blank URL: `{value}`")]
    BlankUrl { value: String },
    #[error("ICE server entry has too many `|` separators: `{value}`")]
    TooManyFields { value: String },
    #[error("ICE server configuration must contain at least one server")]
    Empty,
    #[error("embedded TURN external address must be an IP socket address, got `{value}`")]
    InvalidEmbeddedTurnExternal { value: String },
}

pub(crate) fn parse_ice_server_entries(
    entries: &[String],
) -> Result<Vec<IceServerConfig>, IceServerConfigError> {
    let mut servers = Vec::with_capacity(entries.len());
    for entry in entries {
        servers.push(parse_ice_server_entry(entry)?);
    }
    if servers.is_empty() {
        return Err(IceServerConfigError::Empty);
    }
    Ok(servers)
}

fn parse_ice_server_entry(entry: &str) -> Result<IceServerConfig, IceServerConfigError> {
    if entry.trim().is_empty() {
        return Err(IceServerConfigError::BlankEntry {
            value: entry.to_owned(),
        });
    }
    let parts = entry.split('|').collect::<Vec<_>>();
    if parts.len() > 3 {
        return Err(IceServerConfigError::TooManyFields {
            value: entry.to_owned(),
        });
    }
    let urls = parts[0]
        .split(',')
        .map(str::trim)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if urls.iter().any(|url| url.is_empty()) {
        return Err(IceServerConfigError::BlankUrl {
            value: entry.to_owned(),
        });
    }
    Ok(IceServerConfig {
        urls,
        username: parts.get(1).map(|value| (*value).to_owned()),
        credential: parts.get(2).map(|value| (*value).to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_ice_server_entries() {
        assert_eq!(
            parse_ice_server_entry(" "),
            Err(IceServerConfigError::BlankEntry {
                value: " ".to_owned()
            })
        );
        assert_eq!(
            parse_ice_server_entry("stun:a.example,"),
            Err(IceServerConfigError::BlankUrl {
                value: "stun:a.example,".to_owned()
            })
        );
        assert_eq!(
            parse_ice_server_entry("turn:x|u|p|extra"),
            Err(IceServerConfigError::TooManyFields {
                value: "turn:x|u|p|extra".to_owned()
            })
        );
    }
}
