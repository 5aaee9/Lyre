use std::collections::BTreeMap;

use crate::{ServerMediaRtpPacket, SERVER_MEDIA_OPUS_FRAME_SIZE};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMediaConcealmentRequired {
    pub track_id: String,
    pub sequence_number: u16,
    pub rtp_timestamp: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerMediaJitterBufferOutput {
    Packet(ServerMediaRtpPacket),
    ConcealmentRequired(ServerMediaConcealmentRequired),
}

#[derive(Debug)]
pub struct ServerMediaJitterBuffer {
    pending: BTreeMap<u16, ServerMediaRtpPacket>,
    next_sequence: Option<u16>,
    next_timestamp: Option<u32>,
    max_depth: usize,
    track_id: Option<String>,
}

impl Default for ServerMediaJitterBuffer {
    fn default() -> Self {
        Self {
            pending: BTreeMap::new(),
            next_sequence: None,
            next_timestamp: None,
            max_depth: 3,
            track_id: None,
        }
    }
}

impl ServerMediaJitterBuffer {
    pub fn push(&mut self, packet: ServerMediaRtpPacket) -> Vec<ServerMediaJitterBufferOutput> {
        if self.next_sequence.is_none() {
            self.next_sequence = Some(packet.sequence_number);
            self.next_timestamp = Some(packet.timestamp);
            self.track_id = Some(packet.track_id.clone());
        }

        let next_sequence = self
            .next_sequence
            .expect("jitter buffer next sequence initialized");
        if sequence_distance(next_sequence, packet.sequence_number) < 0 {
            return Vec::new();
        }

        self.pending.entry(packet.sequence_number).or_insert(packet);
        self.drain_ready()
    }

    fn drain_ready(&mut self) -> Vec<ServerMediaJitterBufferOutput> {
        let mut outputs = Vec::new();

        while let Some(sequence) = self.next_sequence {
            if let Some(packet) = self.pending.remove(&sequence) {
                self.next_sequence = Some(sequence.wrapping_add(1));
                self.next_timestamp = Some(
                    packet
                        .timestamp
                        .wrapping_add(SERVER_MEDIA_OPUS_FRAME_SIZE as u32),
                );
                outputs.push(ServerMediaJitterBufferOutput::Packet(packet));
                continue;
            }

            if self.pending.len() <= self.max_depth {
                break;
            }

            let rtp_timestamp = self
                .next_timestamp
                .expect("jitter buffer timestamp initialized");
            if let Some(packet) = self.pending.values().next() {
                let expected_timestamp_gap = sequence_distance(sequence, packet.sequence_number)
                    as u32
                    * SERVER_MEDIA_OPUS_FRAME_SIZE as u32;
                if timestamp_distance(rtp_timestamp, packet.timestamp)
                    > expected_timestamp_gap as i32
                {
                    self.next_sequence = Some(packet.sequence_number);
                    self.next_timestamp = Some(packet.timestamp);
                    continue;
                }
            }
            outputs.push(ServerMediaJitterBufferOutput::ConcealmentRequired(
                ServerMediaConcealmentRequired {
                    track_id: self
                        .track_id
                        .clone()
                        .expect("jitter buffer track id initialized"),
                    sequence_number: sequence,
                    rtp_timestamp,
                },
            ));
            self.next_sequence = Some(sequence.wrapping_add(1));
            self.next_timestamp =
                Some(rtp_timestamp.wrapping_add(SERVER_MEDIA_OPUS_FRAME_SIZE as u32));
        }

        outputs
    }
}

fn sequence_distance(from: u16, to: u16) -> i16 {
    to.wrapping_sub(from) as i16
}

fn timestamp_distance(from: u32, to: u32) -> i32 {
    to.wrapping_sub(from) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ServerMediaRtpPacket;

    fn packet(sequence_number: u16, timestamp: u32) -> ServerMediaRtpPacket {
        ServerMediaRtpPacket {
            track_id: "audio-main".to_owned(),
            sequence_number,
            timestamp,
            marker: true,
            payload_type: 111,
            payload: vec![sequence_number as u8],
        }
    }

    fn emitted_sequences(outputs: &[ServerMediaJitterBufferOutput]) -> Vec<u16> {
        outputs
            .iter()
            .filter_map(|output| match output {
                ServerMediaJitterBufferOutput::Packet(packet) => Some(packet.sequence_number),
                ServerMediaJitterBufferOutput::ConcealmentRequired(_) => None,
            })
            .collect()
    }

    #[test]
    fn emits_in_order_packets_immediately() {
        let mut buffer = ServerMediaJitterBuffer::default();

        assert_eq!(emitted_sequences(&buffer.push(packet(7, 960))), vec![7]);
        assert_eq!(emitted_sequences(&buffer.push(packet(8, 1920))), vec![8]);
    }

    #[test]
    fn reorders_packets_once_gap_arrives() {
        let mut buffer = ServerMediaJitterBuffer::default();

        assert_eq!(emitted_sequences(&buffer.push(packet(10, 960))), vec![10]);
        assert!(buffer.push(packet(12, 2880)).is_empty());
        assert_eq!(
            emitted_sequences(&buffer.push(packet(11, 1920))),
            vec![11, 12]
        );
    }

    #[test]
    fn drops_duplicates_and_stale_packets() {
        let mut buffer = ServerMediaJitterBuffer::default();

        assert_eq!(emitted_sequences(&buffer.push(packet(1, 960))), vec![1]);
        assert!(buffer.push(packet(1, 960)).is_empty());
        assert_eq!(emitted_sequences(&buffer.push(packet(2, 1920))), vec![2]);
        assert!(buffer.push(packet(1, 960)).is_empty());
    }

    #[test]
    fn records_concealment_when_gap_exceeds_depth() {
        let mut buffer = ServerMediaJitterBuffer::default();

        assert_eq!(
            emitted_sequences(&buffer.push(packet(20, 20_000))),
            vec![20]
        );
        assert!(buffer.push(packet(22, 21_920)).is_empty());
        assert!(buffer.push(packet(23, 22_880)).is_empty());
        assert!(buffer.push(packet(24, 23_840)).is_empty());
        let outputs = buffer.push(packet(25, 24_800));

        assert!(matches!(
            &outputs[0],
            ServerMediaJitterBufferOutput::ConcealmentRequired(event)
                if event.sequence_number == 21 && event.rtp_timestamp == 20_960
        ));
        assert_eq!(emitted_sequences(&outputs), vec![22, 23, 24, 25]);
    }

    #[test]
    fn skips_concealment_when_timestamp_gap_indicates_discontinuous_silence() {
        let mut buffer = ServerMediaJitterBuffer::default();

        assert_eq!(
            emitted_sequences(&buffer.push(packet(20, 20_000))),
            vec![20]
        );
        assert!(buffer.push(packet(22, 40_000)).is_empty());
        assert!(buffer.push(packet(23, 40_960)).is_empty());
        assert!(buffer.push(packet(24, 41_920)).is_empty());
        let outputs = buffer.push(packet(25, 42_880));

        assert!(outputs.iter().all(|output| !matches!(
            output,
            ServerMediaJitterBufferOutput::ConcealmentRequired(_)
        )));
        assert_eq!(emitted_sequences(&outputs), vec![22, 23, 24, 25]);
    }

    #[test]
    fn handles_sequence_wraparound() {
        let mut buffer = ServerMediaJitterBuffer::default();

        assert_eq!(
            emitted_sequences(&buffer.push(packet(65_534, u32::MAX - 959))),
            vec![65_534]
        );
        assert_eq!(
            emitted_sequences(&buffer.push(packet(65_535, 0))),
            vec![65_535]
        );
        assert_eq!(emitted_sequences(&buffer.push(packet(0, 960))), vec![0]);
        assert_eq!(emitted_sequences(&buffer.push(packet(1, 1920))), vec![1]);
    }

    #[test]
    fn records_multiple_gap_concealment_events_with_incrementing_timestamps() {
        let mut buffer = ServerMediaJitterBuffer::default();

        assert_eq!(
            emitted_sequences(&buffer.push(packet(40, 40_000))),
            vec![40]
        );
        for (sequence, timestamp) in [(43, 42_880), (44, 43_840), (45, 44_800)] {
            let _ = buffer.push(packet(sequence, timestamp));
        }
        let outputs = buffer.push(packet(46, 45_760));
        let gaps = outputs
            .iter()
            .filter_map(|output| match output {
                ServerMediaJitterBufferOutput::ConcealmentRequired(event) => {
                    Some((event.sequence_number, event.rtp_timestamp))
                }
                ServerMediaJitterBufferOutput::Packet(_) => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(gaps, vec![(41, 40_960), (42, 41_920)]);
        assert_eq!(emitted_sequences(&outputs), vec![43, 44, 45, 46]);
    }

    #[test]
    fn concealment_timestamps_wrap_with_u32_addition() {
        let mut buffer = ServerMediaJitterBuffer::default();

        assert_eq!(
            emitted_sequences(&buffer.push(packet(100, u32::MAX - 479))),
            vec![100]
        );
        for sequence in [102, 103, 104] {
            let _ = buffer.push(packet(sequence, 0));
        }
        let outputs = buffer.push(packet(105, 960));

        assert!(matches!(
            &outputs[0],
            ServerMediaJitterBufferOutput::ConcealmentRequired(event)
                if event.sequence_number == 101 && event.rtp_timestamp == 480
        ));
    }
}
