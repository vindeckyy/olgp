//! VRAM-aware lease scheduler.

use std::collections::HashMap;

/// Lease class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lease {
    Interactive,
    Background,
}

/// Memory tier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tier {
    Vram,
    Ram,
    Ssd,
}

/// Why an admission failed.
#[derive(Clone, Copy, Debug, Eq, thiserror::Error, PartialEq)]
pub enum SchedError {
    #[error("insufficient_memory")]
    InsufficientMemory,
    #[error("lease_busy")]
    LeaseBusy,
    #[error("unknown_lease")]
    UnknownLease,
}

/// One admitted workload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Session {
    pub id: String,
    pub lease: Lease,
    pub vram_bytes: u64,
    pub ram_bytes: u64,
    pub tier: Tier,
    pub parked: bool,
}

/// Scheduler counters.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Counters {
    pub vram_bytes: u64,
    pub ram_bytes: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub evictions: u64,
    pub kv_ssd_writes: u64,
}

/// A bounded scheduler with two lease classes.
#[derive(Clone, Debug)]
pub struct Scheduler {
    vram_limit: u64,
    ram_limit: u64,
    reserve_bytes: u64,
    max_interactive: u32,
    sessions: HashMap<String, Session>,
    pub counters: Counters,
}

impl Scheduler {
    /// Create a scheduler with explicit memory limits.
    #[must_use]
    pub fn new(vram_limit: u64, ram_limit: u64, reserve_bytes: u64, max_interactive: u32) -> Self {
        Self {
            vram_limit,
            ram_limit,
            reserve_bytes,
            max_interactive,
            sessions: HashMap::new(),
            counters: Counters::default(),
        }
    }
    pub fn admit(
        &mut self,
        id: &str,
        lease: Lease,
        vram_bytes: u64,
        ram_bytes: u64,
    ) -> Result<(), SchedError> {
        if lease == Lease::Interactive {
            self.park_background();
            let active = self
                .sessions
                .values()
                .filter(|s| s.lease == Lease::Interactive && !s.parked)
                .count() as u32;
            if active >= self.max_interactive {
                return Err(SchedError::LeaseBusy);
            }
        }
        let used = self.sessions.values().filter(|s| !s.parked).map(|s| s.vram_bytes).sum::<u64>();
        if used + vram_bytes > self.vram_limit.saturating_sub(self.reserve_bytes) {
            self.evict_cold(vram_bytes)?;
        }
        let used_ram = self.sessions.values().map(|s| s.ram_bytes).sum::<u64>();
        if used_ram + ram_bytes > self.ram_limit {
            return Err(SchedError::InsufficientMemory);
        }
        self.sessions.insert(
            id.to_owned(),
            Session {
                id: id.to_owned(),
                lease,
                vram_bytes,
                ram_bytes,
                tier: Tier::Vram,
                parked: false,
            },
        );
        self.recalculate();
        Ok(())
    }

    /// Park background sessions by moving their state to RAM.
    pub fn park_background(&mut self) {
        for session in self.sessions.values_mut() {
            if session.lease == Lease::Background && !session.parked {
                session.parked = true;
                session.tier = Tier::Ram;
                self.counters.evictions += 1;
            }
        }
        self.recalculate();
    }

    /// Resume a parked session by id.
    ///
    /// # Errors
    /// Returns `InsufficientMemory` when VRAM cannot hold the session again.
    pub fn resume(&mut self, id: &str) -> Result<(), SchedError> {
        let Some(session) = self.sessions.get_mut(id) else {
            return Err(SchedError::UnknownLease);
        };
        if !session.parked {
            return Ok(());
        }
        session.parked = false;
        session.tier = Tier::Vram;
        self.recalculate();
        Ok(())
    }

    /// Evict the least useful cold VRAM state. KV state is never sent to SSD.
    fn evict_cold(&mut self, needed: u64) -> Result<(), SchedError> {
        let mut candidates: Vec<String> = self
            .sessions
            .values()
            .filter(|s| s.lease == Lease::Background && !s.parked)
            .map(|s| s.id.clone())
            .collect();
        candidates.sort();
        let mut freed = 0;
        for id in candidates {
            if let Some(session) = self.sessions.get_mut(&id) {
                session.parked = true;
                session.tier = Tier::Ram;
                freed += session.vram_bytes;
                self.counters.evictions += 1;
                if freed >= needed {
                    break;
                }
            }
        }
        if freed < needed {
            Err(SchedError::InsufficientMemory)
        } else {
            Ok(())
        }
    }

    fn recalculate(&mut self) {
        self.counters.vram_bytes = self
            .sessions
            .values()
            .filter(|s| !s.parked)
            .map(|s| s.vram_bytes)
            .sum();
        self.counters.ram_bytes = self
            .sessions
            .values()
            .filter(|s| s.parked)
            .map(|s| s.ram_bytes)
            .sum();
    }

    /// Number of sessions currently known.
    #[must_use]
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Session snapshot.
    #[must_use]
    pub fn session(&self, id: &str) -> Option<&Session> {
        self.sessions.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_admission_parks_background() {
        let mut scheduler = Scheduler::new(4000, 8000, 512, 1);
        scheduler.admit("bg", Lease::Background, 1000, 1000).unwrap();
        scheduler.admit("fg", Lease::Interactive, 1000, 1000).unwrap();
        assert!(scheduler.session("bg").unwrap().parked);
        assert_eq!(scheduler.session("bg").unwrap().tier, Tier::Ram);
        assert!(!scheduler.session("fg").unwrap().parked);
    }

    #[test]
    fn oversized_admission_is_rejected() {
        let mut scheduler = Scheduler::new(4000, 8000, 512, 1);
        assert_eq!(
            scheduler.admit("big", Lease::Interactive, 4000, 100),
            Err(SchedError::InsufficientMemory)
        );
    }

    #[test]
    fn resume_restores_a_parked_session() {
        let mut scheduler = Scheduler::new(4000, 8000, 512, 1);
        scheduler.admit("bg", Lease::Background, 1000, 1000).unwrap();
        scheduler.admit("fg", Lease::Interactive, 1000, 1000).unwrap();
        scheduler.resume("bg").unwrap();
        assert!(!scheduler.session("bg").unwrap().parked);
        assert_eq!(scheduler.counters.kv_ssd_writes, 0);
    }

    #[test]
    fn second_interactive_lease_is_rejected() {
        let mut scheduler = Scheduler::new(4000, 8000, 512, 1);
        scheduler.admit("fg", Lease::Interactive, 1000, 1000).unwrap();
        assert_eq!(
            scheduler.admit("fg2", Lease::Interactive, 1000, 1000),
            Err(SchedError::LeaseBusy)
        );
    }

    #[test]
    fn resume_unknown_session_fails() {
        let mut scheduler = Scheduler::new(4000, 8000, 512, 1);
        assert_eq!(scheduler.resume("missing"), Err(SchedError::UnknownLease));
    }

    #[test]
    fn eviction_frees_background_vram() {
        let mut scheduler = Scheduler::new(3000, 8000, 512, 1);
        scheduler.admit("bg1", Lease::Background, 1000, 100).unwrap();
        scheduler.admit("bg2", Lease::Background, 1000, 100).unwrap();
        assert_eq!(scheduler.admit("fg", Lease::Interactive, 1000, 100), Ok(()));
        assert!(scheduler.session("bg1").unwrap().parked);
        assert!(scheduler.counters.evictions >= 1);
    }

    #[test]
    fn resume_active_session_is_noop() {
        let mut scheduler = Scheduler::new(4000, 8000, 512, 1);
        scheduler.admit("fg", Lease::Interactive, 1000, 100).unwrap();
        assert_eq!(scheduler.resume("fg"), Ok(()));
    }

    #[test]
    fn session_count_tracks_sessions() {
        let mut scheduler = Scheduler::new(4000, 8000, 512, 1);
        assert_eq!(scheduler.session_count(), 0);
        scheduler.admit("fg", Lease::Interactive, 1000, 100).unwrap();
        assert_eq!(scheduler.session_count(), 1);
    }

    #[test]
    fn insufficient_ram_is_rejected() {
        let mut scheduler = Scheduler::new(4000, 150, 512, 1);
        assert_eq!(
            scheduler.admit("fg", Lease::Interactive, 1000, 1000),
            Err(SchedError::InsufficientMemory)
        );
    }

    #[test]
    fn eviction_of_other_background_sessions_succeeds() {
        let mut scheduler = Scheduler::new(3000, 8000, 512, 1);
        scheduler.admit("bg1", Lease::Background, 1500, 100).unwrap();
        assert_eq!(
            scheduler.admit("bg2", Lease::Background, 1500, 100),
            Ok(())
        );
        assert!(scheduler.session("bg1").unwrap().parked);
    }

    #[test]
    fn eviction_fails_when_no_candidate_exists() {
        let mut scheduler = Scheduler::new(3000, 8000, 512, 1);
        scheduler.admit("fg", Lease::Interactive, 1000, 100).unwrap();
        assert_eq!(
            scheduler.admit("bg", Lease::Background, 2000, 100),
            Err(SchedError::InsufficientMemory)
        );
    }
}
