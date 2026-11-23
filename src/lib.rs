//! Row-level [`bitflags`] filters for Bevy ECS.
//!
//! A [`bitflags`] type can derive [`Component`] directly, so its bits live in a
//! normal component column:
//!
//! ```
//! use bevy_ecs::prelude::*;
//!
//! bitflags::bitflags! {
//!     #[derive(Component, Clone, Copy, Debug)]
//!     pub struct Status: u32 {
//!         const FROZEN = 1 << 0;
//!         const SLOWED = 1 << 1;
//!         const BURNING = 1 << 2;
//!     }
//! }
//! ```
//!
//! The filters in this crate select rows by those bits:
//!
//! ```
//! use bevy_ecs::prelude::*;
//! use bevy_bitflag_component::{WithAnyFlags, WithFlags, WithoutFlags};
//!
//! # bitflags::bitflags! {
//! #     #[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
//! #     pub struct Status: u32 {
//! #         const FROZEN = 1 << 0;
//! #         const SLOWED = 1 << 1;
//! #         const BURNING = 1 << 2;
//! #     }
//! # }
//! // `MASK` is a `u64`; `Status::Bits` is `u32`.
//! const FROZEN: u64 = Status::FROZEN.bits() as u64;
//! const SLOWED: u64 = Status::SLOWED.bits() as u64;
//! const FROZEN_OR_SLOWED: u64 = FROZEN | SLOWED;
//!
//! fn frozen(q: Query<&Status, WithFlags<Status, FROZEN>>) {}
//! fn affected(q: Query<&Status, WithAnyFlags<Status, FROZEN_OR_SLOWED>>) {}
//! fn calm(q: Query<&Status, WithoutFlags<Status, FROZEN_OR_SLOWED>>) {}
//!
//! let mut world = World::new();
//! world.spawn(Status::FROZEN);
//! world.spawn(Status::FROZEN | Status::SLOWED);
//! world.spawn(Status::BURNING);
//!
//! let mut query = world.query_filtered::<Entity, WithFlags<Status, FROZEN>>();
//! assert_eq!(query.iter(&world).count(), 2);
//! ```
//!
//! An expression can also be written inline in the mask position:
//! `WithFlags<Status, { Status::FROZEN.bits() as u64 }>`.
//!
//! # Semantics
//!
//! For a row whose `C` bits are `bits` and a filter mask `MASK`:
//!
//! | Filter | Matches when | `bitflags` predicate |
//! |---|---|---|
//! | [`WithFlags`] | every mask bit is set | `contains` |
//! | [`WithAnyFlags`] | at least one mask bit is set | `intersects` |
//! | [`WithoutFlags`] | no mask bit is set | `is_disjoint` |
//!
//! All three also require `C` to be present on the entity: they behave like
//! `With<C>` combined with a per-row bit test. In particular, [`WithoutFlags`]
//! is not [`Without<C>`](bevy_ecs::query::Without).
//!
//! A zero `MASK` follows from the definitions: [`WithFlags`] and
//! [`WithoutFlags`] match every entity that has `C`, while [`WithAnyFlags`]
//! matches none.
//!
//! # Iteration
//!
//! These are *row* filters, not archetype filters (`IS_ARCHETYPAL = false`):
//! changing a bit does not move the entity, and the filter cannot be used with
//! `contiguous_iter`/`contiguous_par_iter`, which require an
//! [`ArchetypeFilter`](bevy_ecs::query::ArchetypeFilter). Hot loops that need
//! contiguous access should take `&C` as normal query data and test the bits
//! themselves.
//!
//! Filters are evaluated before a row's query item is constructed, so they can
//! be combined with mutable access to the same component:
//! `Query<&mut C, WithFlags<C, MASK>>` is allowed.
//!
//! # Mask type
//!
//! `MASK` is a `u64`, and `C::Bits` must convert into `u64` (`u8`, `u16`,
//! `u32`, `u64`). The mask is checked when the query is initialized: bits that
//! do not fit in `C::Bits` panic instead of silently never matching. Because
//! the mask is a const generic, masks must be known at compile time.

use core::{any::type_name, marker::PhantomData};

use bevy_ecs::{
    archetype::Archetype,
    change_detection::Tick,
    component::{Component, ComponentId, Components, StorageType},
    entity::Entity,
    query::{FilteredAccess, FilteredAccessSet, QueryData, QueryFilter, WorldQuery},
    storage::{Table, TableRow},
    world::{unsafe_world_cell::UnsafeWorldCell, World},
};
use bitflags::{Bits, Flags};

/// Matches rows where all bits in `MASK` are set (`flags.contains(mask)`).
///
/// Also requires `C` to be present on the entity.
///
/// ```
/// # use bevy_ecs::prelude::*;
/// # use bevy_bitflag_component::WithFlags;
/// # bitflags::bitflags! { #[derive(Component, Clone, Copy)] pub struct Status: u32 { const FROZEN = 1; const SLOWED = 2; } }
/// const FROZEN_OR_SLOWED: u64 = Status::FROZEN.bits() as u64 | Status::SLOWED.bits() as u64;
/// fn frozen(q: Query<&Status, WithFlags<Status, FROZEN_OR_SLOWED>>) {}
/// ```
pub struct WithFlags<C, const MASK: u64>(PhantomData<C>)
where
    C: Component + Flags,
    C::Bits: Into<u64>;

/// Matches rows where at least one bit in `MASK` is set
/// (`flags.intersects(mask)`).
///
/// Also requires `C` to be present on the entity.
///
/// ```
/// # use bevy_ecs::prelude::*;
/// # use bevy_bitflag_component::WithAnyFlags;
/// # bitflags::bitflags! { #[derive(Component, Clone, Copy)] pub struct Status: u32 { const FROZEN = 1; const SLOWED = 2; } }
/// const FROZEN_OR_SLOWED: u64 = Status::FROZEN.bits() as u64 | Status::SLOWED.bits() as u64;
/// fn affected(q: Query<&Status, WithAnyFlags<Status, FROZEN_OR_SLOWED>>) {}
/// ```
pub struct WithAnyFlags<C, const MASK: u64>(PhantomData<C>)
where
    C: Component + Flags,
    C::Bits: Into<u64>;

/// Matches rows where no bit in `MASK` is set (`flags.is_disjoint(mask)`).
///
/// Also requires `C` to be present on the entity; this is not
/// `Without<C>`, which matches entities *lacking* the component.
///
/// ```
/// # use bevy_ecs::prelude::*;
/// # use bevy_bitflag_component::WithoutFlags;
/// # bitflags::bitflags! { #[derive(Component, Clone, Copy)] pub struct Status: u32 { const FROZEN = 1; const SLOWED = 2; } }
/// const FROZEN_OR_SLOWED: u64 = Status::FROZEN.bits() as u64 | Status::SLOWED.bits() as u64;
/// fn calm(q: Query<&Status, WithoutFlags<Status, FROZEN_OR_SLOWED>>) {}
/// ```
pub struct WithoutFlags<C, const MASK: u64>(PhantomData<C>)
where
    C: Component + Flags,
    C::Bits: Into<u64>;

/// Panics if `mask` has bits that the storage type of `C` cannot hold.
///
/// Without this check such a mask is silently unmatchable: `bits & mask` can
/// never contain a bit above `C::Bits::MAX`.
#[inline]
fn check_mask<C>(mask: u64)
where
    C: Flags,
    C::Bits: Into<u64>,
{
    let storage_mask: u64 = <C::Bits as Bits>::ALL.into();
    assert!(
        mask & !storage_mask == 0,
        "mask {mask:#x} does not fit in the `Bits` type of {}: only the low {storage_mask:#x} bits can be set",
        type_name::<C>(),
    );
}

macro_rules! impl_flag_filter {
    ($name:ident, $matches:expr) => {
        // SAFETY: The filter only reads `C`; the read is registered in
        // `update_component_access`, and `matches_component_set` only matches
        // component sets that contain `C`.
        unsafe impl<C, const MASK: u64> WorldQuery for $name<C, MASK>
        where
            C: Component + Flags,
            C::Bits: Into<u64>,
        {
            type Fetch<'w> = <&'w C as WorldQuery>::Fetch<'w>;
            type State = ComponentId;

            #[inline]
            fn shrink_fetch<'wlong: 'wshort, 'wshort>(
                fetch: Self::Fetch<'wlong>,
            ) -> Self::Fetch<'wshort> {
                <&C as WorldQuery>::shrink_fetch(fetch)
            }

            #[inline]
            unsafe fn init_fetch<'w>(
                world: UnsafeWorldCell<'w>,
                state: &Self::State,
                last_run: Tick,
                this_run: Tick,
            ) -> Self::Fetch<'w> {
                <&C as WorldQuery>::init_fetch(world, state, last_run, this_run)
            }

            const IS_DENSE: bool = matches!(C::STORAGE_TYPE, StorageType::Table);

            #[inline]
            unsafe fn set_archetype<'w>(
                fetch: &mut Self::Fetch<'w>,
                state: &Self::State,
                archetype: &'w Archetype,
                table: &'w Table,
            ) {
                <&C as WorldQuery>::set_archetype(fetch, state, archetype, table);
            }

            #[inline]
            unsafe fn set_table<'w>(
                fetch: &mut Self::Fetch<'w>,
                state: &Self::State,
                table: &'w Table,
            ) {
                <&C as WorldQuery>::set_table(fetch, state, table);
            }

            #[inline]
            fn update_component_access(state: &Self::State, access: &mut FilteredAccess) {
                check_mask::<C>(MASK);
                access.add_read(*state);
            }

            fn init_nested_access(
                _state: &Self::State,
                _system_name: Option<&str>,
                _component_access_set: &mut FilteredAccessSet,
                _world: UnsafeWorldCell,
            ) {
            }

            #[inline]
            fn init_state(world: &mut World) -> Self::State {
                world.register_component::<C>()
            }

            #[inline]
            fn get_state(components: &Components) -> Option<Self::State> {
                components.component_id::<C>()
            }

            #[inline]
            fn matches_component_set(
                state: &Self::State,
                set_contains_id: &impl Fn(ComponentId) -> bool,
            ) -> bool {
                set_contains_id(*state)
            }

            fn update_archetypes(_state: &mut Self::State, _world: UnsafeWorldCell) {}
        }

        // SAFETY: The filter only reads `C`, and `update_component_access`
        // registers that read.
        unsafe impl<C, const MASK: u64> QueryFilter for $name<C, MASK>
        where
            C: Component + Flags,
            C::Bits: Into<u64>,
        {
            const IS_ARCHETYPAL: bool = false;

            #[inline]
            unsafe fn filter_fetch(
                state: &Self::State,
                fetch: &mut Self::Fetch<'_>,
                entity: Entity,
                table_row: TableRow,
            ) -> bool {
                // SAFETY: `fetch` was initialized by `init_fetch` and positioned
                // for the current table/archetype by `set_table`/`set_archetype`;
                // the caller guarantees `table_row` and `entity` are valid rows.
                let Some(component) =
                    (unsafe { <&C as QueryData>::fetch(state, fetch, entity, table_row) })
                else {
                    return false;
                };
                let bits: u64 = <C as Flags>::bits(component).into();
                $matches(bits, MASK)
            }
        }
    };
}

impl_flag_filter!(WithFlags, |bits, mask| bits & mask == mask);
impl_flag_filter!(WithAnyFlags, |bits, mask| bits & mask != 0);
impl_flag_filter!(WithoutFlags, |bits, mask| bits & mask == 0);

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::prelude::*;
    use bevy_ecs::system::RunSystemOnce;

    bitflags::bitflags! {
        #[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
        pub struct Status: u32 {
            const FROZEN = 1 << 0;
            const SLOWED = 1 << 1;
            const BURNING = 1 << 2;
        }
    }

    bitflags::bitflags! {
        #[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
        #[component(storage = "SparseSet")]
        pub struct SparseStatus: u8 {
            const A = 1 << 0;
            const B = 1 << 1;
        }
    }

    bitflags::bitflags! {
        #[derive(Component, Clone, Copy, Debug)]
        pub struct Ghost: u8 {
            const X = 1 << 0;
        }
    }

    #[derive(Component)]
    struct Other;

    const FROZEN: u64 = Status::FROZEN.bits() as u64;
    const SLOWED: u64 = Status::SLOWED.bits() as u64;
    const BURNING: u64 = Status::BURNING.bits() as u64;
    const FROZEN_OR_SLOWED: u64 = FROZEN | SLOWED;
    const FROZEN_OR_BURNING: u64 = FROZEN | BURNING;

    const SPARSE_A: u64 = SparseStatus::A.bits() as u64;
    const SPARSE_B: u64 = SparseStatus::B.bits() as u64;
    const SPARSE_A_OR_B: u64 = SPARSE_A | SPARSE_B;

    const GHOST_X: u64 = Ghost::X.bits() as u64;

    fn ids<F: QueryFilter>(world: &mut World) -> Vec<Entity> {
        let mut query = world.query_filtered::<Entity, F>();
        let mut ids: Vec<Entity> = query.iter(world).collect();
        ids.sort();
        ids
    }

    fn sorted(mut ids: Vec<Entity>) -> Vec<Entity> {
        ids.sort();
        ids
    }

    #[test]
    fn component_can_be_derived_inside_bitflags() {
        let mut world = World::new();
        let entity = world.spawn(Status::FROZEN).id();
        assert_eq!(*world.get::<Status>(entity).unwrap(), Status::FROZEN);
    }

    #[test]
    fn all_any_none_filters() {
        let mut world = World::new();
        let frozen = world.spawn(Status::FROZEN).id();
        let slowed = world.spawn(Status::SLOWED).id();
        let frozen_burning = world.spawn(Status::FROZEN | Status::BURNING).id();
        let empty = world.spawn(Status::empty()).id();
        let with_other_component = world.spawn((Status::FROZEN, Other)).id();

        assert_eq!(
            ids::<WithFlags<Status, FROZEN>>(&mut world),
            sorted(vec![frozen, frozen_burning, with_other_component]),
        );
        assert_eq!(
            ids::<WithAnyFlags<Status, FROZEN_OR_SLOWED>>(&mut world),
            sorted(vec![frozen, slowed, frozen_burning, with_other_component]),
        );
        assert_eq!(
            ids::<WithoutFlags<Status, FROZEN>>(&mut world),
            sorted(vec![slowed, empty]),
        );
        assert_eq!(
            ids::<WithFlags<Status, FROZEN_OR_BURNING>>(&mut world),
            sorted(vec![frozen_burning]),
        );
        assert_eq!(
            ids::<WithFlags<Status, { Status::FROZEN.bits() as u64 }>>(&mut world),
            ids::<WithFlags<Status, FROZEN>>(&mut world),
        );
    }

    #[test]
    fn filters_require_the_component() {
        let mut world = World::new();
        let only_other = world.spawn(Other).id();
        let empty = world.spawn(Status::empty()).id();

        let without = ids::<WithoutFlags<Status, FROZEN>>(&mut world);
        assert!(without.contains(&empty));
        assert!(!without.contains(&only_other));

        assert!(ids::<WithAnyFlags<Status, FROZEN_OR_SLOWED>>(&mut world).is_empty());
    }

    #[test]
    fn zero_mask_semantics() {
        let mut world = World::new();
        world.spawn(Status::FROZEN);
        world.spawn(Status::empty());
        world.spawn(Other);

        let present = sorted(
            world
                .query_filtered::<Entity, With<Status>>()
                .iter(&world)
                .collect(),
        );

        assert_eq!(ids::<WithFlags<Status, 0>>(&mut world), present);
        assert_eq!(ids::<WithoutFlags<Status, 0>>(&mut world), present);
        assert!(ids::<WithAnyFlags<Status, 0>>(&mut world).is_empty());
    }

    #[test]
    fn works_with_mutable_component_access() {
        fn thaw(mut query: Query<&mut Status, WithFlags<Status, FROZEN>>) {
            for mut status in &mut query {
                status.remove(Status::FROZEN);
            }
        }

        let mut world = World::new();
        let frozen = world.spawn(Status::FROZEN).id();
        let both = world.spawn(Status::FROZEN | Status::BURNING).id();
        let burning = world.spawn(Status::BURNING).id();

        world.run_system_once(thaw).unwrap();

        assert_eq!(*world.get::<Status>(frozen).unwrap(), Status::empty());
        assert_eq!(*world.get::<Status>(both).unwrap(), Status::BURNING);
        assert_eq!(*world.get::<Status>(burning).unwrap(), Status::BURNING);
    }

    #[test]
    fn filters_combine_with_each_other_and_with_other_filters() {
        let mut world = World::new();
        let frozen = world.spawn(Status::FROZEN).id();
        let slowed = world.spawn(Status::SLOWED).id();
        let frozen_and_other = world.spawn((Status::FROZEN, Other)).id();

        assert_eq!(
            ids::<Or<(WithFlags<Status, FROZEN>, WithFlags<Status, BURNING>)>>(&mut world),
            sorted(vec![frozen, frozen_and_other]),
        );
        assert_eq!(
            ids::<(WithFlags<Status, FROZEN>, Without<Other>)>(&mut world),
            sorted(vec![frozen]),
        );
        assert_eq!(
            ids::<(WithoutFlags<Status, FROZEN>, WithAnyFlags<Status, SLOWED>)>(&mut world),
            sorted(vec![slowed]),
        );
    }

    #[test]
    fn sparse_set_storage() {
        assert_eq!(SparseStatus::STORAGE_TYPE, StorageType::SparseSet);

        let mut world = World::new();
        let a = world.spawn(SparseStatus::A).id();
        let b = world.spawn(SparseStatus::B).id();
        let both = world.spawn(SparseStatus::A | SparseStatus::B).id();

        assert_eq!(
            ids::<WithAnyFlags<SparseStatus, SPARSE_A>>(&mut world),
            sorted(vec![a, both]),
        );
        assert_eq!(
            ids::<WithFlags<SparseStatus, SPARSE_A_OR_B>>(&mut world),
            sorted(vec![both]),
        );
        assert_eq!(
            ids::<WithoutFlags<SparseStatus, SPARSE_A>>(&mut world),
            sorted(vec![b]),
        );
    }

    #[test]
    fn unregistered_component_matches_nothing() {
        let mut world = World::new();
        world.spawn(Other);

        assert!(ids::<WithFlags<Ghost, GHOST_X>>(&mut world).is_empty());
    }

    #[test]
    fn archetype_changes_are_seen_by_new_queries() {
        let mut world = World::new();
        let entity = world.spawn(Status::FROZEN).id();
        assert_eq!(ids::<WithFlags<Status, FROZEN>>(&mut world), vec![entity]);

        world.entity_mut(entity).remove::<Status>();
        assert!(ids::<WithFlags<Status, FROZEN>>(&mut world).is_empty());

        world.entity_mut(entity).insert(Status::FROZEN);
        world.entity_mut(entity).insert(Other);
        assert_eq!(ids::<WithFlags<Status, FROZEN>>(&mut world), vec![entity]);
    }

    #[test]
    #[should_panic(expected = "does not fit in the `Bits` type")]
    fn mask_wider_than_storage_panics_at_query_initialization() {
        let mut world = World::new();
        let _ = world.query_filtered::<Entity, WithFlags<SparseStatus, { 1 << 8 }>>();
    }
}
