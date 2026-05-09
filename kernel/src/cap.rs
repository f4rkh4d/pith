// capability tables. each process owns CAP_SLOTS handles. user-space
// addresses kernel objects through these handle integers, never with a
// raw pointer or pid. capability install/grant operations are kernel-
// internal in v0.5 (sel4-style cap derivation lives behind v0.6).

use crate::ipc;

pub const CAP_SLOTS: usize = 16;

#[derive(Clone, Copy)]
pub enum Cap {
    Empty,
    /// reference to one of the kernel's endpoint objects.
    Endpoint(ipc::EndpointId),
}

#[derive(Clone, Copy)]
pub struct CapTable {
    pub slots: [Cap; CAP_SLOTS],
}

impl CapTable {
    pub const fn empty() -> Self {
        Self { slots: [Cap::Empty; CAP_SLOTS] }
    }

    pub fn install(&mut self, slot: usize, cap: Cap) -> Result<(), &'static str> {
        if slot >= CAP_SLOTS { return Err("slot out of range"); }
        self.slots[slot] = cap;
        Ok(())
    }

    pub fn get(&self, slot: usize) -> Option<&Cap> {
        if slot >= CAP_SLOTS { return None; }
        match &self.slots[slot] {
            Cap::Empty => None,
            other => Some(other),
        }
    }

    pub fn delete(&mut self, slot: usize) -> Result<(), &'static str> {
        if slot >= CAP_SLOTS { return Err("slot out of range"); }
        self.slots[slot] = Cap::Empty;
        Ok(())
    }

    /// derive: copy the cap at `src` into the (possibly empty) `dst`
    /// slot. Endpoint caps duplicate by reference — both handles point
    /// at the same kernel object — which is the seL4 derivation rule
    /// for non-untyped caps.
    pub fn dupe(&mut self, src: usize, dst: usize) -> Result<(), &'static str> {
        if src >= CAP_SLOTS || dst >= CAP_SLOTS {
            return Err("slot out of range");
        }
        let c = self.slots[src];
        if matches!(c, Cap::Empty) {
            return Err("source slot empty");
        }
        self.slots[dst] = c;
        Ok(())
    }
}
