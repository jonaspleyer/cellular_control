use std::collections::HashMap;

use crate::errors::{BoundaryError, DecomposeError};

/// Provides an abstraction of the physical total simulation domain.
///
/// [cellular_raza](https://github.com/jonaspleyer/cellular_raza) uses domain-decomposition
/// algorithms to split up the computational workload over multiple physical regions.
/// That's why the domain itself is mostly responsible for being deconstructed
/// into smaller [SubDomains](SubDomain) which can then be used to numerically solve our system.
///
/// This trait can be automatically implemented when the [SortCells], [DomainRngSeed],
/// and [DomainCreateSubDomains] are satisfied together with a small number of trait bounds to hash
/// and compare indices.
pub trait Domain<C, S, Ci = Vec<C>> {
    /// Subdomains can be identified by their unique [SubDomainIndex](Domain::SubDomainIndex).
    /// The backend uses this property to construct a mapping (graph) between subdomains.
    type SubDomainIndex;

    /// Similarly to the [SubDomainIndex](Domain::SubDomainIndex), voxels can be accessed by
    /// their unique index. The backend will use this information to construct a mapping
    /// (graph) between voxels inside their respective subdomains.
    type VoxelIndex;

    /// Retrieves all indices of subdomains.
    fn get_all_voxel_indices(&self) -> Vec<Self::VoxelIndex>;

    /// Deconstructs the [Domain] into its respective subdomains.
    ///
    /// When using the blanket implementation of this function, the following steps are carried
    /// out:
    /// Its functionality consists of the following steps:
    /// 1. Decompose the Domain into [Subdomains](SubDomain)
    /// 2. Build a neighbor map between [SubDomains](SubDomain)
    /// 3. Sort cells to their respective [SubDomain]
    /// However, to increase performance or avoid trait bounds, one can also opt to implement this
    /// trait directly.
    fn decompose(
        self,
        n_subdomains: core::num::NonZeroUsize,
        cells: Ci,
    ) -> Result<DecomposedDomain<Self::SubDomainIndex, S, C>, DecomposeError>;
}

/// Manage the current rng seed of a [Domain]
pub trait DomainRngSeed {
    // fn set_rng_seed(&mut self, seed: u64);

    /// Obtains the current rng seed
    fn get_rng_seed(&self) -> u64;
}

/// Generate [SubDomains](SubDomain) from an existing [Domain]
pub trait DomainCreateSubDomains<S> {
    /// This should always be identical to [Domain::SubDomainIndex].
    type SubDomainIndex;
    /// This should always be identical to [Domain::VoxelIndex].
    type VoxelIndex;

    /// Generates at most `n_subdomains`. This function can also return a lower amount of
    /// subdomains but never less than 1.
    fn create_subdomains(
        &self,
        n_subdomains: core::num::NonZeroUsize,
    ) -> Result<Vec<(Self::SubDomainIndex, S, Vec<Self::VoxelIndex>)>, DecomposeError>;
}

impl<C, S, T> Domain<C, S> for T
where
    T: DomainRngSeed
        + DomainCreateSubDomains<S>
        + SortCells<C, Index = <T as DomainCreateSubDomains<S>>::SubDomainIndex>,
    S: SubDomain<VoxelIndex = T::VoxelIndex>,
    T::SubDomainIndex: Clone + core::hash::Hash + Eq,
    T::VoxelIndex: Clone + core::hash::Hash + Eq,
{
    type SubDomainIndex = T::SubDomainIndex;
    type VoxelIndex = T::VoxelIndex;

    fn get_all_voxel_indices(&self) -> Vec<Self::VoxelIndex> {
        todo!()
    }

    fn decompose(
        self,
        n_subdomains: core::num::NonZeroUsize,
        cells: Vec<C>,
    ) -> Result<DecomposedDomain<Self::SubDomainIndex, S, C>, DecomposeError> {
        // Get all subdomains
        let subdomains = self.create_subdomains(n_subdomains)?;

        // Build a map from a voxel_index to the subdomain_index in which it is
        let voxel_index_to_subdomain_index: std::collections::HashMap<
            Self::VoxelIndex,
            Self::SubDomainIndex,
        > = subdomains
            .iter()
            .map(|(subdomain_index, _, voxel_indices)| {
                voxel_indices
                    .into_iter()
                    .map(|voxel_index| (voxel_index.clone(), subdomain_index.clone()))
            })
            .flatten()
            .collect();

        // Build neighbor map
        let mut neighbor_map: HashMap<Self::SubDomainIndex, Vec<Self::SubDomainIndex>> =
            HashMap::new();
        for (subdomain_index, subdomain, voxel_indices) in subdomains.iter() {
            for voxel_index in voxel_indices.iter() {
                for neighbor_voxel_index in subdomain.get_neighbor_voxel_indices(voxel_index) {
                    let neighbor_subdomain = voxel_index_to_subdomain_index
                        .get(&neighbor_voxel_index)
                        .ok_or(DecomposeError::IndexError(crate::IndexError(format!(
                            "TODO"
                        ))))?;
                    let neighbors =
                        neighbor_map
                            .get_mut(subdomain_index)
                            .ok_or(DecomposeError::IndexError(crate::IndexError(format!(
                                "TODO"
                            ))))?;
                    if neighbors.contains(neighbor_subdomain) {
                        neighbors.push(neighbor_subdomain.clone());
                    }
                }
            }
        }

        // Sort cells into the subdomains
        let mut index_to_cells = HashMap::<_, Vec<C>>::new();
        for cell in cells {
            let index = self.get_index_of(&cell)?;
            let existing_cells = index_to_cells.get_mut(&index);
            match existing_cells {
                Some(cell_vec) => cell_vec.push(cell),
                None => {
                    index_to_cells.insert(index, vec![cell]);
                }
            }
        }
        let index_subdomain_cells = subdomains
            .into_iter()
            .map(|(subdomain_index, subdomain, _)| {
                let cells = index_to_cells
                    .remove(&subdomain_index)
                    .unwrap_or(Vec::new());
                (subdomain_index, subdomain, cells)
            })
            .collect();

        Ok(DecomposedDomain {
            n_subdomains,
            index_subdomain_cells,
            neighbor_map,
            rng_seed: self.get_rng_seed(),
        })
    }
}

/// Generated by the [decompose](Domain::decompose) method. The backend will know how to
/// deal with this type and crate a working simulation from it.
pub struct DecomposedDomain<I, S, C> {
    /// Number of spawned [SubDomains](SubDomain). This number is guaranteed to be
    /// smaller or equal to the number may be different to the one given to the
    /// [Domain::decompose] method.
    /// Such behaviour can result from not being able to construct as many subdomains as desired.
    /// Note that this function will attempt to construct more [SubDomains](SubDomain)
    /// than available CPUs if given a larger number.
    pub n_subdomains: core::num::NonZeroUsize,
    /// Vector containing properties of individual [SubDomains](SubDomain).
    /// Entries are [Domain::SubDomainIndex], [SubDomain], and a vector of cells.
    pub index_subdomain_cells: Vec<(I, S, Vec<C>)>,
    /// Encapsulates how the subdomains are linked to each other.
    /// Eg. two subdomains without any boundary will never appear in each others collection
    /// of neighbors.
    /// For the future, we might opt to change to an undirected graph rather than a hashmap.
    pub neighbor_map: HashMap<I, Vec<I>>,
    /// Initial seed of the simulation for random number generation.
    pub rng_seed: u64,
}

/// Subdomains are produced by decomposing a [Domain] into multiple physical regions.
///
/// # Derivation
/// ```
/// # use cellular_raza_concepts::domain_new::*;
/// struct MySubDomain {
///     x_min: f32,
///     x_max: f32,
///     n: usize,
/// }
///
/// impl SubDomain for MySubDomain {
///     type VoxelIndex = usize;
///
///     fn get_neighbor_voxel_indices(
///         &self,
///         voxel_index: &Self::VoxelIndex
///     ) -> Vec<Self::VoxelIndex> {
///         (voxel_index.saturating_sub(1)..voxel_index.saturating_add(1).min(self.n)+1)
///             .filter(|k| k!=voxel_index)
///             .collect()
///     }
///
///     fn get_all_indices(&self) -> Vec<Self::VoxelIndex> {
///         (0..self.n).collect()
///     }
/// }
///
/// #[derive(SubDomain)]
/// struct MyNewSubDomain {
///     #[Base]
///     base: MySubDomain,
/// }
/// # let _my_sdm = MyNewSubDomain {
/// #     base: MySubDomain {
/// #         x_min: -20.0,
/// #         x_max: -11.0,
/// #         n: 20,
/// #     }
/// # };
/// # assert_eq!(_my_sdm.get_all_indices(), (0..20).collect::<Vec<_>>());
/// # assert_eq!(_my_sdm.get_neighbor_voxel_indices(&0), vec![1]);
/// # assert_eq!(_my_sdm.get_neighbor_voxel_indices(&3), vec![2,4]);
/// # assert_eq!(_my_sdm.get_neighbor_voxel_indices(&7), vec![6,8]);
/// ```
pub trait SubDomain {
    /// Individual Voxels inside each subdomain can be accessed by this index.
    type VoxelIndex;

    /// Obtains the neighbor voxels of the specified voxel index. This function behaves similarly
    /// to [SubDomainSortCells::get_voxel_index_of] in that it also has to return
    /// indices which are in other [SubDomains](SubDomain).
    fn get_neighbor_voxel_indices(&self, voxel_index: &Self::VoxelIndex) -> Vec<Self::VoxelIndex>;

    // fn apply_boundary(&self, cell: &mut C) -> Result<(), BoundaryError>;

    /// Get all voxel indices of this [SubDomain].
    fn get_all_indices(&self) -> Vec<Self::VoxelIndex>;
}

/// Assign an [Index](SortCells::Index) to a given cell.
///
/// This trait is used by the [Domain] and [SubDomain] trait to assign a [Domain::SubDomainIndex]
/// and [SubDomain::VoxelIndex] respectively.
///
/// # [SubDomain]
/// This trait is supposed to return the correct voxel index of the cell
/// even if this index is inside another [SubDomain].
/// This restriction might be lifted in the future but is still
/// required now.
pub trait SortCells<C> {
    /// An index which determines to which next smaller unit the cell should be assigned.
    type Index;

    /// If given a cell, we can sort this cell into the corresponding sub unit.
    fn get_index_of(&self, cell: &C) -> Result<Self::Index, BoundaryError>;
}

/// Apply boundary conditions to a cells position and velocity.
///
/// # Derivation
/// ```
/// # use cellular_raza_concepts::domain_new::*;
/// # use cellular_raza_concepts::BoundaryError;
/// struct MyMechanics {
///     x_min: f64,
///     x_max: f64,
/// }
///
/// impl SubDomainMechanics<f64, f64> for MyMechanics {
///     fn apply_boundary(&self, pos: &mut f64, vel: &mut f64) -> Result<(), BoundaryError> {
///         if *pos < self.x_min {
///             *vel = vel.abs();
///         }
///         if *pos > self.x_max {
///             *vel = -vel.abs();
///         }
///         *pos = pos.clamp(self.x_min, self.x_max);
///         Ok(())
///     }
/// }
///
/// #[derive(SubDomain)]
/// struct MySubDomain {
///     #[Mechanics]
///     mechanics: MyMechanics,
/// }
/// # let _my_sdm = MySubDomain {
/// #     mechanics: MyMechanics {
/// #         x_min: 1.0,
/// #         x_max: 33.0,
/// #     }
/// # };
/// # let mut pos = 0.0;
/// # let mut vel = - 0.1;
/// # _my_sdm.apply_boundary(&mut pos, &mut vel).unwrap();
/// # assert_eq!(pos, 1.0);
/// # assert_eq!(vel, 0.1);
/// ```
pub trait SubDomainMechanics<Pos, Vel> {
    /// If the subdomain has boundary conditions, this function will enforce them onto the cells.
    /// For the future, we plan to replace this function to additionally obtain information
    /// about the previous and current location of the cell.
    fn apply_boundary(&self, pos: &mut Pos, vel: &mut Vel) -> Result<(), BoundaryError>;
}

/// Apply a force on a cell depending on its position and velocity.
///
/// # Derivation
/// ```
/// # use cellular_raza_concepts::domain_new::*;
/// # use cellular_raza_concepts::CalcError;
/// struct MyForce {
///     damping: f64,
/// }
///
/// impl SubDomainForce<f64, f64, f64> for MyForce {
///     fn calculate_custom_force(&self, pos: &f64, vel: &f64) -> Result<f64, CalcError> {
///         Ok(- self.damping * vel)
///     }
/// }
///
/// #[derive(SubDomain)]
/// struct MySubDomain {
///     #[Force]
///     force: MyForce,
/// }
/// # let _my_sdm = MySubDomain {
/// #     force: MyForce {
/// #         damping: 0.1,
/// #     }
/// # };
/// # let calculated_force = _my_sdm.calculate_custom_force(&0.0, &1.0).unwrap();
/// # assert_eq!(calculated_force, -0.1);
/// ```
pub trait SubDomainForce<Pos, Vel, For> {
    ///
    fn calculate_custom_force(&self, pos: &Pos, vel: &Vel) -> Result<For, crate::CalcError>;
}

/// Describes extracellular reactions and fluid dynamics
///
/// # Derivation
/// ```compile_fail
/// # use cellular_raza_concepts::domain_new::*;
/// struct MyReactions;
///
/// impl SubDomainReactions for MyReactions {}
///
/// #[derive(SubDomain)]
/// struct DerivedSubDomain {
///     #[Reactions]
///     reactions: MyReactions,
/// }
/// ```
pub trait SubDomainReactions {}

/// This trait derives the different aspects of a [SubDomain].
///
/// It serves similarly as the [cellular_raza_concepts_derive::CellAgent] trait to quickly
/// build new structures from already existing functionality.
///
/// | Attribute | Trait | Implemented |
/// | ---  | --- |:---:|
/// | `Base` | [SubDomain] | ✅ |
/// | `SortCells` | [SubDomainSortCells] | ✅ |
/// | `Mechanics` | [SubDomainMechanics] | ✅ |
/// | `Force` | [SubDomainForce] | ✅  |
/// | `Reactions` | [SubDomainReactions] | ❌ |
///
/// # Example Usage
/// ```
/// # use cellular_raza_concepts::domain_new::*;
/// # struct MySubDomain;
/// # impl SubDomain for MySubDomain {
/// #     type VoxelIndex = usize;
/// #     fn get_neighbor_voxel_indices(&self, voxel_index: &Self::VoxelIndex) -> Vec<usize> {
/// #         Vec::new()
/// #     }
/// #     fn get_all_indices(&self) -> Vec<Self::VoxelIndex> {
/// #         Vec::new()
/// #     }
/// # }
/// #[derive(SubDomain)]
/// struct MyDerivedSubDomain {
///     #[Base]
///     s: MySubDomain,
/// }
/// # let derived_subdomain = MyDerivedSubDomain {
/// #     s: MySubDomain,
/// # };
/// # let all_indices = derived_subdomain.get_all_indices();
/// # assert_eq!(all_indices.len(), 0);
/// ```
#[doc(inline)]
pub use cellular_raza_concepts_derive::SubDomain;

// TODO
#[doc(inline)]
pub use cellular_raza_concepts_derive::Domain;
