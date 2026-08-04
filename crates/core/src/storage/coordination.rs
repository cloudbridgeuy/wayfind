//! The coordination domain's storage capabilities.
//!
//! The immutable graph names its records by content hash and needs no
//! allocator; this is for the rowid-identified entities the coordination
//! domain still has.

use crate::storage::values::{AllocatedId, IdScope, StorageResult};

/// Handing out identifiers.
///
/// The promise is monotonic and unique, with gaps allowed. A caller that
/// allocates and then fails has burned an identifier; nothing reuses it.
pub trait MutableIdAllocator {
    /// Take the next identifier from a scope.
    fn allocate(&self, scope: IdScope) -> StorageResult<AllocatedId>;
}

/// Everything the coordination domain needs from a store.
pub trait CoordinationStorage: MutableIdAllocator {}

impl<T> CoordinationStorage for T where T: MutableIdAllocator {}

#[cfg(test)]
mod tests {
    use super::CoordinationStorage;

    /// Compiles only while [`CoordinationStorage`] stays object safe.
    fn accepts_any(_s: &dyn CoordinationStorage) {}

    #[test]
    fn coordination_storage_is_object_safe() {
        let _: fn(&dyn CoordinationStorage) = accepts_any;
    }
}
