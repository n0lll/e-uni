//! First short-supply recovery detector (`T_recover`).
//!
//! Window at boundary `t` is `[t-W, t)`. Eligible only when `t >= L+W`.
//! ΔP uses signed `P = E_old + E_new + 8C` (never unsigned saturating sub).

use std::collections::VecDeque;

use universe_types::{CanonReader, CanonWriter, NumericFault};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoverStatus {
    Recovered,
    Died,
    NotRecoveredByH,
    EngineeringError,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecoverOutcome {
    pub status: RecoverStatus,
    pub success: bool,
    pub first_recover_t: Option<u64>,
    pub restricted_time: u64,
    pub died_at: Option<u64>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TickSample {
    pub t_rel: u64,
    pub world_tick_after: u64,
    pub living: bool,
    pub photo_extent: u64,
    pub p: i128,
    pub debt: u64,
    pub debt_accrued: u64,
    pub maint_due: u64,
    pub maint_paid: u64,
    pub maint_unpaid: u64,
    pub cognition_due: u64,
    pub cognition_paid: u64,
    pub decision_boundary: bool,
}

impl TickSample {
    pub fn encode(&self, w: &mut CanonWriter) {
        w.u64(self.t_rel);
        w.u64(self.world_tick_after);
        w.bool(self.living);
        w.u64(self.photo_extent);
        w.i128(self.p);
        w.u64(self.debt);
        w.u64(self.debt_accrued);
        w.u64(self.maint_due);
        w.u64(self.maint_paid);
        w.u64(self.maint_unpaid);
        w.u64(self.cognition_due);
        w.u64(self.cognition_paid);
        w.bool(self.decision_boundary);
    }

    pub fn decode(r: &mut CanonReader) -> Result<Self, NumericFault> {
        Ok(Self {
            t_rel: r.u64()?,
            world_tick_after: r.u64()?,
            living: r.bool()?,
            photo_extent: r.u64()?,
            p: r.i128()?,
            debt: r.u64()?,
            debt_accrued: r.u64()?,
            maint_due: r.u64()?,
            maint_paid: r.u64()?,
            maint_unpaid: r.u64()?,
            cognition_due: r.u64()?,
            cognition_paid: r.u64()?,
            decision_boundary: r.bool()?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct RecoverDetector {
    pub l: u64,
    pub w: u64,
    pub h: u64,
    pub t0_world: u64,
    pub p0: i128,
    pub living0: bool,
    buf: VecDeque<TickSample>,
    first_recover: Option<u64>,
    died_at: Option<u64>,
    engineering: Option<String>,
}

impl RecoverDetector {
    pub fn new(l: u64, w: u64, h: u64, t0_world: u64, p0: i128, living0: bool) -> Self {
        Self {
            l,
            w,
            h,
            t0_world,
            p0,
            living0,
            buf: VecDeque::new(),
            first_recover: None,
            died_at: None,
            engineering: None,
        }
    }

    pub fn fail_engineering(&mut self, msg: impl Into<String>) {
        if self.engineering.is_none() {
            self.engineering = Some(msg.into());
        }
    }

    pub fn first_recover_t(&self) -> Option<u64> {
        self.first_recover
    }

    pub fn died_at(&self) -> Option<u64> {
        self.died_at
    }

    pub fn on_tick(&mut self, s: TickSample) {
        if self.engineering.is_some() {
            return;
        }
        if !s.living && self.died_at.is_none() {
            self.died_at = Some(s.t_rel);
        }
        self.buf.push_back(s);
        let cap = self.w.saturating_add(1) as usize;
        while self.buf.len() > cap {
            self.buf.pop_front();
        }
        if let Some(last) = self.buf.back() {
            let t = last.t_rel;
            if self.first_recover.is_none() && t >= self.l.saturating_add(self.w) && t <= self.h {
                if self.window_recovers(t) {
                    self.first_recover = Some(t);
                }
            }
        }
    }

    fn sample_at(&self, t_rel: u64) -> Option<&TickSample> {
        self.buf.iter().find(|s| s.t_rel == t_rel)
    }

    fn p_at(&self, t_rel: u64) -> Option<i128> {
        if t_rel == 0 {
            return Some(self.p0);
        }
        self.sample_at(t_rel).map(|s| s.p)
    }

    fn living_at(&self, t_rel: u64) -> Option<bool> {
        if t_rel == 0 {
            return Some(self.living0);
        }
        self.sample_at(t_rel).map(|s| s.living)
    }

    /// Window ticks are the W completed steps with `t_rel ∈ (t-W, t]`.
    pub fn window_recovers(&self, t: u64) -> bool {
        if t < self.l.saturating_add(self.w) || self.w == 0 {
            return false;
        }
        let t_w = t - self.w;
        let p_t = match self.p_at(t) {
            Some(p) => p,
            None => return false,
        };
        let p_old = match self.p_at(t_w) {
            Some(p) => p,
            None => return false,
        };
        let dp = p_t - p_old;
        if dp < 0 {
            return false;
        }
        if self.living_at(t) != Some(true) || self.living_at(t_w) != Some(true) {
            return false;
        }
        let mut photo = 0u64;
        let mut n = 0u64;
        for s in &self.buf {
            if s.t_rel <= t_w || s.t_rel > t {
                continue;
            }
            n += 1;
            if !s.living {
                return false;
            }
            photo = photo.saturating_add(s.photo_extent);
            if s.debt_accrued > 0 {
                return false;
            }
            if s.maint_unpaid > 0 || s.maint_paid < s.maint_due {
                return false;
            }
            if s.decision_boundary && s.cognition_paid < s.cognition_due {
                return false;
            }
        }
        n == self.w && photo > 0
    }

    pub fn outcome(&self, t_now: u64) -> RecoverOutcome {
        if let Some(err) = &self.engineering {
            return RecoverOutcome {
                status: RecoverStatus::EngineeringError,
                success: false,
                first_recover_t: None,
                restricted_time: 0,
                died_at: self.died_at,
                error: Some(err.clone()),
            };
        }
        if let Some(t) = self.first_recover {
            return RecoverOutcome {
                status: RecoverStatus::Recovered,
                success: true,
                first_recover_t: Some(t),
                restricted_time: t,
                died_at: self.died_at,
                error: None,
            };
        }
        if self.died_at.is_some() && self.died_at.unwrap() <= self.h {
            return RecoverOutcome {
                status: RecoverStatus::Died,
                success: false,
                first_recover_t: None,
                restricted_time: self.h,
                died_at: self.died_at,
                error: None,
            };
        }
        if t_now >= self.h {
            RecoverOutcome {
                status: RecoverStatus::NotRecoveredByH,
                success: false,
                first_recover_t: None,
                restricted_time: self.h,
                died_at: self.died_at,
                error: None,
            }
        } else {
            RecoverOutcome {
                status: RecoverStatus::NotRecoveredByH,
                success: false,
                first_recover_t: None,
                restricted_time: self.h,
                died_at: self.died_at,
                error: None,
            }
        }
    }

    pub fn encode(&self, w: &mut CanonWriter) {
        w.raw(b"TR01");
        w.u64(self.l);
        w.u64(self.w);
        w.u64(self.h);
        w.u64(self.t0_world);
        w.i128(self.p0);
        w.bool(self.living0);
        w.option_u64(self.first_recover);
        w.option_u64(self.died_at);
        match &self.engineering {
            None => w.u8(0),
            Some(s) => {
                w.u8(1);
                w.blob(s.as_bytes());
            }
        }
        w.u64(self.buf.len() as u64);
        for s in &self.buf {
            s.encode(w);
        }
    }

    pub fn decode(r: &mut CanonReader) -> Result<Self, NumericFault> {
        let mag = r.raw(4)?;
        if mag != b"TR01" {
            return Err(NumericFault::Overflow("t_recover_magic"));
        }
        let mut d = Self::new(r.u64()?, r.u64()?, r.u64()?, r.u64()?, r.i128()?, r.bool()?);
        d.first_recover = r.option_u64()?;
        d.died_at = r.option_u64()?;
        d.engineering = match r.u8()? {
            0 => None,
            1 => Some(
                String::from_utf8(r.blob()?.to_vec())
                    .map_err(|_| NumericFault::Overflow("t_recover_err"))?,
            ),
            _ => return Err(NumericFault::Overflow("t_recover_err_tag")),
        };
        let n = r.u64()? as usize;
        for _ in 0..n {
            d.buf.push_back(TickSample::decode(r)?);
        }
        Ok(d)
    }
}

/// Recompute first recovery from a complete tick log (independent of the live ring).
pub fn first_recover_from_log(
    samples: &[TickSample],
    p0: i128,
    living0: bool,
    l: u64,
    w: u64,
    h: u64,
) -> Option<u64> {
    let mut d = RecoverDetector::new(l, w, h, 0, p0, living0);
    for s in samples {
        d.on_tick(s.clone());
        if s.t_rel >= h {
            break;
        }
    }
    d.first_recover_t()
}

fn healthy(t: u64, p: i128, photo: u64, decision: bool, cog_due: u64, cog_paid: u64) -> TickSample {
    TickSample {
        t_rel: t,
        world_tick_after: t,
        living: true,
        photo_extent: photo,
        p,
        debt: 0,
        debt_accrued: 0,
        maint_due: 10,
        maint_paid: 10,
        maint_unpaid: 0,
        cognition_due: cog_due,
        cognition_paid: cog_paid,
        decision_boundary: decision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(samples: Vec<TickSample>, l: u64, w: u64, h: u64, p0: i128) -> RecoverDetector {
        let mut d = RecoverDetector::new(l, w, h, 0, p0, true);
        for s in samples {
            d.on_tick(s);
        }
        d
    }

    fn seq(n: u64, p0: i128, dp_per: i128, photo: u64) -> Vec<TickSample> {
        (1..=n)
            .map(|t| {
                let decision = (t - 1) % 5 == 0;
                healthy(
                    t,
                    p0 + dp_per * t as i128,
                    photo,
                    decision,
                    if decision { 8 } else { 0 },
                    if decision { 8 } else { 0 },
                )
            })
            .collect()
    }

    #[test]
    fn earliest_legal_is_l_plus_w() {
        let l = 2;
        let w = 3;
        let h = 20;
        let p0 = 100;
        let d = run(seq(4, p0, 1, 1), l, w, h, p0);
        assert!(d.first_recover_t().is_none(), "t=4 < L+W=5");
        let d = run(seq(5, p0, 1, 1), l, w, h, p0);
        assert_eq!(d.first_recover_t(), Some(5));
        assert!(d.outcome(5).success);
        assert_eq!(d.outcome(5).restricted_time, 5);
    }

    #[test]
    fn delta_p_zero_ok_minus_one_not() {
        let l = 0;
        let w = 2;
        let h = 10;
        let p0 = 50;
        let mut flat = seq(2, p0, 0, 1);
        let d = run(flat.clone(), l, w, h, p0);
        assert_eq!(d.first_recover_t(), Some(2));
        flat[1].p = p0 - 1;
        let d = run(flat, l, w, h, p0);
        assert!(d.first_recover_t().is_none());
    }

    #[test]
    fn no_photo_rejected_even_if_delta_p_nonneg() {
        let d = run(seq(5, 10, 1, 0), 2, 3, 20, 10);
        assert!(d.first_recover_t().is_none());
    }

    #[test]
    fn unpaid_maint_or_cognition_or_debt_rejected() {
        let l = 0;
        let w = 2;
        let h = 10;
        let p0 = 20;
        let mut s = seq(2, p0, 1, 1);
        s[0].maint_unpaid = 1;
        s[0].maint_paid = 9;
        assert!(run(s, l, w, h, p0).first_recover_t().is_none());
        let mut s = seq(2, p0, 1, 1);
        s[0].decision_boundary = true;
        s[0].cognition_due = 8;
        s[0].cognition_paid = 0;
        assert!(run(s, l, w, h, p0).first_recover_t().is_none());
        let mut s = seq(2, p0, 1, 1);
        s[1].debt_accrued = 3;
        assert!(run(s, l, w, h, p0).first_recover_t().is_none());
    }

    #[test]
    fn death_in_window_and_h_boundary() {
        let l = 0;
        let w = 2;
        let h = 4;
        let p0 = 10;
        let mut s = seq(4, p0, 1, 1);
        s[1].living = false;
        let d = run(s, l, w, h, p0);
        let o = d.outcome(4);
        assert_eq!(o.status, RecoverStatus::Died);
        assert!(!o.success);
        assert_eq!(o.restricted_time, h);

        let s = seq(4, p0, 1, 1);
        let d = run(s, l, w, h, p0);
        assert_eq!(d.first_recover_t(), Some(2));

        let s = seq(4, p0, 1, 0);
        let d = run(s, l, w, h, p0);
        let o = d.outcome(4);
        assert_eq!(o.status, RecoverStatus::NotRecoveredByH);
        assert_eq!(o.restricted_time, h);
        assert!(!o.success);
    }

    #[test]
    fn recover_exactly_at_h_is_success() {
        let l = 3;
        let w = 2;
        let h = 5;
        let d = run(seq(5, 0, 1, 1), l, w, h, 0);
        assert_eq!(d.first_recover_t(), Some(5));
        let o = d.outcome(5);
        assert!(o.success);
        assert_eq!(o.restricted_time, 5);
        assert_eq!(o.status, RecoverStatus::Recovered);
    }

    #[test]
    fn restore_mid_window_keeps_first_time() {
        let l = 1;
        let w = 3;
        let h = 20;
        let p0 = 0;
        let samples = seq(10, p0, 1, 1);
        let mut d = RecoverDetector::new(l, w, h, 0, p0, true);
        for s in samples.iter().take(5) {
            d.on_tick(s.clone());
        }
        let mut enc = CanonWriter::new();
        d.encode(&mut enc);
        let bytes = enc.into_inner();
        let mut r = CanonReader::new(&bytes);
        let mut d2 = RecoverDetector::decode(&mut r).unwrap();
        r.finish().unwrap();
        for s in samples.iter().skip(5) {
            d.on_tick(s.clone());
            d2.on_tick(s.clone());
        }
        assert_eq!(d.first_recover_t(), d2.first_recover_t());
        assert_eq!(d.first_recover_t(), Some(4));
    }

    #[test]
    fn log_recompute_matches_live() {
        let samples = seq(12, 4, 1, 2);
        let mut d = RecoverDetector::new(2, 3, 20, 0, 4, true);
        for s in &samples {
            d.on_tick(s.clone());
        }
        assert_eq!(
            first_recover_from_log(&samples, 4, true, 2, 3, 20),
            d.first_recover_t()
        );
    }
}
