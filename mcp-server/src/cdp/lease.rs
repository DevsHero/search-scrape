//! LC/M6 WB-02 — per-profile browser lease.
//!
//! A lease is keyed by the canonical profile directory path and held by one
//! job id. Another job cannot acquire, steal, or close through it while it is
//! live. Prototype scope: in-process table, serial-per-profile.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileLease {
    pub profile_key: String,
    pub holder_job_id: String,
    pub lease_id: String,
    pub acquired_at: Instant,
    pub ttl: Duration,
}

impl ProfileLease {
    pub fn is_live(&self, now: Instant) -> bool {
        now.duration_since(self.acquired_at) < self.ttl
    }
}

fn table() -> &'static Mutex<BTreeMap<String, ProfileLease>> {
    static TABLE: OnceLock<Mutex<BTreeMap<String, ProfileLease>>> = OnceLock::new();
    TABLE.get_or_init(Default::default)
}

pub fn profile_key_for(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

/// Acquire the lease for one profile. Returns Err when a *different* live
/// job already holds it. Same holder re-acquiring refreshes the TTL.
pub fn acquire(profile_key: &str, job_id: &str, ttl: Duration) -> Result<ProfileLease, String> {
    if profile_key.trim().is_empty() || job_id.trim().is_empty() {
        return Err("profile key and job id are required".into());
    }
    let mut guard = table()
        .lock()
        .map_err(|_| "lease table poisoned".to_string())?;
    let now = Instant::now();
    if let Some(existing) = guard.get(profile_key) {
        if existing.holder_job_id != job_id || !existing.is_live(now) {
            return Err(format!(
                "profile lease held by job {}; refusing steal or shared close",
                existing.holder_job_id
            ));
        }
        return Ok(existing.clone());
    }
    let lease = ProfileLease {
        profile_key: profile_key.to_string(),
        holder_job_id: job_id.to_string(),
        lease_id: format!("lease:{}", uuid::Uuid::new_v4()),
        acquired_at: now,
        ttl,
    };
    guard.insert(profile_key.to_string(), lease.clone());
    Ok(lease)
}

pub fn status(profile_key: &str) -> Option<ProfileLease> {
    let guard = table().lock().ok()?;
    let lease = guard.get(profile_key)?.clone();
    if lease.is_live(Instant::now()) {
        Some(lease)
    } else {
        None
    }
}

/// Release ONLY when the caller is the holder. A wrong holder gets an error
/// and the lease remains untouched.
pub fn release(profile_key: &str, job_id: &str) -> Result<(), String> {
    let mut guard = table()
        .lock()
        .map_err(|_| "lease table poisoned".to_string())?;
    match guard.get(profile_key) {
        None => Ok(()),
        Some(lease) if lease.holder_job_id == job_id => {
            guard.remove(profile_key);
            Ok(())
        }
        Some(lease) => Err(format!(
            "profile lease held by job {}; {} cannot release it",
            lease.holder_job_id, job_id
        )),
    }
}

/// Refuse browser-session close for a profile that another live lease holds.
pub fn assert_closable(profile_key: &str, job_id: &str) -> Result<(), String> {
    let guard = table().lock().map_err(|_| "lease table poisoned")?;
    if let Some(lease) = guard.get(profile_key) {
        if lease.holder_job_id != job_id {
            return Err(format!(
                "browser session is leased by job {}; close refused",
                lease.holder_job_id
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leases_are_serial_per_profile_and_cannot_be_stolen_or_closed_by_others() {
        use crate::webchat::{BrowserSessionReadiness, ProbeOutcome, ReadinessState};
        let mut readiness =
            BrowserSessionReadiness::needs_user("l", "p", "j", "http://127.0.0.1:1");
        readiness.transition_login_required();
        readiness.transition_user_done();
        assert_eq!(readiness.state, ReadinessState::UserDonePendingProbe);
        assert!(!readiness.is_ready());
        readiness.apply_probe(ProbeOutcome::Ready, false);
        assert!(!readiness.is_ready());
        readiness.apply_probe(ProbeOutcome::Ready, true);
        assert!(readiness.is_ready());
        readiness.mark_expired();
        readiness.apply_probe(ProbeOutcome::Ready, true);
        assert!(!readiness.is_ready());
        let key = format!("profile-test-{}", uuid::Uuid::new_v4());
        let first = acquire(&key, "job-a", Duration::from_secs(60)).unwrap();
        assert!(acquire(&key, "job-b", Duration::from_secs(60)).is_err());
        // Same holder refreshes rather than duplicates.
        let refreshed = acquire(&key, "job-a", Duration::from_secs(60)).unwrap();
        assert_eq!(refreshed.holder_job_id, first.holder_job_id);
        assert!(
            release(&key, "job-b").is_err(),
            "wrong holder cannot release"
        );
        assert!(
            assert_closable(&key, "job-b").is_err(),
            "wrong holder cannot close"
        );
        assert!(assert_closable(&key, "job-a").is_ok());
        release(&key, "job-a").unwrap();
        assert!(status(&key).is_none());
        acquire(&key, "job-b", Duration::from_millis(10)).unwrap();
        std::thread::sleep(Duration::from_millis(25));
        assert!(status(&key).is_none(), "expired lease stops blocking");
        assert!(acquire(&key, "job-c", Duration::from_secs(60)).is_err());
        release(&key, "job-b").unwrap();
        acquire(&key, "job-c", Duration::from_secs(60)).unwrap();
        release(&key, "job-c").unwrap();
    }
}
