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

// `MASK` is a `u64`; `Status::Bits` is `u32`.
const FROZEN: u64 = Status::FROZEN.bits() as u64;
const SLOWED: u64 = Status::SLOWED.bits() as u64;
const BURNING: u64 = Status::BURNING.bits() as u64;
const FROZEN_OR_SLOWED: u64 = FROZEN | SLOWED;

fn frozen(q: Query<&Status, WithFlags<Status, FROZEN>>) {}

fn affected(q: Query<&Status, WithAnyFlags<Status, FROZEN_OR_SLOWED>>) {}

fn clear(q: Query<&Status, WithoutFlags<Status, FROZEN_OR_SLOWED>>) {}
```

Masks can also be written inline:
`WithFlags<Status, { Status::FROZEN.bits() as u64 }>`.

## Semantics

For a row whose bits are `bits` and a filter mask `MASK`:

| Filter         | Matches when              | `bitflags` predicate |
| -------------- | ------------------------- | -------------------- |
| `WithFlags`    | every mask bit is set     | `contains`           |
| `WithAnyFlags` | at least one mask bit set | `intersects`         |
| `WithoutFlags` | no mask bit is set        | `is_disjoint`        |

All three require the component to be present, like `With<C>` combined with a
per-row bit test. `WithoutFlags` is not `Without<C>`.

A zero mask matches every entity that has `C` for `WithFlags`/`WithoutFlags`,
and none for `WithAnyFlags`.

## Notes

- The filters are per-row, not archetype filters. Changing a bit does not
  move the entity or fragment the table.
- `IS_ARCHETYPAL = false`, so they cannot be used with
  `contiguous_iter`/`contiguous_par_iter`. Hot loops should read the flags
  component as a normal column and check bits themselves.
- Filters run before a row's item is constructed, so
  `Query<&mut Status, WithFlags<Status, MASK>>` is allowed.
- `MASK` is a `u64`; the component must implement `Into<u64>` for its `Bits`
  type (`u8`, `u16`, `u32`, `u64`). Masks with bits above the storage width
  panic at query initialization instead of silently matching nothing.
- The mask is a const generic, so masks must be known at compile time.
