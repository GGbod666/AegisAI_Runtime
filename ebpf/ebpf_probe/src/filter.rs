use std::collections::BTreeSet;

use crate::event::EventTarget;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProbeFilter {
    pids: BTreeSet<u32>,
    tids: BTreeSet<u32>,
    cgroup_ids: BTreeSet<u64>,
    comms: BTreeSet<String>,
    comm_prefixes: Vec<String>,
}

impl ProbeFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allow_pid(mut self, pid: u32) -> Self {
        self.pids.insert(pid);
        self
    }

    pub fn allow_tid(mut self, tid: u32) -> Self {
        self.tids.insert(tid);
        self
    }

    pub fn allow_cgroup(mut self, cgroup_id: u64) -> Self {
        self.cgroup_ids.insert(cgroup_id);
        self
    }

    pub fn allow_comm(mut self, comm: impl Into<String>) -> Self {
        self.comms.insert(comm.into());
        self
    }

    pub fn allow_comm_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.comm_prefixes.push(prefix.into());
        self
    }

    pub fn is_unbounded(&self) -> bool {
        self.pids.is_empty()
            && self.tids.is_empty()
            && self.cgroup_ids.is_empty()
            && self.comms.is_empty()
            && self.comm_prefixes.is_empty()
    }

    pub fn matches(&self, target: &EventTarget) -> bool {
        let pid_match = self.pids.is_empty() || self.pids.contains(&target.pid);
        let tid_match = self.tids.is_empty() || self.tids.contains(&target.tid);
        let cgroup_match = if self.cgroup_ids.is_empty() {
            true
        } else {
            match target.cgroup_id {
                Some(cgroup_id) => self.cgroup_ids.contains(&cgroup_id),
                None => false,
            }
        };
        let comm_match = if self.comms.is_empty() && self.comm_prefixes.is_empty() {
            true
        } else {
            self.comms.contains(&target.comm)
                || self
                    .comm_prefixes
                    .iter()
                    .any(|prefix| target.comm.starts_with(prefix))
        };

        pid_match && tid_match && cgroup_match && comm_match
    }
}

#[cfg(test)]
mod tests {
    use super::ProbeFilter;
    use crate::event::EventTarget;

    #[test]
    fn filter_is_unbounded_by_default() {
        let filter = ProbeFilter::new();

        assert!(filter.is_unbounded());
    }

    #[test]
    fn filter_matches_all_configured_dimensions() {
        let filter = ProbeFilter::new()
            .allow_pid(1200)
            .allow_tid(1201)
            .allow_cgroup(99)
            .allow_comm_prefix("ollama");

        let target = EventTarget::new(1200, 1201, "ollama-worker").with_cgroup_id(99);

        assert!(filter.matches(&target));
    }

    #[test]
    fn filter_rejects_target_outside_scope() {
        let filter = ProbeFilter::new().allow_pid(2000);
        let target = EventTarget::new(1999, 1999, "stress-ng");

        assert!(!filter.matches(&target));
    }
}
