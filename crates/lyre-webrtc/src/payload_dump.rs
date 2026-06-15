use std::{
    env,
    fs::{File, OpenOptions},
    io::{self, Write},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use lyre_core::UserId;
use tracing::warn;

use crate::{ServerMediaEgressRtpPacket, ServerMediaRtpPacket, ServerMediaSessionKey};

const DEBUG_DUMP_PAYLOAD_ENV: &str = "LYRE_DEBUG_DUMP_PAYLOAD";
#[cfg(test)]
const PAYLOAD_DUMP_RECORD_HEADER_LEN: usize = 11;

#[derive(Debug, Clone)]
pub(crate) struct PayloadDumper {
    inner: Arc<Mutex<PayloadDumperState>>,
}

#[derive(Debug)]
struct PayloadDumperState {
    enabled: bool,
    conn_start_time: u128,
    writers: Option<PayloadDumpWriters>,
}

#[derive(Debug)]
struct PayloadDumpWriters {
    inbound: PayloadDumpWriter,
    outbound: PayloadDumpWriter,
}

#[derive(Debug)]
struct PayloadDumpWriter {
    file: File,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PayloadDumpDirection {
    Inbound,
    Outbound,
}

impl PayloadDumper {
    pub(crate) fn from_env() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PayloadDumperState {
                enabled: env::var_os(DEBUG_DUMP_PAYLOAD_ENV).is_some_and(|value| !value.is_empty()),
                conn_start_time: connection_start_time_millis(),
                writers: None,
            })),
        }
    }

    #[cfg(test)]
    pub(crate) fn disabled_for_test() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PayloadDumperState {
                enabled: false,
                conn_start_time: 1,
                writers: None,
            })),
        }
    }

    pub(crate) fn set_session_key(&self, key: &ServerMediaSessionKey) {
        let mut state = self
            .inner
            .lock()
            .expect("payload dumper lock must not be poisoned");
        if !state.enabled || state.writers.is_some() {
            return;
        }
        match PayloadDumpWriters::open(&key.user_id, state.conn_start_time) {
            Ok(writers) => state.writers = Some(writers),
            Err(error) => warn!(
                user_id = %key.user_id,
                conn_start_time = state.conn_start_time,
                error = %error,
                "failed to initialize server media RTP payload dump"
            ),
        }
    }

    pub(crate) fn dump_inbound(&self, packet: &ServerMediaRtpPacket) {
        self.dump(
            PayloadDumpDirection::Inbound,
            packet.payload_type,
            packet.sequence_number,
            packet.timestamp,
            &packet.payload,
        );
    }

    pub(crate) fn dump_outbound(&self, packet: &ServerMediaEgressRtpPacket) {
        self.dump(
            PayloadDumpDirection::Outbound,
            packet.payload_type,
            packet.sequence_number,
            packet.timestamp,
            &packet.payload,
        );
    }

    fn dump(
        &self,
        direction: PayloadDumpDirection,
        payload_type: u8,
        sequence_number: u16,
        timestamp: u32,
        payload: &[u8],
    ) {
        let mut state = self
            .inner
            .lock()
            .expect("payload dumper lock must not be poisoned");
        let Some(writers) = state.writers.as_mut() else {
            return;
        };
        if let Err(error) = writers.writer(direction).write_record(
            payload_type,
            sequence_number,
            timestamp,
            payload,
        ) {
            warn!(
                direction = direction.as_str(),
                error = %error,
                "failed to write server media RTP payload dump"
            );
        }
    }
}

impl PayloadDumpWriters {
    fn open(user_id: &UserId, conn_start_time: u128) -> io::Result<Self> {
        Ok(Self {
            inbound: PayloadDumpWriter::open(
                user_id,
                conn_start_time,
                PayloadDumpDirection::Inbound,
            )?,
            outbound: PayloadDumpWriter::open(
                user_id,
                conn_start_time,
                PayloadDumpDirection::Outbound,
            )?,
        })
    }

    fn writer(&mut self, direction: PayloadDumpDirection) -> &mut PayloadDumpWriter {
        match direction {
            PayloadDumpDirection::Inbound => &mut self.inbound,
            PayloadDumpDirection::Outbound => &mut self.outbound,
        }
    }
}

impl PayloadDumpWriter {
    fn open(
        user_id: &UserId,
        conn_start_time: u128,
        direction: PayloadDumpDirection,
    ) -> io::Result<Self> {
        let path =
            env::current_dir()?.join(payload_dump_filename(user_id, conn_start_time, direction));
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self { file })
    }

    fn write_record(
        &mut self,
        payload_type: u8,
        sequence_number: u16,
        timestamp: u32,
        payload: &[u8],
    ) -> io::Result<()> {
        let payload_len = u32::try_from(payload.len()).expect("RTP payload length must fit in u32");
        self.file.write_all(&[payload_type])?;
        self.file.write_all(&sequence_number.to_be_bytes())?;
        self.file.write_all(&timestamp.to_be_bytes())?;
        self.file.write_all(&payload_len.to_be_bytes())?;
        self.file.write_all(payload)?;
        self.file.flush()?;
        Ok(())
    }
}

impl PayloadDumpDirection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
        }
    }
}

fn connection_start_time_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must not be before UNIX epoch")
        .as_millis()
}

fn sanitize_filename(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn payload_dump_filename(
    user_id: &UserId,
    conn_start_time: u128,
    direction: PayloadDumpDirection,
) -> String {
    format!(
        "{}_{}_{}.payload.bin",
        sanitize_filename(user_id.as_str()),
        conn_start_time,
        direction.as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyre_core::RoomId;
    use std::fs;

    #[test]
    fn sanitize_filename_replaces_path_separators() {
        assert_eq!(sanitize_filename("user/../a:b"), "user____a_b");
    }

    #[test]
    fn direction_names_are_file_safe() {
        assert_eq!(PayloadDumpDirection::Inbound.as_str(), "inbound");
        assert_eq!(PayloadDumpDirection::Outbound.as_str(), "outbound");
    }

    #[test]
    fn disabled_dumper_does_not_open_writers() {
        let dumper = PayloadDumper::disabled_for_test();

        dumper.set_session_key(&ServerMediaSessionKey {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("user_01"),
        });

        assert!(dumper
            .inner
            .lock()
            .expect("payload dumper lock must not be poisoned")
            .writers
            .is_none());
    }

    #[test]
    fn payload_dump_filename_contains_user_start_time_and_direction() {
        assert_eq!(
            payload_dump_filename(
                &UserId::from_external("user/01"),
                123,
                PayloadDumpDirection::Inbound
            ),
            "user_01_123_inbound.payload.bin"
        );
    }

    #[test]
    fn writer_writes_parseable_payload_records() {
        let dir = env::temp_dir().join(format!(
            "lyre-payload-dump-test-{}",
            connection_start_time_millis()
        ));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("payload.bin");
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        let mut writer = PayloadDumpWriter { file };
        writer.write_record(111, 7, 9_600, &[1, 2, 3]).unwrap();

        let bytes = fs::read(path).unwrap();
        fs::remove_dir_all(dir).unwrap();

        assert_eq!(bytes.len(), PAYLOAD_DUMP_RECORD_HEADER_LEN + 3);
        assert_eq!(bytes[0], 111);
        assert_eq!(u16::from_be_bytes([bytes[1], bytes[2]]), 7);
        assert_eq!(
            u32::from_be_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]),
            9_600
        );
        assert_eq!(
            u32::from_be_bytes([bytes[7], bytes[8], bytes[9], bytes[10]]),
            3
        );
        assert_eq!(&bytes[PAYLOAD_DUMP_RECORD_HEADER_LEN..], &[1, 2, 3]);
    }
}
