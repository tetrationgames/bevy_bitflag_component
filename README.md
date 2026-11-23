# bevy_bitflag_component

Row-level `bitflags` query filters for Bevy ECS.

A `bitflags` type can derive `Component` directly. This crate adds filters
that check individual bits without creating a new archetype for every flag
combination.

```rust
use bevy_bitflag_component::{WithAnyFlags, WithFlags, WithoutFlags};
use bevy_ecs::prelude::*;

bitflags::bitflags! {
    #[derive(Component, Clone, Copy)]
    pub struct Status: u32 {
        const FROZEN  = 1 << 0;
        const SLOWED  = 1 << 1;
        const BURNING = 1 << 2;
    }
}

const FROZEN: u64 = Status::FROZEN.bits() as u64;
const SLOWED: u64 = Status::SLOWED.bits() as u64;

fn frozen(q: Query<&Status, WithFlags<Status, FROZEN>>) {
    // ...
}

fn affected(q: Query<&Status, WithAnyFlags<Status, { FROZEN | SLOWED }>>) {
    // ...
}

fn clear(q: Query<&Status, WithoutFlags<Status, { FROZEN | SLOWED }>>) {
    // ...
}
```

## Notes

- The filters are per-row, not archetype filters. Changing a bit does not
  move the entity or fragment the table.
- Because they are not `ArchetypeFilter`s, they cannot be used directly in
  `contiguous_iter`. Hot loops should read the flags component as a normal
  column and check bits themselves.
- `bitflags::Flags` is used directly. No separate component trait is
  required.
- The mask is a `u64`; flags storage must implement `Into<u64>`
  (`u8`, `u16`, `u32`, `u64`).
