//! Authoritative world state. Field order in canonical encoding is fixed.

use crate::canon::{CanonReader, CanonWriter, MAGIC_STATE};
use crate::enums::{HIDDEN, INPUTS, PHI, POLICY_LOGITS};
use crate::error::NumericFault;
use crate::hash::{rules_hash, sha256, Hash32};
use crate::rules::RuleSpec;

#[derive(Debug, Clone, PartialEq)]
pub struct WorldState {
    pub tick: u64,
    pub root_seed: u64,
    pub rules_hash: Hash32,
    pub next_entity_id: u64,
    pub fields: Fields,
    pub organisms: Vec<Organism>,
    pub genomes: Vec<Genome>,
    pub embryos: Vec<Embryo>,
    pub future_inputs: Vec<FutureInput>,
    pub audit: AuditTotals,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fields {
    pub width: u32,
    pub height: u32,
    pub n: Vec<u64>,
    pub o: Vec<u64>,
    pub s: Vec<u64>,
    pub q: Vec<u64>,
    pub s_remainder: Vec<u16>,
    pub q_remainder: Vec<u16>,
}

impl Fields {
    pub fn empty(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            n: Vec::new(),
            o: Vec::new(),
            s: Vec::new(),
            q: Vec::new(),
            s_remainder: Vec::new(),
            q_remainder: Vec::new(),
        }
    }

    pub fn zeros(width: u32, height: u32) -> Self {
        let n = (width as usize) * (height as usize);
        Self {
            width,
            height,
            n: vec![0; n],
            o: vec![0; n],
            s: vec![0; n],
            q: vec![0; n],
            s_remainder: vec![0; n],
            q_remainder: vec![0; n],
        }
    }

    pub fn len(&self) -> usize {
        (self.width as usize) * (self.height as usize)
    }

    pub fn idx(&self, x: u32, y: u32) -> usize {
        (y as usize) * (self.width as usize) + (x as usize)
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuditTotals {
    pub initial_m: u128,
    pub initial_u: u128,
    pub cumulative_radiation_input: u128,
    pub cumulative_dissipation: u128,
    pub cumulative_radiation_loss: u128,
    pub step_dissipation: u64,
    pub step_radiation_loss: u64,
    pub step_radiation_input: u64,
}

impl Default for AuditTotals {
    fn default() -> Self {
        Self {
            initial_m: 0,
            initial_u: 0,
            cumulative_radiation_input: 0,
            cumulative_dissipation: 0,
            cumulative_radiation_loss: 0,
            step_dissipation: 0,
            step_radiation_loss: 0,
            step_radiation_input: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accounts {
    pub n: u64,
    pub c: u64,
    pub b: u64,
    pub e_old: u64,
    pub e_new: u64,
}

impl Default for Accounts {
    fn default() -> Self {
        Self {
            n: 0,
            c: 0,
            b: 0,
            e_old: 0,
            e_new: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub x: f64,
    pub y: f64,
    pub theta: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExecutableControl {
    pub thrust: i8,
    pub turn: i8,
    pub u_effective: f64,
    pub w_effective: f64,
    pub n_boost: bool,
    pub o_uptake: bool,
    pub photo_boost: bool,
    pub metab_boost: bool,
    pub repair: bool,
    pub reproduce: bool,
    pub signal: bool,
    pub transfer: bool,
    pub attack: bool,
    pub adhere: bool,
}

impl Default for ExecutableControl {
    fn default() -> Self {
        Self {
            thrust: 0,
            turn: 0,
            u_effective: 0.0,
            w_effective: 0.0,
            n_boost: false,
            o_uptake: false,
            photo_boost: false,
            metab_boost: false,
            repair: false,
            reproduce: false,
            signal: false,
            transfer: false,
            attack: false,
            adhere: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SampledIntent {
    pub thrust: i8,
    pub turn: i8,
    pub binary: [bool; 10],
    pub valid: bool,
}

impl Default for SampledIntent {
    fn default() -> Self {
        Self {
            thrust: 0,
            turn: 0,
            binary: [false; 10],
            valid: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntervalFeedback {
    pub struct_loss: u64,
    pub e_net_i128: i128,
    pub thrust_sum: f64,
    pub turn_sum: f64,
    pub uptake_no: u64,
    pub photo_extent: u64,
    pub ticks: u64,
    pub birth_count: u64,
    pub died: bool,
}

impl Default for IntervalFeedback {
    fn default() -> Self {
        Self {
            struct_loss: 0,
            e_net_i128: 0,
            thrust_sum: 0.0,
            turn_sum: 0.0,
            uptake_no: 0,
            photo_extent: 0,
            ticks: 0,
            birth_count: 0,
            died: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BrainState {
    pub a: [[f64; PHI]; POLICY_LOGITS],
    pub v: [f64; PHI],
    pub h: [f64; HIDDEN],
    pub e_a: [[f64; PHI]; POLICY_LOGITS],
    pub e_v: [f64; PHI],
    pub phi_prev: [f64; PHI],
    pub grad_log: [[f64; PHI]; POLICY_LOGITS],
    pub grad_ent: [[f64; PHI]; POLICY_LOGITS],
    pub sampled: SampledIntent,
    pub has_pending: bool,
    pub r: f64,
    pub gamma: f64,
    pub trace_decay: f64,
    pub interval_d: u64,
    pub last_delta: f64,
}

impl BrainState {
    pub fn zeroed() -> Self {
        Self {
            a: [[0.0; PHI]; POLICY_LOGITS],
            v: [0.0; PHI],
            h: [0.0; HIDDEN],
            e_a: [[0.0; PHI]; POLICY_LOGITS],
            e_v: [0.0; PHI],
            phi_prev: [0.0; PHI],
            grad_log: [[0.0; PHI]; POLICY_LOGITS],
            grad_ent: [[0.0; PHI]; POLICY_LOGITS],
            sampled: SampledIntent::default(),
            has_pending: false,
            r: 0.0,
            gamma: 1.0,
            trace_decay: 1.0,
            interval_d: 0,
            last_delta: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Organism {
    pub entity_id: u64,
    pub geom: Geometry,
    pub genome_hash: Hash32,
    pub accounts: Accounts,
    pub b_target: u64,
    pub n_max: u64,
    pub c_max: u64,
    pub e_max: u64,
    pub maintenance_debt: u64,
    pub age: u64,
    pub control: ExecutableControl,
    pub intent: SampledIntent,
    pub brain: Box<BrainState>,
    pub feedback: IntervalFeedback,
    pub last_feedback: IntervalFeedback,
    pub attempt_count: u64,
    pub last_dx: f64,
    pub last_dy: f64,
    pub last_dtheta: f64,
    pub cognition_paid: bool,
    pub marked_dead: bool,
}

impl Organism {
    pub fn new(entity_id: u64, geom: Geometry, genome_hash: Hash32, spec: &RuleSpec) -> Self {
        Self {
            entity_id,
            geom,
            genome_hash,
            accounts: Accounts {
                n: spec.init.initial_n,
                c: spec.init.initial_c,
                b: spec.init.initial_b,
                e_old: spec.init.initial_e,
                e_new: 0,
            },
            b_target: spec.init.b_target,
            n_max: spec.init.n_max,
            c_max: spec.init.c_max,
            e_max: spec.init.e_max,
            maintenance_debt: 0,
            age: 0,
            control: ExecutableControl::default(),
            intent: SampledIntent::default(),
            brain: Box::new(BrainState::zeroed()),
            feedback: IntervalFeedback::default(),
            last_feedback: IntervalFeedback::default(),
            attempt_count: 0,
            last_dx: 0.0,
            last_dy: 0.0,
            last_dtheta: 0.0,
            cognition_paid: false,
            marked_dead: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Genome {
    pub hash: Hash32,
    pub q_win: Vec<i32>,
    pub q_wrec: Vec<i32>,
    pub q_bh: Vec<i32>,
    pub q_a0: Vec<i32>,
    pub q_v0: Vec<i32>,
}

impl Genome {
    pub fn param_count_ok(&self) -> bool {
        self.q_win.len() == HIDDEN * INPUTS
            && self.q_wrec.len() == HIDDEN * HIDDEN
            && self.q_bh.len() == HIDDEN
            && self.q_a0.len() == POLICY_LOGITS * PHI
            && self.q_v0.len() == PHI
    }

    pub fn recompute_hash(&mut self) {
        let mut w = CanonWriter::new();
        w.i32_slice(&self.q_win);
        w.i32_slice(&self.q_wrec);
        w.i32_slice(&self.q_bh);
        w.i32_slice(&self.q_a0);
        w.i32_slice(&self.q_v0);
        self.hash = sha256(&w.into_inner());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EmbryoStage {
    Building = 1,
    Ready = 2,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Embryo {
    pub parent_id: u64,
    pub attempt_id: u64,
    pub genome_hash: Hash32,
    pub extent: u64,
    pub target_extent: u64,
    pub b: u64,
    pub c: u64,
    pub e: u64,
    pub created_tick: u64,
    pub heading0: f64,
    pub ready_tick: Option<u64>,
    pub stage: EmbryoStage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FutureInput {
    pub effective_tick: u64,
    pub seq: u64,
    pub kind: u8,
    pub payload: Vec<u8>,
}

impl WorldState {
    pub fn empty(spec: &RuleSpec, root_seed: u64) -> Result<Self, NumericFault> {
        let rh = rules_hash(spec)?;
        Ok(Self {
            tick: 0,
            root_seed,
            rules_hash: rh,
            next_entity_id: 1,
            fields: Fields::empty(spec.world.grid_width, spec.world.grid_height),
            organisms: Vec::new(),
            genomes: Vec::new(),
            embryos: Vec::new(),
            future_inputs: Vec::new(),
            audit: AuditTotals::default(),
        })
    }

    pub fn sort_authoritative(&mut self) {
        self.organisms.sort_by_key(|o| o.entity_id);
        self.genomes.sort_by(|a, b| a.hash.cmp(&b.hash));
        self.embryos
            .sort_by_key(|e| (e.parent_id, e.attempt_id));
        self.future_inputs
            .sort_by_key(|f| (f.effective_tick, f.seq));
    }

    pub fn encode_canonical(&self) -> Result<Vec<u8>, NumericFault> {
        let mut w = CanonWriter::with_magic_state();
        w.u64(self.tick);
        w.u64(self.root_seed);
        w.bytes32(&self.rules_hash);
        w.u64(self.next_entity_id);
        encode_fields(&mut w, &self.fields)?;
        w.u64(self.organisms.len() as u64);
        for o in &self.organisms {
            encode_organism(&mut w, o)?;
        }
        w.u64(self.genomes.len() as u64);
        for g in &self.genomes {
            encode_genome(&mut w, g);
        }
        w.u64(self.embryos.len() as u64);
        for e in &self.embryos {
            encode_embryo(&mut w, e)?;
        }
        w.u64(self.future_inputs.len() as u64);
        for f in &self.future_inputs {
            w.u64(f.effective_tick);
            w.u64(f.seq);
            w.u8(f.kind);
            w.u64(f.payload.len() as u64);
            w.raw(&f.payload);
        }
        w.u128(self.audit.initial_m);
        w.u128(self.audit.initial_u);
        w.u128(self.audit.cumulative_radiation_input);
        w.u128(self.audit.cumulative_dissipation);
        w.u128(self.audit.cumulative_radiation_loss);
        w.u64(self.audit.step_dissipation);
        w.u64(self.audit.step_radiation_loss);
        w.u64(self.audit.step_radiation_input);
        Ok(w.into_inner())
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, NumericFault> {
        let mut r = CanonReader::new(bytes);
        r.expect_magic(&MAGIC_STATE)?;
        let mut st = Self {
            tick: r.u64()?,
            root_seed: r.u64()?,
            rules_hash: r.bytes32()?,
            next_entity_id: r.u64()?,
            fields: decode_fields(&mut r)?,
            organisms: Vec::new(),
            genomes: Vec::new(),
            embryos: Vec::new(),
            future_inputs: Vec::new(),
            audit: AuditTotals::default(),
        };
        let no = r.u64()? as usize;
        for _ in 0..no {
            st.organisms.push(decode_organism(&mut r)?);
        }
        let ng = r.u64()? as usize;
        for _ in 0..ng {
            st.genomes.push(decode_genome(&mut r)?);
        }
        let ne = r.u64()? as usize;
        for _ in 0..ne {
            st.embryos.push(decode_embryo(&mut r)?);
        }
        let nf = r.u64()? as usize;
        for _ in 0..nf {
            let effective_tick = r.u64()?;
            let seq = r.u64()?;
            let kind = r.u8()?;
            let n = r.u64()? as usize;
            let mut payload = vec![0u8; n];
            for b in payload.iter_mut() {
                *b = r.u8()?;
            }
            st.future_inputs.push(FutureInput {
                effective_tick,
                seq,
                kind,
                payload,
            });
        }
        st.audit.initial_m = r.u128()?;
        st.audit.initial_u = r.u128()?;
        st.audit.cumulative_radiation_input = r.u128()?;
        st.audit.cumulative_dissipation = r.u128()?;
        st.audit.cumulative_radiation_loss = r.u128()?;
        st.audit.step_dissipation = r.u64()?;
        st.audit.step_radiation_loss = r.u64()?;
        st.audit.step_radiation_input = r.u64()?;
        r.finish()?;
        Ok(st)
    }
}

fn encode_fields(w: &mut CanonWriter, f: &Fields) -> Result<(), NumericFault> {
    w.u32(f.width);
    w.u32(f.height);
    w.u64_slice(&f.n);
    w.u64_slice(&f.o);
    w.u64_slice(&f.s);
    w.u64_slice(&f.q);
    w.u16_slice(&f.s_remainder);
    w.u16_slice(&f.q_remainder);
    Ok(())
}

fn decode_fields(r: &mut CanonReader<'_>) -> Result<Fields, NumericFault> {
    let width = r.u32()?;
    let height = r.u32()?;
    let mut f = Fields::empty(width, height);
    let nn = r.u64()? as usize;
    f.n.reserve(nn);
    for _ in 0..nn {
        f.n.push(r.u64()?);
    }
    let no = r.u64()? as usize;
    for _ in 0..no {
        f.o.push(r.u64()?);
    }
    let ns = r.u64()? as usize;
    for _ in 0..ns {
        f.s.push(r.u64()?);
    }
    let nq = r.u64()? as usize;
    for _ in 0..nq {
        f.q.push(r.u64()?);
    }
    let nsr = r.u64()? as usize;
    for _ in 0..nsr {
        f.s_remainder.push(r.u16()?);
    }
    let nqr = r.u64()? as usize;
    for _ in 0..nqr {
        f.q_remainder.push(r.u16()?);
    }
    Ok(f)
}

fn encode_control(w: &mut CanonWriter, c: &ExecutableControl) -> Result<(), NumericFault> {
    w.i8(c.thrust);
    w.i8(c.turn);
    w.f64_canonical(c.u_effective)?;
    w.f64_canonical(c.w_effective)?;
    w.bool(c.n_boost);
    w.bool(c.o_uptake);
    w.bool(c.photo_boost);
    w.bool(c.metab_boost);
    w.bool(c.repair);
    w.bool(c.reproduce);
    w.bool(c.signal);
    w.bool(c.transfer);
    w.bool(c.attack);
    w.bool(c.adhere);
    Ok(())
}

fn decode_control(r: &mut CanonReader<'_>) -> Result<ExecutableControl, NumericFault> {
    Ok(ExecutableControl {
        thrust: r.i8()?,
        turn: r.i8()?,
        u_effective: r.f64_canonical()?,
        w_effective: r.f64_canonical()?,
        n_boost: r.bool()?,
        o_uptake: r.bool()?,
        photo_boost: r.bool()?,
        metab_boost: r.bool()?,
        repair: r.bool()?,
        reproduce: r.bool()?,
        signal: r.bool()?,
        transfer: r.bool()?,
        attack: r.bool()?,
        adhere: r.bool()?,
    })
}

fn encode_intent(w: &mut CanonWriter, s: &SampledIntent) {
    w.i8(s.thrust);
    w.i8(s.turn);
    for b in s.binary {
        w.bool(b);
    }
    w.bool(s.valid);
}

fn decode_intent(r: &mut CanonReader<'_>) -> Result<SampledIntent, NumericFault> {
    let mut binary = [false; 10];
    let thrust = r.i8()?;
    let turn = r.i8()?;
    for b in binary.iter_mut() {
        *b = r.bool()?;
    }
    Ok(SampledIntent {
        thrust,
        turn,
        binary,
        valid: r.bool()?,
    })
}

fn encode_feedback(w: &mut CanonWriter, f: &IntervalFeedback) -> Result<(), NumericFault> {
    w.u64(f.struct_loss);
    w.raw(&f.e_net_i128.to_le_bytes());
    w.f64_canonical(f.thrust_sum)?;
    w.f64_canonical(f.turn_sum)?;
    w.u64(f.uptake_no);
    w.u64(f.photo_extent);
    w.u64(f.ticks);
    w.u64(f.birth_count);
    w.bool(f.died);
    Ok(())
}

fn decode_feedback(r: &mut CanonReader<'_>) -> Result<IntervalFeedback, NumericFault> {
    let struct_loss = r.u64()?;
    let mut b = [0u8; 16];
    for x in b.iter_mut() {
        *x = r.u8()?;
    }
    Ok(IntervalFeedback {
        struct_loss,
        e_net_i128: i128::from_le_bytes(b),
        thrust_sum: r.f64_canonical()?,
        turn_sum: r.f64_canonical()?,
        uptake_no: r.u64()?,
        photo_extent: r.u64()?,
        ticks: r.u64()?,
        birth_count: r.u64()?,
        died: r.bool()?,
    })
}

fn encode_brain(w: &mut CanonWriter, b: &BrainState) -> Result<(), NumericFault> {
    for row in &b.a {
        for &x in row {
            w.f64_canonical(x)?;
        }
    }
    for &x in &b.v {
        w.f64_canonical(x)?;
    }
    for &x in &b.h {
        w.f64_canonical(x)?;
    }
    for row in &b.e_a {
        for &x in row {
            w.f64_canonical(x)?;
        }
    }
    for &x in &b.e_v {
        w.f64_canonical(x)?;
    }
    for &x in &b.phi_prev {
        w.f64_canonical(x)?;
    }
    for row in &b.grad_log {
        for &x in row {
            w.f64_canonical(x)?;
        }
    }
    for row in &b.grad_ent {
        for &x in row {
            w.f64_canonical(x)?;
        }
    }
    encode_intent(w, &b.sampled);
    w.bool(b.has_pending);
    w.f64_canonical(b.r)?;
    w.f64_canonical(b.gamma)?;
    w.f64_canonical(b.trace_decay)?;
    w.u64(b.interval_d);
    w.f64_canonical(b.last_delta)?;
    Ok(())
}

fn decode_brain(r: &mut CanonReader<'_>) -> Result<BrainState, NumericFault> {
    let mut b = BrainState::zeroed();
    for row in &mut b.a {
        for x in row {
            *x = r.f64_canonical()?;
        }
    }
    for x in &mut b.v {
        *x = r.f64_canonical()?;
    }
    for x in &mut b.h {
        *x = r.f64_canonical()?;
    }
    for row in &mut b.e_a {
        for x in row {
            *x = r.f64_canonical()?;
        }
    }
    for x in &mut b.e_v {
        *x = r.f64_canonical()?;
    }
    for x in &mut b.phi_prev {
        *x = r.f64_canonical()?;
    }
    for row in &mut b.grad_log {
        for x in row {
            *x = r.f64_canonical()?;
        }
    }
    for row in &mut b.grad_ent {
        for x in row {
            *x = r.f64_canonical()?;
        }
    }
    b.sampled = decode_intent(r)?;
    b.has_pending = r.bool()?;
    b.r = r.f64_canonical()?;
    b.gamma = r.f64_canonical()?;
    b.trace_decay = r.f64_canonical()?;
    b.interval_d = r.u64()?;
    b.last_delta = r.f64_canonical()?;
    Ok(b)
}

fn encode_organism(w: &mut CanonWriter, o: &Organism) -> Result<(), NumericFault> {
    w.u64(o.entity_id);
    w.f64_canonical(o.geom.x)?;
    w.f64_canonical(o.geom.y)?;
    w.f64_canonical(o.geom.theta)?;
    w.bytes32(&o.genome_hash);
    w.u64(o.accounts.n);
    w.u64(o.accounts.c);
    w.u64(o.accounts.b);
    w.u64(o.accounts.e_old);
    w.u64(o.accounts.e_new);
    w.u64(o.b_target);
    w.u64(o.n_max);
    w.u64(o.c_max);
    w.u64(o.e_max);
    w.u64(o.maintenance_debt);
    w.u64(o.age);
    encode_control(w, &o.control)?;
    encode_intent(w, &o.intent);
    encode_brain(w, &o.brain)?;
    encode_feedback(w, &o.feedback)?;
    encode_feedback(w, &o.last_feedback)?;
    w.u64(o.attempt_count);
    w.f64_canonical(o.last_dx)?;
    w.f64_canonical(o.last_dy)?;
    w.f64_canonical(o.last_dtheta)?;
    w.bool(o.cognition_paid);
    w.bool(o.marked_dead);
    Ok(())
}

fn decode_organism(r: &mut CanonReader<'_>) -> Result<Organism, NumericFault> {
    Ok(Organism {
        entity_id: r.u64()?,
        geom: Geometry {
            x: r.f64_canonical()?,
            y: r.f64_canonical()?,
            theta: r.f64_canonical()?,
        },
        genome_hash: r.bytes32()?,
        accounts: Accounts {
            n: r.u64()?,
            c: r.u64()?,
            b: r.u64()?,
            e_old: r.u64()?,
            e_new: r.u64()?,
        },
        b_target: r.u64()?,
        n_max: r.u64()?,
        c_max: r.u64()?,
        e_max: r.u64()?,
        maintenance_debt: r.u64()?,
        age: r.u64()?,
        control: decode_control(r)?,
        intent: decode_intent(r)?,
        brain: Box::new(decode_brain(r)?),
        feedback: decode_feedback(r)?,
        last_feedback: decode_feedback(r)?,
        attempt_count: r.u64()?,
        last_dx: r.f64_canonical()?,
        last_dy: r.f64_canonical()?,
        last_dtheta: r.f64_canonical()?,
        cognition_paid: r.bool()?,
        marked_dead: r.bool()?,
    })
}

fn encode_genome(w: &mut CanonWriter, g: &Genome) {
    w.bytes32(&g.hash);
    w.i32_slice(&g.q_win);
    w.i32_slice(&g.q_wrec);
    w.i32_slice(&g.q_bh);
    w.i32_slice(&g.q_a0);
    w.i32_slice(&g.q_v0);
}

fn decode_genome(r: &mut CanonReader<'_>) -> Result<Genome, NumericFault> {
    let hash = r.bytes32()?;
    let mut q_win = Vec::new();
    let n = r.u64()? as usize;
    for _ in 0..n {
        q_win.push(r.i32()?);
    }
    let mut q_wrec = Vec::new();
    let n = r.u64()? as usize;
    for _ in 0..n {
        q_wrec.push(r.i32()?);
    }
    let mut q_bh = Vec::new();
    let n = r.u64()? as usize;
    for _ in 0..n {
        q_bh.push(r.i32()?);
    }
    let mut q_a0 = Vec::new();
    let n = r.u64()? as usize;
    for _ in 0..n {
        q_a0.push(r.i32()?);
    }
    let mut q_v0 = Vec::new();
    let n = r.u64()? as usize;
    for _ in 0..n {
        q_v0.push(r.i32()?);
    }
    Ok(Genome {
        hash,
        q_win,
        q_wrec,
        q_bh,
        q_a0,
        q_v0,
    })
}

fn encode_embryo(w: &mut CanonWriter, e: &Embryo) -> Result<(), NumericFault> {
    w.u64(e.parent_id);
    w.u64(e.attempt_id);
    w.bytes32(&e.genome_hash);
    w.u64(e.extent);
    w.u64(e.target_extent);
    w.u64(e.b);
    w.u64(e.c);
    w.u64(e.e);
    w.u64(e.created_tick);
    w.f64_canonical(e.heading0)?;
    w.option_u64(e.ready_tick);
    w.u8(e.stage as u8);
    Ok(())
}

fn decode_embryo(r: &mut CanonReader<'_>) -> Result<Embryo, NumericFault> {
    Ok(Embryo {
        parent_id: r.u64()?,
        attempt_id: r.u64()?,
        genome_hash: r.bytes32()?,
        extent: r.u64()?,
        target_extent: r.u64()?,
        b: r.u64()?,
        c: r.u64()?,
        e: r.u64()?,
        created_tick: r.u64()?,
        heading0: r.f64_canonical()?,
        ready_tick: r.option_u64()?,
        stage: match r.u8()? {
            1 => EmbryoStage::Building,
            2 => EmbryoStage::Ready,
            _ => return Err(NumericFault::Overflow("embryo_stage")),
        },
    })
}
