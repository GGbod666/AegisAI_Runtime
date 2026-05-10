use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CpuTopology {
    pub online_cpus: Option<Vec<u32>>,
}

impl CpuTopology {
    pub fn with_online_cpus(online_cpus: Option<Vec<u32>>) -> Self {
        Self {
            online_cpus: online_cpus
                .map(normalize_cpu_list)
                .filter(|cpus| !cpus.is_empty()),
        }
    }

    #[cfg(target_os = "linux")]
    pub fn discover() -> Self {
        Self::with_online_cpus(read_online_cpu_list())
    }

    #[cfg(not(target_os = "linux"))]
    pub fn discover() -> Self {
        Self::default()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpuAffinityCapture {
    pub configured_cpus: Vec<u32>,
    pub allowed_cpus: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpuAffinityTarget {
    cpus: Vec<u32>,
}

impl CpuAffinityTarget {
    pub fn new(cpus: Vec<u32>) -> Result<Self, String> {
        let cpus = normalize_cpu_list(cpus);
        if cpus.is_empty() {
            Err("missing original affinity cpu list".to_string())
        } else {
            Ok(Self { cpus })
        }
    }

    pub fn cpus(&self) -> &[u32] {
        &self.cpus
    }

    pub fn to_taskset_list(&self) -> String {
        format_cpu_list(&self.cpus)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CpuAffinityPlanner {
    topology: CpuTopology,
}

impl CpuAffinityPlanner {
    pub fn new(topology: CpuTopology) -> Self {
        Self { topology }
    }

    pub fn discover() -> Self {
        Self::new(CpuTopology::discover())
    }

    pub fn with_online_cpus(online_cpus: Option<Vec<u32>>) -> Self {
        Self::new(CpuTopology::with_online_cpus(online_cpus))
    }

    pub fn plan_capture(&self, configured_cpus: Vec<u32>) -> Option<CpuAffinityCapture> {
        let configured_cpus = normalize_cpu_list(configured_cpus);
        if configured_cpus.is_empty() {
            return None;
        }

        let allowed_cpus = intersect_allowed_cpus(
            configured_cpus.as_slice(),
            self.topology.online_cpus.as_deref(),
        );
        Some(CpuAffinityCapture {
            configured_cpus,
            allowed_cpus,
        })
    }

    pub fn plan_apply_target(
        &self,
        strategy: &str,
        max_cpu_ratio: f32,
        allowed_cpus: &[u32],
    ) -> Result<CpuAffinityTarget, String> {
        if allowed_cpus.is_empty() {
            return Err("missing original affinity cpu list".to_string());
        }
        if !max_cpu_ratio.is_finite() {
            return Err("invalid affinity max_cpu_ratio".to_string());
        }

        let mut cpus = normalize_cpu_list(allowed_cpus.to_vec());
        let bounded_ratio = max_cpu_ratio.clamp(0.0, 1.0);
        let target_count = ((cpus.len() as f32) * bounded_ratio).ceil().max(1.0) as usize;

        match strategy {
            "prefer_low_contention_cores" => cpus.sort_unstable_by(|left, right| right.cmp(left)),
            "prefer_reserved_cores" => cpus.sort_unstable(),
            _ => cpus.sort_unstable(),
        }
        cpus.truncate(target_count.min(cpus.len()));
        CpuAffinityTarget::new(cpus)
    }

    pub fn plan_rollback_target(&self, allowed_cpus: &[u32]) -> Result<CpuAffinityTarget, String> {
        CpuAffinityTarget::new(allowed_cpus.to_vec())
    }
}

pub fn parse_status_cpu_list(raw: &str) -> Option<Vec<u32>> {
    let cpu_list = raw
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
        .map(str::trim)?;

    parse_cpu_list(cpu_list)
}

pub fn parse_cpu_list(cpu_list: &str) -> Option<Vec<u32>> {
    let mut cpus = Vec::new();
    for segment in cpu_list.split(',').filter(|segment| !segment.is_empty()) {
        if let Some((start, end)) = segment.split_once('-') {
            let start = start.trim().parse::<u32>().ok()?;
            let end = end.trim().parse::<u32>().ok()?;
            if start > end {
                return None;
            }
            cpus.extend(start..=end);
        } else {
            cpus.push(segment.trim().parse::<u32>().ok()?);
        }
    }

    if cpus.is_empty() {
        None
    } else {
        Some(cpus)
    }
}

#[cfg(target_os = "linux")]
fn read_online_cpu_list() -> Option<Vec<u32>> {
    std::fs::read_to_string("/sys/devices/system/cpu/online")
        .ok()
        .and_then(|raw| parse_cpu_list(raw.trim()))
}

fn intersect_allowed_cpus(configured_cpus: &[u32], online_cpus: Option<&[u32]>) -> Vec<u32> {
    let configured_cpus = normalize_cpu_list(configured_cpus.to_vec());
    let Some(online_cpus) = online_cpus else {
        return configured_cpus;
    };

    let online_set = normalize_cpu_list(online_cpus.to_vec())
        .into_iter()
        .collect::<BTreeSet<_>>();
    let online_intersection = configured_cpus
        .iter()
        .copied()
        .filter(|cpu| online_set.contains(cpu))
        .collect::<Vec<_>>();

    online_intersection
}

fn normalize_cpu_list(mut cpus: Vec<u32>) -> Vec<u32> {
    cpus.sort_unstable();
    cpus.dedup();
    cpus
}

fn format_cpu_list(cpus: &[u32]) -> String {
    cpus.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cpu_list_expands_ranges() {
        assert_eq!(parse_cpu_list("0-2,4,6-7"), Some(vec![0, 1, 2, 4, 6, 7]));
        assert_eq!(parse_cpu_list("3-1"), None);
    }

    #[test]
    fn parse_status_cpu_list_extracts_configured_affinity() {
        let status = "Name:\tollama\nCpus_allowed_list:\t0-2,4\nMems_allowed_list:\t0\n";

        assert_eq!(parse_status_cpu_list(status), Some(vec![0, 1, 2, 4]));
    }

    #[test]
    fn planner_prefers_effective_online_subset_for_configured_cpu_mismatch() {
        let planner = CpuAffinityPlanner::with_online_cpus(Some(vec![0, 1, 2, 3]));
        let capture = planner
            .plan_capture((0..=127).collect())
            .expect("affinity capture");

        assert_eq!(capture.configured_cpus.len(), 128);
        assert_eq!(capture.allowed_cpus, vec![0, 1, 2, 3]);
    }

    #[test]
    fn planner_intersects_proc_status_allowed_list_with_online_cpus() {
        let status = "Name:\tollama\nCpus_allowed_list:\t0-7,16,32\nMems_allowed_list:\t0\n";
        let configured_cpus = parse_status_cpu_list(status).expect("procfs CPU list");
        let online_cpus = parse_cpu_list("1-3,6,8").expect("online CPU list");
        let planner = CpuAffinityPlanner::with_online_cpus(Some(online_cpus));
        let capture = planner
            .plan_capture(configured_cpus)
            .expect("affinity capture");

        assert_eq!(capture.allowed_cpus, vec![1, 2, 3, 6]);

        let target = planner
            .plan_apply_target("prefer_reserved_cores", 0.5, &capture.allowed_cpus)
            .expect("affinity target");

        assert_eq!(target.cpus(), &[1, 2]);
        assert_eq!(target.to_taskset_list(), "1,2");
    }

    #[test]
    fn planner_uses_restricted_vm_online_mask_for_taskset_targets() {
        let status = "Name:\tollama\nCpus_allowed_list:\t0-127\nMems_allowed_list:\t0\n";
        let configured_cpus = parse_status_cpu_list(status).expect("procfs CPU list");
        let online_cpus = parse_cpu_list("0-3").expect("online CPU list");
        let planner = CpuAffinityPlanner::with_online_cpus(Some(online_cpus));
        let capture = planner
            .plan_capture(configured_cpus)
            .expect("affinity capture");

        assert_eq!(capture.configured_cpus.len(), 128);
        assert_eq!(capture.allowed_cpus, vec![0, 1, 2, 3]);

        let target = planner
            .plan_apply_target("prefer_low_contention_cores", 0.5, &capture.allowed_cpus)
            .expect("affinity target");

        assert_eq!(target.cpus(), &[2, 3]);
        assert_eq!(target.to_taskset_list(), "2,3");
    }

    #[test]
    fn planner_does_not_select_taskset_target_for_empty_online_intersection() {
        let status = "Name:\tollama\nCpus_allowed_list:\t4-5\nMems_allowed_list:\t0\n";
        let configured_cpus = parse_status_cpu_list(status).expect("procfs CPU list");
        let planner = CpuAffinityPlanner::with_online_cpus(Some(vec![0, 1]));
        let capture = planner
            .plan_capture(configured_cpus)
            .expect("affinity capture");

        assert!(capture.allowed_cpus.is_empty());
        assert!(planner
            .plan_apply_target("prefer_reserved_cores", 0.5, &capture.allowed_cpus)
            .is_err());
    }

    #[test]
    fn planner_falls_back_when_online_is_unavailable() {
        let unavailable = CpuAffinityPlanner::with_online_cpus(None)
            .plan_capture(vec![0, 1, 2, 3])
            .expect("affinity capture");
        assert_eq!(unavailable.allowed_cpus, vec![0, 1, 2, 3]);
    }

    #[test]
    fn planner_selects_reserved_core_target_from_lowest_allowed_cpus() {
        let planner = CpuAffinityPlanner::default();
        let target = planner
            .plan_apply_target("prefer_reserved_cores", 0.5, &[4, 0, 2])
            .expect("affinity target");

        assert_eq!(target.cpus(), &[0, 2]);
        assert_eq!(target.to_taskset_list(), "0,2");
    }

    #[test]
    fn planner_selects_low_contention_target_from_highest_allowed_cpus() {
        let planner = CpuAffinityPlanner::default();
        let target = planner
            .plan_apply_target("prefer_low_contention_cores", 0.5, &[0, 1, 2, 3, 4, 5])
            .expect("affinity target");

        assert_eq!(target.cpus(), &[3, 4, 5]);
        assert_eq!(target.to_taskset_list(), "3,4,5");
    }

    #[test]
    fn planner_formats_rollback_target_from_allowed_cpus() {
        let planner = CpuAffinityPlanner::default();
        let target = planner
            .plan_rollback_target(&[4, 0, 2, 2])
            .expect("rollback target");

        assert_eq!(target.cpus(), &[0, 2, 4]);
        assert_eq!(target.to_taskset_list(), "0,2,4");
    }

    #[test]
    fn planner_generates_deterministic_rollback_targets() {
        let planner = CpuAffinityPlanner::default();

        for allowed_cpus in [vec![4, 0, 2, 2], vec![2, 4, 0], vec![0, 2, 4]] {
            let target = planner
                .plan_rollback_target(&allowed_cpus)
                .expect("rollback target");

            assert_eq!(target.cpus(), &[0, 2, 4]);
            assert_eq!(target.to_taskset_list(), "0,2,4");
        }
    }
}
