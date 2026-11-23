//! Row-level bitflags filters for Bevy ECS.
//!
//! A `bitflags` flags type can derive `Component` directly:
//!
//! ```
//! use bevy_ecs::prelude::*;
//!
//! bitflags::bitflags! {
//!     #[derive(Component, Clone, Copy)]
//!     pub struct Status: u32 {
//!         const FROZEN = 1 << 0;
//!         const SLOWED = 1 << 1;
//!     }
//! }
//! ```
//!
//! This crate adds filters for those flags. They are per-row filters, not
//! archetype filters: changing a bit does not move the entity, and the
//! filter cannot be used directly in contiguous iteration.

use core::marker::PhantomData;

use bevy_ecs::{
    archetype::Archetype,
    change_detection::Tick,
    component::{Component, ComponentId, Components, StorageType},
    entity::Entity,
    query::{FilteredAccess, FilteredAccessSet, QueryData, QueryFilter, WorldQuery},
    storage::{Table, TableRow},
    world::{unsafe_world_cell::UnsafeWorldCell, World},
};
use bitflags::Flags;

/// Matches rows where all bits in `MASK` are set.
pub struct WithFlags<C, const MASK: u64>(PhantomData<C>)
where
    C: Component + Flags,
    C::Bits: Into<u64>;

/// Matches rows where at least one bit in `MASK` is set.
pub struct WithAnyFlags<C, const MASK: u64>(PhantomData<C>)
where
    C: Component + Flags,
    C::Bits: Into<u64>;

/// Matches rows where no bit in `MASK` is set.
pub struct WithoutFlags<C, const MASK: u64>(PhantomData<C>)
where
    C: Component + Flags,
    C::Bits: Into<u64>;

macro_rules! impl_flag_filter {
    ($name:ident, $matches:expr) => {
        // SAFETY: The filter only reads `C`; access is registered in
        // `update_component_access` and no mutable access is requested.
        unsafe impl<C, const MASK: u64> WorldQuery for $name<C, MASK>
        where
            C: Component + Flags,
            C::Bits: Into<u64>,
        {
            type Fetch<'w> = <&'w C as WorldQuery>::Fetch<'w>;
            type State = ComponentId;

            fn shrink_fetch<'wlong: 'wshort, 'wshort>(
                fetch: Self::Fetch<'wlong>,
            ) -> Self::Fetch<'wshort> {
                <&C as WorldQuery>::shrink_fetch(fetch)
            }

            unsafe fn init_fetch<'w, 's>(
                world: UnsafeWorldCell<'w>,
                state: &'s Self::State,
                last_run: Tick,
                this_run: Tick,
            ) -> Self::Fetch<'w> {
                <&C as WorldQuery>::init_fetch(world, state, last_run, this_run)
            }

            const IS_DENSE: bool = matches!(C::STORAGE_TYPE, StorageType::Table);

            unsafe fn set_archetype<'w, 's>(
                fetch: &mut Self::Fetch<'w>,
                state: &'s Self::State,
                archetype: &'w Archetype,
                table: &'w Table,
            ) {
                <&C as WorldQuery>::set_archetype(fetch, state, archetype, table);
            }

            unsafe fn set_table<'w, 's>(
                fetch: &mut Self::Fetch<'w>,
                state: &'s Self::State,
                table: &'w Table,
            ) {
                <&C as WorldQuery>::set_table(fetch, state, table);
            }

            fn update_component_access(state: &Self::State, access: &mut FilteredAccess) {
                access.add_read(*state);
            }

            fn init_nested_access(
                _state: &Self::State,
                _system_name: Option<&str>,
                _component_access_set: &mut FilteredAccessSet,
                _world: UnsafeWorldCell,
            ) {
            }

            fn init_state(world: &mut World) -> Self::State {
                world.register_component::<C>()
            }

            fn get_state(components: &Components) -> Option<Self::State> {
                components.component_id::<C>()
            }

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

            unsafe fn filter_fetch(
                state: &Self::State,
                fetch: &mut Self::Fetch<'_>,
                entity: Entity,
                table_row: TableRow,
            ) -> bool {
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

    bitflags::bitflags! {
        #[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
        pub struct Status: u32 {
            const FROZEN = 1 << 0;
            const SLOWED = 1 << 1;
            const BURNING = 1 << 2;
        }
    }

    const FROZEN: u64 = Status::FROZEN.bits() as u64;
    const SLOWED: u64 = Status::SLOWED.bits() as u64;
    const BURNING: u64 = Status::BURNING.bits() as u64;

    #[test]
    fn component_can_be_derived_inside_bitflags() {
        let mut world = World::new();
        let entity = world.spawn(Status::FROZEN).id();
        assert_eq!(*world.get::<Status>(entity).unwrap(), Status::FROZEN);
    }

    #[test]
    fn all_any_without_filters() {
        let mut world = World::new();
        let frozen = world.spawn(Status::FROZEN).id();
        let slowed = world.spawn(Status::SLOWED).id();
        let both = world.spawn(Status::FROZEN | Status::BURNING).id();
        let none = world.spawn(Status::empty()).id();

        assert_eq!(
            ids::<WithFlags<Status, FROZEN>>(&mut world),
            sorted(vec![frozen, both]),
        );
        assert_eq!(
            ids::<WithAnyFlags<Status, { FROZEN | SLOWED }>>(&mut world),
            sorted(vec![frozen, slowed, both]),
        );
        assert_eq!(
            ids::<WithoutFlags<Status, FROZEN>>(&mut world),
            sorted(vec![slowed, none]),
        );
        assert_eq!(
            ids::<WithFlags<Status, { FROZEN | BURNING }>>(&mut world),
            sorted(vec![both]),
        );
    }

    fn ids<F: QueryFilter>(world: &mut World) -> Vec<Entity> {
        let mut query = world.query_filtered::<Entity, F>();
        let ids: Vec<Entity> = query.iter(world).collect();
        sorted(ids)
    }

    fn sorted(mut ids: Vec<Entity>) -> Vec<Entity> {
        ids.sort();
        ids
    }
}
