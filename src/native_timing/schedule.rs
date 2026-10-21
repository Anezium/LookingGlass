//! One immutable residence-age window. All file/anchor work is before S-10s.
//! Submitted tokens identify a rendered frame, never physical camera exposure.
use super::{State, Text};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const WINDOW_US: u64 = 30_000_000;
pub const GUARD_US: u64 = 2_000_000;
pub const PRE_GUARD_US: u64 = 12_000_000;
pub const REVIEW_US: u64 = 10_000_000;
pub const TOKEN_CAP: usize = 128;
pub const TOKEN_MAX_AGE_US: u64 = 60_000_000;
pub const POLL_US: u64 = 500_000;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub map: String,
    pub ready_offset_us: u64,
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.map.is_empty() && self.map.len() <= 64, "schedule map");
        anyhow::ensure!(
            (20_000_000..=600_000_000).contains(&self.ready_offset_us),
            "schedule offset must be predeclared 20..600s"
        );
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Token {
    pub ordinal: u64,
    pub viewer_seq: u64,
    pub submitted_us: u64,
    pub generation: u64,
    pub operation: u64,
    pub auth: [u8; 16],
}
#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub ordinal: u64,
    pub viewer_seq: u64,
    pub submitted_us: u64,
    pub generation: u64,
    pub operation: u64,
    pub auth: [u8; 16],
}
impl From<Token> for Reference {
    fn from(t: Token) -> Self {
        Self {
            ordinal: t.ordinal,
            viewer_seq: t.viewer_seq,
            submitted_us: t.submitted_us,
            generation: t.generation,
            operation: t.operation,
            auth: t.auth,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ack {
    pub run_id: [u8; 16],
    pub token: Reference,
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub enum Fault {
    Clock,
    Entropy,
    Authentication,
    WrongMap,
    Generation,
    Overflow,
    LateReview,
    InvalidReview,
    RepeatedReview,
    Interrupted,
    Ineligible,
    StartLate,
    EndLate,
}
#[derive(Serialize)]
pub struct Schedule {
    pub schema: &'static str,
    pub expected_map: Text<64>,
    pub ready_offset_us: u64,
    pub ready_us: Option<u64>,
    pub generation: Option<u64>,
    pub start_us: Option<u64>,
    pub end_us: Option<u64>,
    pub review_deadline_us: Option<u64>,
    pub review_ack_us: Option<u64>,
    pub reviewed_token: Option<Token>,
    /// Ring in slot order, ordinal % TOKEN_CAP. Not sorted/reconstructed at export.
    pub tokens: Vec<Token>,
    pub token_total: u64,
    pub token_max_age_us: u64,
    pub acquired_begin_us: Option<u64>,
    pub seal_request_us: Option<u64>,
    pub start_state: Option<State>,
    pub end_state: Option<State>,
    pub fault: Option<Fault>,
    pub token_mac_kat: bool,
    pub token_authentication: &'static str,
    pub physical_image_time: &'static str,
    pub campaign_qualified: bool,
    #[serde(skip)]
    next_poll_us: u64,
    #[serde(skip)]
    token_key: Option<[u8; 32]>,
    #[serde(skip)]
    pub started: bool,
    #[serde(skip)]
    pub finished: bool,
}
impl Schedule {
    pub fn new(cfg: Config) -> Self {
        let token_mac_kat = mac_self_test();
        let token_key = super::export::random_id()
            .zip(super::export::random_id())
            .map(|(a, b)| {
                let mut key = [0; 32];
                key[..16].copy_from_slice(&a);
                key[16..].copy_from_slice(&b);
                key
            });
        Self {
            schema: "alice-native-window-v2",
            expected_map: Text::new(&cfg.map).unwrap(),
            ready_offset_us: cfg.ready_offset_us,
            ready_us: None,
            generation: None,
            start_us: None,
            end_us: None,
            review_deadline_us: None,
            review_ack_us: None,
            reviewed_token: None,
            tokens: Vec::with_capacity(TOKEN_CAP),
            token_total: 0,
            token_max_age_us: TOKEN_MAX_AGE_US,
            acquired_begin_us: None,
            seal_request_us: None,
            start_state: None,
            end_state: None,
            fault: if !token_mac_kat {
                Some(Fault::Authentication)
            } else if token_key.is_some() {
                None
            } else {
                Some(Fault::Entropy)
            },
            token_mac_kat,
            token_authentication:
                "HMAC-SHA256/128; per-process random key withheld; native receipt UNATTESTED",
            physical_image_time: "UNKNOWN; submitted token is not camera exposure time",
            campaign_qualified: false,
            next_poll_us: 0,
            token_key,
            started: false,
            finished: false,
        }
    }
    fn authenticate(&self, t: Reference) -> Option<[u8; 16]> {
        let key = self.token_key?;
        let mut message = [0u8; 48];
        message[..8].copy_from_slice(b"LGNTW001");
        for (i, v) in [
            t.ordinal,
            t.viewer_seq,
            t.submitted_us,
            t.generation,
            t.operation,
        ]
        .iter()
        .enumerate()
        {
            message[8 + i * 8..16 + i * 8].copy_from_slice(&v.to_le_bytes());
        }
        Some(hmac(&key, &message)[..16].try_into().unwrap())
    }
    pub fn storage_bytes(&self) -> usize {
        self.tokens.capacity() * std::mem::size_of::<Token>()
    }
    pub fn reject(&mut self, reason: Fault) {
        if self.fault.is_none() {
            self.fault = Some(reason)
        }
    }
    pub fn ready(&mut self, map: &str, generation: u64, now: Option<u64>) {
        if self.ready_us.is_some() {
            self.reject(Fault::Generation);
            return;
        }
        if map.as_bytes() != &self.expected_map.bytes[..self.expected_map.len] {
            self.reject(Fault::WrongMap);
            return;
        }
        let Some(now) = now else {
            self.reject(Fault::Clock);
            return;
        };
        let Some(start) = now.checked_add(self.ready_offset_us) else {
            self.reject(Fault::Overflow);
            return;
        };
        let Some(end) = start.checked_add(WINDOW_US) else {
            self.reject(Fault::Overflow);
            return;
        };
        self.ready_us = Some(now);
        self.generation = Some(generation);
        self.start_us = Some(start);
        self.end_us = Some(end);
        self.review_deadline_us = Some(start - REVIEW_US);
    }
    pub fn review_open(&self, now: u64) -> bool {
        self.fault.is_none()
            && self.review_ack_us.is_none()
            && self.review_deadline_us.is_some_and(|d| now <= d)
    }
    pub fn poll_due(&mut self, now: u64) -> bool {
        if !self.review_open(now) || now < self.next_poll_us {
            return false;
        }
        self.next_poll_us = now.saturating_add(POLL_US);
        true
    }
    /// Called only after the corresponding token overlay is submitted.
    pub fn submit(
        &mut self,
        now: u64,
        seq: u64,
        generation: u64,
        operation: u64,
        _state: State,
    ) -> Option<Token> {
        if !self.review_open(now) || seq == 0 || self.latest().is_some_and(|t| t.viewer_seq == seq)
        {
            return None;
        }
        let Some(ordinal) = self.token_total.checked_add(1) else {
            self.reject(Fault::Overflow);
            return None;
        };
        self.token_total = ordinal;
        let mut t = Token {
            ordinal,
            viewer_seq: seq,
            submitted_us: now,
            generation,
            operation,
            auth: [0; 16],
        };
        t.auth = self.authenticate(Reference::from(t))?;
        let slot = ((ordinal - 1) % TOKEN_CAP as u64) as usize;
        if self.tokens.len() < TOKEN_CAP {
            self.tokens.push(t);
        } else {
            self.tokens[slot] = t;
        }
        Some(t)
    }
    pub fn latest(&self) -> Option<Token> {
        self.token_total
            .checked_sub(1)
            .and_then(|n| self.tokens.get((n % TOKEN_CAP as u64) as usize))
            .copied()
    }
    pub fn acknowledge(
        &mut self,
        ack: Ack,
        run_id: Option<[u8; 16]>,
        now: u64,
        generation: u64,
        operation: u64,
    ) {
        if self.review_ack_us.is_some() {
            self.reject(Fault::RepeatedReview);
            return;
        }
        if !self.review_open(now) {
            self.reject(Fault::LateReview);
            return;
        }
        if Some(ack.run_id) != run_id
            || run_id == Some([0; 16])
            || ack.token.ordinal == 0
            || ack.token.ordinal > self.token_total
            || self.authenticate(ack.token).is_none_or(|tag| {
                !tag.iter()
                    .zip(ack.token.auth)
                    .fold(0u8, |diff, (a, b)| diff | (*a ^ b))
                    .eq(&0)
            })
            || ack.token.generation != generation
            || Some(generation) != self.generation
            || ack.token.operation != operation
            || ack.token.submitted_us >= now
            || now.saturating_sub(ack.token.submitted_us) > TOKEN_MAX_AGE_US
        {
            self.reject(Fault::InvalidReview);
            return;
        }
        self.reviewed_token = Some(Token {
            ordinal: ack.token.ordinal,
            viewer_seq: ack.token.viewer_seq,
            submitted_us: ack.token.submitted_us,
            generation: ack.token.generation,
            operation: ack.token.operation,
            auth: ack.token.auth,
        });
        self.review_ack_us = Some(now);
    }
    /// Main-thread clock only; window never moves after a late/missing review.
    pub fn tick(&mut self, now: Option<u64>, generation: u64, state: State) -> (bool, bool) {
        if self.finished {
            return (false, false);
        }
        let Some(now) = now else {
            self.reject(Fault::Clock);
            self.finished = true;
            return (false, self.started);
        };
        if self.generation.is_some_and(|g| g != generation) {
            self.reject(Fault::Generation);
        }
        if self.review_deadline_us.is_some_and(|d| now > d) && self.review_ack_us.is_none() {
            self.reject(Fault::LateReview);
        }
        if self.fault.is_some() {
            self.finished = true;
            return (false, self.started);
        }
        let (Some(s), Some(t)) = (self.start_us, self.end_us) else {
            return (false, false);
        };
        let mut start = false;
        if !self.started && now >= s - PRE_GUARD_US {
            if now >= s - REVIEW_US {
                self.reject(Fault::StartLate);
                self.finished = true;
                return (false, false);
            }
            self.started = true;
            self.acquired_begin_us = Some(now);
            start = true;
        }
        if now >= s && self.start_state.is_none() {
            self.start_state = Some(state);
        }
        if now >= t && self.end_state.is_none() {
            self.end_state = Some(state);
        }
        if self.started && now >= t + GUARD_US {
            if now > t + GUARD_US + 1_000_000 {
                self.reject(Fault::EndLate);
            }
            self.finished = true;
            self.seal_request_us = Some(now);
            return (start, true);
        }
        (start, false)
    }
}
// Fixed-size HMAC-SHA256. Key generated once before world installation, never
// exported. This authenticates issued submitted-frame references, not photographs.
fn hmac(key: &[u8; 32], message: &[u8]) -> [u8; 32] {
    let mut pad = [0x36u8; 64];
    for (p, k) in pad.iter_mut().zip(key) {
        *p ^= *k;
    }
    let mut inner = Sha256::new();
    inner.update(pad);
    inner.update(message);
    let digest = inner.finalize();
    pad.fill(0x5c);
    for (p, k) in pad.iter_mut().zip(key) {
        *p ^= *k;
    }
    let mut outer = Sha256::new();
    outer.update(pad);
    outer.update(digest);
    outer.finalize().into()
}
fn mac_self_test() -> bool {
    let mut key = [0u8; 32];
    key[..20].fill(0x0b);
    hmac(&key, b"Hi There")
        == [
            0xb0, 0x34, 0x4c, 0x61, 0xd8, 0xdb, 0x38, 0x53, 0x5c, 0xa8, 0xaf, 0xce, 0xaf, 0x0b,
            0xf1, 0x2b, 0x88, 0x1d, 0xc2, 0x00, 0xc9, 0x83, 0x3d, 0xa7, 0x26, 0xe9, 0x37, 0x6c,
            0x2e, 0x32, 0xcf, 0xf7,
        ]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hmac_sha256_full_standard_vector() {
        let mut key = [0u8; 32];
        key[..20].fill(0x0b);
        assert_eq!(
            hmac(&key, b"Hi There"),
            [
                0xb0, 0x34, 0x4c, 0x61, 0xd8, 0xdb, 0x38, 0x53, 0x5c, 0xa8, 0xaf, 0xce, 0xaf, 0x0b,
                0xf1, 0x2b, 0x88, 0x1d, 0xc2, 0x00, 0xc9, 0x83, 0x3d, 0xa7, 0x26, 0xe9, 0x37, 0x6c,
                0x2e, 0x32, 0xcf, 0xf7
            ]
        );
    }
    #[test]
    fn each_submitted_frame_has_unique_authenticated_reference_even_after_ring_wrap() {
        let mut p = plan();
        let first = p.submit(1_000_000, 1, 1, 2, State::default()).unwrap();
        assert!(p.submit(1_000_001, 1, 1, 2, State::default()).is_none());
        for i in 2..=200 {
            let next = p
                .submit(1_000_000 + i * 1000, i, 1, 2, State::default())
                .unwrap();
            assert_ne!(next.auth, first.auth);
        }
        assert!(!p.tokens.contains(&first));
        p.acknowledge(
            Ack {
                run_id: [7; 16],
                token: first.into(),
            },
            Some([7; 16]),
            2_000_000,
            1,
            2,
        );
        assert_eq!(p.reviewed_token, Some(first));
        assert!(p.fault.is_none());
    }
    #[test]
    fn authenticated_field_and_tag_tampering_rejects_without_a_later_ring_lookup() {
        for field in 0..6 {
            let mut p = plan();
            let mut t = p.submit(1_000_000, 1, 1, 2, State::default()).unwrap();
            match field {
                0 => t.ordinal += 1,
                1 => t.viewer_seq += 1,
                2 => t.submitted_us += 1,
                3 => t.generation += 1,
                4 => t.operation += 1,
                _ => t.auth[15] ^= 1,
            }
            p.acknowledge(
                Ack {
                    run_id: [7; 16],
                    token: t.into(),
                },
                Some([7; 16]),
                2_000_000,
                1,
                2,
            );
            assert_eq!(p.fault, Some(Fault::InvalidReview));
        }
    }
    fn plan() -> Schedule {
        let mut p = Schedule::new(Config {
            map: "tower2".into(),
            ready_offset_us: 40_000_000,
        });
        p.ready("tower2", 1, Some(1_000_000));
        p
    }
    fn ack(p: &mut Schedule) {
        let t = p.submit(2_000_000, 4, 1, 2, State::default()).unwrap();
        p.acknowledge(
            Ack {
                run_id: [7; 16],
                token: t.into(),
            },
            Some([7; 16]),
            3_000_000,
            1,
            2,
        );
    }
    #[test]
    fn exact_residence_age_and_bounded_start_seal() {
        let mut p = plan();
        ack(&mut p);
        assert_eq!(
            p.tick(Some(28_999_999), 1, State::default()),
            (false, false)
        );
        assert_eq!(p.tick(Some(29_000_000), 1, State::default()), (true, false));
        assert_eq!(p.tick(Some(73_000_000), 1, State::default()), (false, true));
        assert_eq!(p.end_us, Some(71_000_000));
        assert_eq!(
            p.tick(Some(900_000_000), 1, State::default()),
            (false, false)
        );
        assert_eq!(p.seal_request_us, Some(73_000_000));
    }
    #[test]
    fn late_or_missing_review_never_slides() {
        let mut p = plan();
        assert_eq!(
            p.tick(Some(31_000_001), 1, State::default()),
            (false, false)
        );
        assert_eq!(p.fault, Some(Fault::LateReview));
        assert_eq!(p.start_us, Some(41_000_000));
    }
    #[test]
    fn wrong_generation_map_and_late_start_reject() {
        let mut p = plan();
        ack(&mut p);
        p.tick(Some(10_000_000), 2, State::default());
        assert_eq!(p.fault, Some(Fault::Generation));
        let mut p = plan();
        p.ready("tower2", 2, Some(10));
        assert_eq!(p.fault, Some(Fault::Generation));
        let mut p = plan();
        ack(&mut p);
        p.tick(Some(41_000_000), 1, State::default());
        assert_eq!(p.fault, Some(Fault::StartLate));
    }
    #[test]
    fn tokens_are_bounded_and_replay_state_or_run_mismatch_reject() {
        let mut p = plan();
        let old = p.submit(1_000_000, 1, 1, 2, State::default()).unwrap();
        for i in 1..=TOKEN_CAP {
            p.submit(
                1_000_000 + i as u64 * POLL_US,
                i as u64 + 1,
                1,
                2,
                State::default(),
            );
        }
        assert!(p.tokens.capacity() == TOKEN_CAP);
        p.acknowledge(
            Ack {
                run_id: [8; 16],
                token: old.into(),
            },
            Some([7; 16]),
            30_000_000,
            1,
            2,
        );
        assert!(p.fault.is_some());
    }
    #[test]
    fn polling_and_tokens_stop_before_window() {
        let mut p = plan();
        assert!(p.poll_due(1_000_000));
        assert!(!p.poll_due(1_000_001));
        ack(&mut p);
        assert!(!p.poll_due(4_000_000));
        assert!(p.submit(40_000_000, 100, 1, 2, State::default()).is_none());
        assert_eq!(p.token_total, 1);
    }
    #[test]
    fn overflow_clock_and_late_finish_are_explicit() {
        let mut p = Schedule::new(Config {
            map: "tower2".into(),
            ready_offset_us: 40_000_000,
        });
        p.ready("tower2", 1, Some(u64::MAX));
        assert_eq!(p.fault, Some(Fault::Overflow));
        let mut p = plan();
        ack(&mut p);
        p.tick(Some(29_000_000), 1, State::default());
        p.tick(None, 1, State::default());
        assert_eq!(p.fault, Some(Fault::Clock));
        let mut p = plan();
        ack(&mut p);
        p.tick(Some(29_000_000), 1, State::default());
        p.tick(Some(75_000_000), 1, State::default());
        assert_eq!(p.fault, Some(Fault::EndLate));
    }
}
