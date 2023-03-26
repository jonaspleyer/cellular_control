pub mod custom_cell_nd;

/// This module contains a ModularCell which is used to define cell types from individual cellular
/// properties such as mechanics, reactions, etc.
///
/// The [ModularCell](modular_cell::ModularCell) is a struct with fields that implement the various
/// [concepts](crate::concepts). The concepts are afterwards derived automatically for the
/// [ModularCell] struct.
pub mod modular_cell;
pub mod standard_cell_2d;
