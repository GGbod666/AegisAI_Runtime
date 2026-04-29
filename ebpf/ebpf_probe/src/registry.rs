use std::collections::BTreeMap;

use crate::probe::{ProbeDescriptor, ProbeKind};

#[derive(Debug, Clone, Default)]
pub struct ProbeRegistry {
    descriptors: BTreeMap<ProbeKind, ProbeDescriptor>,
}

impl ProbeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        for kind in ProbeKind::ALL {
            registry.register(ProbeDescriptor::default_for(kind));
        }

        registry
    }

    pub fn register(&mut self, descriptor: ProbeDescriptor) -> Option<ProbeDescriptor> {
        self.descriptors.insert(descriptor.kind, descriptor)
    }

    pub fn get(&self, kind: ProbeKind) -> Option<&ProbeDescriptor> {
        self.descriptors.get(&kind)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ProbeDescriptor> {
        self.descriptors.values()
    }

    pub fn len(&self) -> usize {
        self.descriptors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }
}

impl FromIterator<ProbeDescriptor> for ProbeRegistry {
    fn from_iter<T: IntoIterator<Item = ProbeDescriptor>>(iter: T) -> Self {
        let mut registry = Self::new();

        for descriptor in iter {
            registry.register(descriptor);
        }

        registry
    }
}

#[cfg(test)]
mod tests {
    use super::ProbeRegistry;
    use crate::probe::ProbeKind;

    #[test]
    fn default_registry_contains_first_wave_probes() {
        let registry = ProbeRegistry::with_defaults();

        assert_eq!(registry.len(), 4);
        assert!(registry.get(ProbeKind::Sched).is_some());
        assert!(registry.get(ProbeKind::OffCpu).is_some());
        assert!(registry.get(ProbeKind::Fault).is_some());
        assert!(registry.get(ProbeKind::Io).is_some());
    }
}
