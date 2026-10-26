use crate::SERVER_MEDIA_OPUS_FRAME_SIZE;
use std::time::Instant;

const RTP_CLOCK_RATE_PER_MS: u64 = 48;
/// Wall-clock lag beyond which the outgoing timeline jumps forward instead of compressing
/// the pause, so the receiver does not treat the resumed talk spurt as late audio.
const MAX_TIMESTAMP_LAG: u32 = 6 * SERVER_MEDIA_OPUS_FRAME_SIZE as u32;

/// Owns the sequence number and timestamp of one outgoing egress stream (one source on one
/// recipient peer connection). Source sequence numbers and timestamps restart whenever the
/// source reconnects, so forwarding them unchanged on a long-lived egress SSRC makes the
/// receiver drop the new packets as replays or late audio.
#[derive(Debug)]
pub(crate) struct EgressRtpClock {
    sequence_number: u16,
    timestamp: u32,
    origin: Option<(Instant, u32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EgressRtpStamp {
    pub(crate) sequence_number: u16,
    pub(crate) timestamp: u32,
    pub(crate) marker: bool,
}

impl EgressRtpClock {
    pub(crate) fn new(sequence_number: u16, timestamp: u32) -> Self {
        Self {
            sequence_number,
            timestamp,
            origin: None,
        }
    }

    pub(crate) fn next(&mut self, now: Instant) -> EgressRtpStamp {
        let marker = match self.origin {
            None => {
                self.origin = Some((now, self.timestamp));
                true
            }
            Some((origin_instant, origin_timestamp)) => {
                let elapsed_ms = now.saturating_duration_since(origin_instant).as_millis() as u64;
                let wall_timestamp =
                    origin_timestamp.wrapping_add((elapsed_ms * RTP_CLOCK_RATE_PER_MS) as u32);
                let lag = wall_timestamp.wrapping_sub(self.timestamp);
                let behind_wall_clock = lag < u32::MAX / 2 && lag > MAX_TIMESTAMP_LAG;
                if behind_wall_clock {
                    self.timestamp = wall_timestamp;
                }
                behind_wall_clock
            }
        };
        let stamp = EgressRtpStamp {
            sequence_number: self.sequence_number,
            timestamp: self.timestamp,
            marker,
        };
        self.sequence_number = self.sequence_number.wrapping_add(1);
        self.timestamp = self
            .timestamp
            .wrapping_add(SERVER_MEDIA_OPUS_FRAME_SIZE as u32);
        stamp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const FRAME: u32 = SERVER_MEDIA_OPUS_FRAME_SIZE as u32;

    #[test]
    fn continuous_packets_advance_one_frame_with_marker_only_on_first() {
        let start = Instant::now();
        let mut clock = EgressRtpClock::new(65_535, 1_000);

        let first = clock.next(start);
        let second = clock.next(start + Duration::from_millis(20));
        let third = clock.next(start + Duration::from_millis(40));

        assert_eq!(
            first,
            EgressRtpStamp {
                sequence_number: 65_535,
                timestamp: 1_000,
                marker: true
            }
        );
        assert_eq!(
            (second.sequence_number, second.timestamp),
            (0, 1_000 + FRAME)
        );
        assert!(!second.marker);
        assert_eq!(third.timestamp, 1_000 + 2 * FRAME);
    }

    #[test]
    fn burst_after_short_delay_keeps_timestamps_contiguous() {
        let start = Instant::now();
        let mut clock = EgressRtpClock::new(0, 0);
        clock.next(start);

        let delayed = start + Duration::from_millis(100);
        let stamps: Vec<_> = (0..5).map(|_| clock.next(delayed)).collect();

        for (index, stamp) in stamps.iter().enumerate() {
            assert_eq!(stamp.timestamp, (index as u32 + 1) * FRAME);
            assert!(!stamp.marker);
        }
    }

    #[test]
    fn pause_moves_timestamp_to_wall_clock_and_keeps_sequence_contiguous() {
        let start = Instant::now();
        let mut clock = EgressRtpClock::new(10, u32::MAX - FRAME);
        clock.next(start);

        let resumed = clock.next(start + Duration::from_secs(2));
        let following = clock.next(start + Duration::from_millis(2_020));

        assert_eq!(resumed.sequence_number, 11);
        assert_eq!(resumed.timestamp, (u32::MAX - FRAME).wrapping_add(96_000));
        assert!(resumed.marker);
        assert_eq!(following.sequence_number, 12);
        assert_eq!(following.timestamp, resumed.timestamp.wrapping_add(FRAME));
        assert!(!following.marker);
    }
}
