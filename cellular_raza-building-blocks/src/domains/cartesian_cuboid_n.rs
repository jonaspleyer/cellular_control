use cellular_raza_concepts::domain_new::SubDomainMechanics;
// Imports from this crate
use cellular_raza_concepts::*;

#[cfg(feature = "pyo3")]
use pyo3::prelude::*;

// Imports from std and core
use core::cmp::{max, min};
use std::usize;

// Imports from other crates
use itertools::Itertools;
use nalgebra::SVector;

use serde::{Deserialize, Serialize};

use plotters::backend::BitMapBackend;
use plotters::coord::cartesian::Cartesian2d;
use plotters::coord::types::RangedCoordf64;
use plotters::prelude::DrawingArea;

/// Helper function to calculate the decomposition of a large number N into n as evenly-sizedchunks
/// chunks as possible
/// Examples:
/// N   n   decomp
/// 10  3    1 *  4  +  3 *  3
/// 13  4    1 *  5  +  3 *  4
/// 100 13   4 * 13  +  4 * 12
/// 225 16   1 * 15  + 15 * 14
/// 225 17   4 * 14  + 13 * 13
pub(super) fn get_decomp_res(n_voxel: usize, n_regions: usize) -> Option<(usize, usize, usize)> {
    // We calculate how many times we need to drain how many voxels
    // Example:
    //      n_voxels    = 59
    //      n_regions   = 6
    //      average_len = (59 / 8).ceil() = (9.833 ...).ceil() = 10
    //
    // try to solve this equation:
    //      n_voxels = average_len * n + (average_len-1) * m
    //      where n,m are whole positive numbers
    //
    // We start with    n = n_regions = 6
    // and with         m = min(0, n_voxel - average_len.pow(2)) = min(0, 59 - 6^2) = 23
    let mut average_len: i64 = (n_voxel as f64 / n_regions as f64).ceil() as i64;

    let residue = |n: i64, m: i64, avg: i64| n_voxel as i64 - avg * n - (avg - 1) * m;

    let mut n = n_regions as i64;
    let mut m = 0;

    for _ in 0..n_regions {
        let r = residue(n, m, average_len);
        if r == 0 {
            return Some((n as usize, m as usize, average_len as usize));
        } else if r > 0 {
            if n == n_regions as i64 {
                // Start from the beginning again but with different value for average length
                average_len += 1;
                n = n_regions as i64;
                m = 0;
            } else {
                n += 1;
                m -= 1;
            }
        // Residue is negative. This means we have subtracted too much and we just decrease n and
        // increase m
        } else {
            n -= 1;
            m += 1;
        }
    }
    None
}

/// A generic Domain with a cuboid layout.
///
/// This struct can be used to define custom domains on top of its behaviour.
#[derive(Clone, Debug)]
pub struct CartesianCuboid<F, const D: usize> {
    min: SVector<F, D>,
    max: SVector<F, D>,
    dx: SVector<F, D>,
    n_voxels: SVector<usize, D>,
    /// Seed from which all random numbers will be initially drawn
    pub rng_seed: u64,
}

impl<F, const D: usize> CartesianCuboid<F, D>
where
    F: Clone,
{
    /// Get the minimum point which defines the simulation domain
    pub fn get_min(&self) -> SVector<F, D> {
        self.min.clone()
    }

    /// Get the maximum point which defines the simulation domain
    pub fn get_max(&self) -> SVector<F, D> {
        self.max.clone()
    }

    /// Get the discretization used to generate voxels
    pub fn get_dx(&self) -> SVector<F, D> {
        self.dx.clone()
    }

    /// Get the number of voxels in each dimension of the domain
    pub fn get_n_voxels(&self) -> SVector<usize, D> {
        self.n_voxels.clone()
    }
}

impl<F, const D: usize> CartesianCuboid<F, D>
where
    F: 'static + num::Float + Copy + core::fmt::Debug + num::FromPrimitive + num::ToPrimitive,
{
    fn check_min_max(min: &[F; D], max: &[F; D]) -> Result<(), BoundaryError>
    where
        F: core::fmt::Debug,
    {
        for i in 0..D {
            if min[i] >= max[i] {
                return Err(BoundaryError(format!(
                    "Min {:?} must be smaller than Max {:?} for domain boundaries!",
                    min, max
                )));
            }
        }
        Ok(())
    }

    /// Builds a new [CartesianCuboid] from given boundaries and maximum interaction ranges of the
    /// containing cells.
    ///
    /// ```
    /// # use cellular_raza_building_blocks::CartesianCuboid;
    /// let min = [2.0, 3.0, 1.0];
    /// let max = [10.0, 10.0, 20.0];
    /// let interaction_range = 2.0;
    /// let domain = CartesianCuboid::from_boundaries_and_interaction_range(
    ///     min,
    ///     max,
    ///     interaction_range
    /// )?;
    ///
    /// assert_eq!(domain.get_n_voxels()[0], 4);
    /// assert_eq!(domain.get_n_voxels()[1], 3);
    /// assert_eq!(domain.get_n_voxels()[2], 9);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn from_boundaries_and_interaction_range(
        min: impl Into<[F; D]>,
        max: impl Into<[F; D]>,
        interaction_range: F,
    ) -> Result<Self, BoundaryError> {
        // Perform conversions
        let min: [F; D] = min.into();
        let max: [F; D] = max.into();

        // Check that the specified min and max are actually smaller / larger
        Self::check_min_max(&min, &max)?;

        // Calculate the number of voxels from given interaction ranges
        let mut n_voxels = [0; D];
        let mut dx = [F::zero(); D];
        for i in 0..D {
            let n = ((max[i] - min[i]) / interaction_range).floor();
            // This conversion should hopefully never fail.
            n_voxels[i] = n.to_usize().ok_or(BoundaryError(
                cellular_raza_concepts::format_error_message!(
                    format!(
                        "Cannot convert float {:?} of type {} to usize",
                        n,
                        std::any::type_name::<F>()
                    ),
                    "conversion error during domain setup"
                ),
            ))?;
            dx[i] = (max[i] - min[i]) / n;
        }

        Ok(Self {
            min: min.into(),
            max: max.into(),
            dx: dx.into(),
            n_voxels: n_voxels.into(),
            rng_seed: 0,
        })
    }

    /// Builds a new [CartesianCuboid] from given boundaries and the number of voxels per dimension
    /// specified.
    pub fn from_boundaries_and_n_voxels(
        min: impl Into<[F; D]>,
        max: impl Into<[F; D]>,
        n_voxels: impl Into<[usize; D]>,
    ) -> Result<Self, BoundaryError> {
        let min: [F; D] = min.into();
        let max: [F; D] = max.into();
        let n_voxels: [usize; D] = n_voxels.into();
        Self::check_min_max(&min, &max)?;
        let mut dx: SVector<F, D> = [F::zero(); D].into();
        for i in 0..D {
            let n = F::from_usize(n_voxels[i]).ok_or(BoundaryError(
                cellular_raza_concepts::format_error_message!(
                    "conversion error during domain setup",
                    format!(
                        "Cannot convert usize {} to float of type {}",
                        n_voxels[i],
                        std::any::type_name::<F>()
                    )
                ),
            ))?;
            dx[i] = (max[i] - min[i]) / n;
        }
        Ok(Self {
            min: min.into(),
            max: max.into(),
            dx,
            n_voxels: n_voxels.into(),
            rng_seed: 0,
        })
    }
}

impl<F, const D: usize> CartesianCuboid<F, D> {
    fn get_all_voxel_indices(&self) -> impl IntoIterator<Item = [usize; D]> {
        use itertools::*;
        (0..D)
            .map(|i| 0..self.n_voxels[i])
            .multi_cartesian_product()
            .map(|x| {
                let mut index = [0; D];
                for j in 0..D {
                    index[j] = x[j];
                }
                index
            })
    }

    /// Get the total amount of indices in this domain
    fn get_n_indices(&self) -> usize {
        let mut res = 1;
        for i in 0..D {
            res *= self.n_voxels[i];
        }
        res
    }
}

mod test_domain_setup {
    #[test]
    fn from_boundaries_and_interaction_range() {
        use crate::CartesianCuboid;
        let min = [0.0; 2];
        let max = [2.0; 2];
        let interaction_range = 1.0;
        let _ = CartesianCuboid::from_boundaries_and_interaction_range(min, max, interaction_range)
            .unwrap();
        // TODO add actual test case here
    }

    #[test]
    fn from_boundaries_and_n_voxels() {
        use crate::CartesianCuboid;
        let min = [-100.0f32; 55];
        let max = [43000.0f32; 55];
        let n_voxels = [22; 55];
        let _ = CartesianCuboid::from_boundaries_and_n_voxels(min, max, n_voxels).unwrap();
        // TODO add actual test case here
    }
}

impl<F, const D: usize> CartesianCuboid<F, D>
where
    F: 'static
        + num::Float
        + Copy
        + core::fmt::Debug
        + num::FromPrimitive
        + num::ToPrimitive
        + core::ops::SubAssign
        + nalgebra::ClosedDiv<F>,
{
    /// Obtains the voxel index given a regular vector
    ///
    /// This function can be used in derivatives of this type.
    pub fn get_voxel_index_of_raw(&self, pos: &SVector<F, D>) -> Result<[usize; D], BoundaryError> {
        Self::check_min_max(&self.min.into(), &(*pos).into())?;
        let n_vox = (pos - self.min).component_div(&self.dx);
        let mut res = [0usize; D];
        for i in 0..D {
            res[i] = n_vox[i].to_usize().ok_or(BoundaryError(
                cellular_raza_concepts::format_error_message!(
                    "conversion error during domain setup",
                    format!(
                        "Cannot convert float {:?} of type {} to usize",
                        n_vox[i],
                        std::any::type_name::<F>()
                    )
                ),
            ))?;
        }
        Ok(res.into())
    }
}

impl<C, F, const D: usize> cellular_raza_concepts::domain_new::SortCells<C>
    for CartesianCuboid<F, D>
where
    F: 'static
        + num::Float
        + Copy
        + core::fmt::Debug
        + num::FromPrimitive
        + num::ToPrimitive
        + core::ops::SubAssign
        + nalgebra::ClosedDiv<F>,
    C: Mechanics<SVector<F, D>, SVector<F, D>, SVector<F, D>>,
{
    type VoxelIndex = [usize; D];

    fn get_voxel_index_of(&self, cell: &C) -> Result<Self::VoxelIndex, BoundaryError> {
        let pos = cell.pos();
        self.get_voxel_index_of_raw(&pos)
    }
}

impl<F, const D: usize> cellular_raza_concepts::domain_new::DomainRngSeed
    for CartesianCuboid<F, D>
{
    fn get_rng_seed(&self) -> u64 {
        self.rng_seed
    }
}

#[test]
fn generate_subdomains() {
    use cellular_raza_concepts::domain_new::DomainCreateSubDomains;
    let min = [0.0; 3];
    let max = [100.0; 3];
    let interaction_range = 20.0;
    let domain =
        CartesianCuboid::from_boundaries_and_interaction_range(min, max, interaction_range)
            .unwrap();
    let sub_domains = domain
        .create_subdomains(4.try_into().unwrap())
        .unwrap()
        .into_iter()
        .collect::<Vec<_>>();
    assert_eq!(sub_domains.len(), 4);
    assert_eq!(
        sub_domains
            .iter()
            .map(|(_, _, voxels)| voxels.len())
            .sum::<usize>(),
        5usize.pow(3)
    );
}

/// Subdomain corresponding to the [CartesianCuboid] struct.
#[derive(Clone, Debug)]
pub struct CartesianSubDomain<F, const D: usize> {
    min: SVector<F, D>,
    max: SVector<F, D>,
    dx: SVector<F, D>,
    voxels: Vec<[usize; D]>,
    domain_min: SVector<F, D>,
    domain_max: SVector<F, D>,
    domain_n_voxels: SVector<usize, D>,
}

impl<F, const D: usize> CartesianSubDomain<F, D>
where
    F: Clone,
{
    /// Get the minimum boundary of the subdomain.
    /// Note that not all voxels which could be in the space of the subdomain need to be in it.
    pub fn get_min(&self) -> SVector<F, D> {
        self.min.clone()
    }

    /// Get the maximum boundary of the subdomain.
    /// Note that not all voxels which could be in the space of the subdomain need to be in it.
    pub fn get_max(&self) -> SVector<F, D> {
        self.max.clone()
    }

    /// Get the discretization used to generate voxels
    pub fn get_dx(&self) -> SVector<F, D> {
        self.dx.clone()
    }

    /// Get all voxel indices which are currently in this subdomain
    pub fn get_voxels(&self) -> Vec<[usize; D]> {
        self.voxels.clone()
    }

    /// See [CartesianCuboid::get_min].
    pub fn get_domain_min(&self) -> SVector<F, D> {
        self.domain_min.clone()
    }

    /// See [CartesianCuboid::get_max].
    pub fn get_domain_max(&self) -> SVector<F, D> {
        self.domain_max.clone()
    }

    /// See [CartesianCuboid::get_n_voxels].
    pub fn get_domain_n_voxels(&self) -> SVector<usize, D> {
        self.domain_n_voxels.clone()
    }
}

impl<F, const D: usize> CartesianSubDomain<F, D> {
    /// Generic method to obtain the voxel index of any type that can be casted to an array.
    pub fn get_index_of<P>(&self, pos: P) -> Result<[usize; D], BoundaryError>
    where
        [F; D]: From<P>,
        F: 'static + num::Float + core::fmt::Debug + core::ops::SubAssign + core::ops::DivAssign,
    {
        let pos: [F; D] = pos.into();
        let mut res = [0usize; D];
        for i in 0..D {
            let n_vox = (pos[i] - self.min[i]) / self.dx[i];
            res[i] = n_vox.to_usize().ok_or(BoundaryError(
                cellular_raza_concepts::format_error_message!(
                    "conversion error during domain setup",
                    format!(
                        "Cannot convert float {:?} of type {} to usize",
                        n_vox,
                        std::any::type_name::<F>()
                    )
                ),
            ))?;
        }
        Ok(res)
    }
}

impl<F, const D: usize>
    cellular_raza_concepts::domain_new::DomainCreateSubDomains<CartesianSubDomain<F, D>>
    for CartesianCuboid<F, D>
where
    F: 'static + num::Float + core::fmt::Debug + num::FromPrimitive,
{
    type SubDomainIndex = usize;
    type VoxelIndex = [usize; D];

    fn create_subdomains(
        &self,
        n_subdomains: core::num::NonZeroUsize,
    ) -> Result<
        impl IntoIterator<
            Item = (
                Self::SubDomainIndex,
                CartesianSubDomain<F, D>,
                Vec<Self::VoxelIndex>,
            ),
        >,
        DecomposeError,
    > {
        let indices = self.get_all_voxel_indices();
        let n_indices = self.get_n_indices();

        let (n, _m, average_len) = get_decomp_res(n_indices, n_subdomains.into()).ok_or(
            DecomposeError::Generic("Could not find a suiting decomposition".to_owned()),
        )?;

        // TODO Currently we are not splitting the voxels apart efficiently
        // These are subdomains which contain n voxels
        let switcher = n * average_len;
        let indices_grouped = indices.into_iter().enumerate().group_by(|(i, _)| {
            use num::Integer;
            if *i < switcher {
                i.div_rem(&average_len).0
            } else {
                (i - switcher).div_rem(&(average_len - 1).max(1)).0 + n
            }
        });
        let mut res = Vec::new();
        for (n_subdomain, indices) in indices_grouped.into_iter() {
            let mut min_vox = [0; D];
            let mut max_vox = [0; D];
            let voxels = indices
                .into_iter()
                .map(|(_, index)| {
                    for i in 0..D {
                        min_vox[i] = min_vox[i].min(index[i]);
                        max_vox[i] = max_vox[i].max(index[i]);
                    }
                    index
                })
                .collect::<Vec<_>>();
            let mut min = [F::zero(); D];
            let mut max = [F::zero(); D];
            for i in 0..D {
                let n_vox_min = F::from_usize(min_vox[i]).ok_or(DecomposeError::Generic(
                    cellular_raza_concepts::format_error_message!(
                        "conversion error during domain setup",
                        format!(
                            "Cannot convert float {:?} of type {} to usize",
                            min_vox[i],
                            std::any::type_name::<F>()
                        )
                    ),
                ))?;
                let n_vox_max = F::from_usize(max_vox[i]).ok_or(DecomposeError::Generic(
                    cellular_raza_concepts::format_error_message!(
                        "conversion error during domain setup",
                        format!(
                            "Cannot convert float {:?} of type {} to usize",
                            max_vox[i],
                            std::any::type_name::<F>()
                        )
                    ),
                ))?;
                min[i] = self.min[i] + n_vox_min * self.dx[i];
                max[i] = self.min[i] + (n_vox_max + F::one()) * self.dx[i];
            }
            let subdomain = CartesianSubDomain {
                min: min.into(),
                max: max.into(),
                dx: self.dx.clone(),
                voxels: voxels.clone(),
                domain_min: self.min,
                domain_max: self.max,
                domain_n_voxels: self.n_voxels.clone(),
            };
            res.push((n_subdomain, subdomain, voxels));
        }
        Ok(res)
    }
}

impl<Coord, F, const D: usize> SubDomainMechanics<Coord, Coord> for CartesianSubDomain<F, D>
where
    for<'a> &'a mut Coord: Into<[F; D]>,
    Coord: From<[F; D]>,
    Coord: std::fmt::Debug,
    F: num::Float,
{
    fn apply_boundary(&self, pos: &mut Coord, vel: &mut Coord) -> Result<(), BoundaryError> {
        let mut velocity: [F; D] = vel.into();
        let mut position: [F; D] = pos.into();

        // Define constant two
        let two = F::one() + F::one();

        // For each dimension
        for i in 0..D {
            // Check if the particle is below lower edge
            if position[i] < self.min[i] {
                position[i] = two * self.min[i] - position[i];
                velocity[i] = velocity[i].abs();
            }
            // Check if the particle is over the edge
            if position[i] > self.max[i] {
                position[i] = two * self.max[i] - position[i];
                velocity[i] = -velocity[i].abs();
            }
        }

        // If new position is still out of boundary return error
        for i in 0..D {
            if position[i] < self.min[i] || position[i] > self.max[i] {
                return Err(BoundaryError(format!(
                    "Particle is out of domain at position {:?}",
                    pos
                )));
            }
        }

        // Set the position and velocity
        *pos = position.into();
        *vel = velocity.into();
        Ok(())
    }
}

impl<F, const D: usize> cellular_raza_concepts::domain_new::SubDomain for CartesianSubDomain<F, D> {
    type VoxelIndex = [usize; D];

    fn get_all_indices(&self) -> Vec<Self::VoxelIndex> {
        self.voxels.clone()
    }

    fn get_neighbor_voxel_indices(&self, voxel_index: &Self::VoxelIndex) -> Vec<Self::VoxelIndex> {
        // Create the bounds for the following creation of all the voxel indices
        let mut bounds = [[0; 2]; D];
        for i in 0..D {
            bounds[i] = [
                (voxel_index[i] as i64 - 1).max(0) as usize,
                (voxel_index[i] + 2).min(self.domain_n_voxels[i]),
            ];
        }

        // Create voxel indices
        (0..D)
            .map(|i| (bounds[i][0]..bounds[i][1]))
            .multi_cartesian_product()
            .map(|ind_v| {
                let mut res = [0; D];
                for i in 0..D {
                    res[i] = ind_v[i];
                }
                res
            })
            .filter(|ind| ind != voxel_index)
            .collect()
    }
}

macro_rules! define_and_implement_cartesian_cuboid {
    ($d: expr, $name: ident, $($k: expr),+) => {
        /// Cuboid Domain with regular cartesian coordinates in
        #[doc = concat!(" `", stringify!($d), "D`")]
        #[derive(Clone,Debug,Serialize,Deserialize)]
        #[cfg_attr(feature = "pyo3", pyclass)]
        #[cfg_attr(feature = "pyo3", pyo3(get_all, set_all))]
        pub struct $name {
            min: [f64; $d],
            max: [f64; $d],
            n_vox: [i64; $d],
            voxel_sizes: [f64; $d],
        }


        impl $name {
            fn check_min_max(min: [f64; $d], max: [f64; $d]) -> Result<(), CalcError> {
                for i in 0..$d {
                    match max[i] > min[i] {
                        false => Err(CalcError(format!(
                            "Min {:?} must be smaller than Max {:?} for domain boundaries!",
                            min,
                            max
                        ))),
                        true => Ok(()),
                    }?;
                }
                Ok(())
            }

            fn check_positive<F>(interaction_ranges: [F; $d]) -> Result<(), CalcError>
            where
                F: PartialOrd + num::Zero + core::fmt::Debug,
            {
                for i in 0..$d {
                    match interaction_ranges[i] > F::zero() {
                        false => Err(CalcError(format!(
                            "Interaction range must be positive and non-negative! Got value {:?}",
                            interaction_ranges[i]
                        ))),
                        true => Ok(())
                    }?;
                }
                Ok(())
            }

            // TODO write this nicely!
            #[doc = "Builds a new `"]
            #[doc = stringify!($name)]
            #[doc = "` from given boundaries and maximum interaction ranges of the containing
                cells."]
            pub fn from_boundaries_and_interaction_ranges(
                min: [f64; $d],
                max: [f64; $d],
                interaction_ranges: [f64; $d]
            ) -> Result<$name, CalcError> {
                $name::check_min_max(min, max)?;
                $name::check_positive(interaction_ranges)?;
                let mut n_vox = [0; $d];
                let mut voxel_sizes = [0.0; $d];
                for i in 0..$d {
                    n_vox[i] = ((max[i] - min[i]) / interaction_ranges[i] * 0.5).ceil() as i64;
                    voxel_sizes[i] = (max[i]-min[i])/n_vox[i] as f64;
                }
                Ok($name {
                    min,
                    max,
                    n_vox,
                    voxel_sizes,
                })
            }

            #[doc = "Builds a new `"]
            #[doc = stringify!($name)]
            #[doc = "` from given boundaries and the number of voxels per dimension specified."]
            pub fn from_boundaries_and_n_voxels(
                min: [f64; $d],
                max: [f64; $d],
                n_vox: [usize; $d]
            ) -> Result<$name, CalcError> {
                $name::check_min_max(min, max)?;
                $name::check_positive(n_vox)?;
                let mut voxel_sizes = [0.0; $d];
                for i in 0..$d {
                    voxel_sizes[i] = (max[i] - min[i]) / n_vox[i] as f64;
                }
                Ok($name {
                    min,
                    max,
                    n_vox: [$(n_vox[$k] as i64),+],
                    voxel_sizes,
                })
            }
        }
    }
}

macro_rules! implement_cartesian_cuboid_voxel_fluid_mechanics{
    ($d: literal, $name: ident, $voxel_name: ident, $($k: expr),+) => {
        // Define the struct for the voxel
        /// Cuboid Voxel for [
        #[doc = stringify!($name)]
        /// ] in
        #[doc = concat!(" `", stringify!($d), "D`")]
        #[derive(Clone,Debug,Serialize,Deserialize)]
        pub struct $voxel_name<const N: usize> {
                min: [f64; $d],
                max: [f64; $d],
                middle: [f64; $d],
                dx: [f64; $d],
                index: [i64; $d],

                /// Concentrations of the different diffusables
                pub extracellular_concentrations: SVector<f64, N>,
                #[cfg(feature = "gradients")]
                /// The gradient of diffusables at this voxel
                pub extracellular_gradient: SVector<SVector<f64, $d>, N>,
                /// Local diffusion constant
                pub diffusion_constant: SVector<f64, N>,
                /// Local production rate of diffusables
                pub production_rate: SVector<f64, N>,
                /// Local degradation rate of diffusables
                pub degradation_rate: SVector<f64, N>,
                domain_boundaries: Vec<([i64; $d], BoundaryCondition<SVector<f64, N>>)>,
        }

        impl<const N: usize> Volume for $voxel_name<N> {
            fn get_volume(&self) -> f64 {
                self.min.iter().zip(self.max.iter()).map(|(x, y)| y-x).product()
            }
        }

        impl<const N: usize> $voxel_name<N> {
            pub(crate) fn new(
                min: [f64; $d],
                max: [f64; $d],
                index: [i64; $d],
                domain_boundaries: Vec<([i64; $d], BoundaryCondition<SVector<f64, N>>)>
            ) -> $voxel_name<N> {
                let middle = [$((max[$k] + min[$k])/2.0),+];
                let dx = [$(max[$k]-min[$k]),+];
                $voxel_name::<N> {
                    min,
                    max,
                    middle,
                    dx,
                    index,
                    extracellular_concentrations: SVector::<f64, N>::from_element(0.0),
                    #[cfg(feature = "gradients")]
                    extracellular_gradient: SVector::<SVector<f64, $d>, N>::from_element(
                        SVector::<f64, $d>::from_element(0.0)
                    ),
                    diffusion_constant: SVector::<f64, N>::from_element(0.0),
                    production_rate: SVector::<f64, N>::from_element(0.0),
                    degradation_rate: SVector::<f64, N>::from_element(0.0),
                    domain_boundaries,
                }
            }

            /// Get lower boundary of voxel
            pub fn get_min(&self) -> [f64; $d] {self.min}
            /// Get upper boundary of voxel
            pub fn get_max(&self) -> [f64; $d] {self.max}
            /// Get middle of voxel
            pub fn get_middle(&self) -> [f64; $d] {self.middle}
            /// Get side lengths of voxel
            pub fn get_dx(&self) -> [f64; $d] {self.dx}

            fn position_is_in_domain(&self, pos: &SVector<f64, $d>) -> Result<(), RequestError> {
                match pos.iter().enumerate().any(|(i, p)| !(self.min[i] <= *p && *p <= self.max[i])) {
                    true => Err(RequestError(format!(
                        "point {:?} is not in requested voxel with boundaries {:?} {:?}",
                        pos,
                        self.min,
                        self.max
                    ))),
                    false => Ok(()),
                }
            }

            fn index_to_distance_squared(&self, index: &[i64; $d]) -> f64 {
                let mut diffs = [0; $d];
                for i in 0..$d {
                    diffs[i] = (index[i] as i32 - self.index[i] as i32).abs()
                }
                diffs
                    .iter()
                    .enumerate()
                    .map(|(i, d)| self.dx[i].powf(2.0)* (*d as f64))
                    .sum::<f64>()
            }
        }

        // Implement the Voxel trait for our n-dim voxel
        impl<const N: usize> Voxel<[i64; $d], SVector<f64, $d>, SVector<f64, $d>, SVector<f64, $d>>
            for $voxel_name<N> {
            fn get_index(&self) -> [i64; $d] {
                self.index
            }
        }

        impl<const N: usize> ExtracellularMechanics<
            [i64; $d],
            SVector<f64, $d>,
            SVector<f64, N>,
            SVector<SVector<f64, $d>, N>,
            SVector<f64, N>,
            SVector<f64, N>
        > for $voxel_name<N> {
            fn get_extracellular_at_point(
                &self,
                pos: &SVector<f64, $d>
            ) -> Result<SVector<f64, N>, RequestError> {
                self.position_is_in_domain(pos)?;
                Ok(self.extracellular_concentrations)
            }

            fn get_total_extracellular(&self) -> SVector<f64, N> {
                self.extracellular_concentrations
            }

            #[cfg(feature = "gradients")]
            fn update_extracellular_gradient(
                &mut self,
                boundaries: &[([i64; $d], BoundaryCondition<SVector<f64, N>>)]
            ) -> Result<(), CalcError> {
                let mut new_gradient = SVector::<SVector<f64, $d>, N>::from_element(
                    SVector::<f64, $d>::from_element(0.0)
                );
                boundaries.iter()
                    .for_each(|(index, boundary_condition)| {
                        let extracellular_difference = match boundary_condition {
                            BoundaryCondition::Neumann(value) => {*value},
                            BoundaryCondition::Dirichlet(value) => {
                                self.extracellular_concentrations-value
                            },
                            BoundaryCondition::Value(value) => {
                                self.extracellular_concentrations-value
                            },
                        };
                        let pointer = SVector::from(
                            [$(self.index[$k] as f64 - index[$k] as f64),+]
                        );
                        let dist = pointer.norm();
                        let gradient = pointer.normalize()/dist;
                        new_gradient
                            .iter_mut()
                            .zip(extracellular_difference.into_iter())
                            .for_each(|(component, diff)| *component += *diff*gradient);
                    });
                self.extracellular_gradient = new_gradient;
                Ok(())
            }

            #[cfg(feature = "gradients")]
            fn get_extracellular_gradient_at_point(
                &self,
                _pos: &SVector<f64, $d>
            ) -> Result<SVector<SVector<f64, $d>, N>, RequestError> {
                Ok(self.extracellular_gradient)
            }

            fn set_total_extracellular(
                &mut self,
                concentrations: &SVector<f64, N>
            ) -> Result<(), CalcError> {
                Ok(self.extracellular_concentrations = *concentrations)
            }

            fn calculate_increment(
                &self,
                total_extracellular: &SVector<f64, N>,
                point_sources: &[(SVector<f64, $d>, SVector<f64, N>)],
                boundaries: &[([i64; $d], BoundaryCondition<SVector<f64, N>>)]
            ) -> Result<SVector<f64, N>, CalcError> {
                let mut inc = SVector::<f64, N>::from_element(0.0);

                self.domain_boundaries
                    .iter()
                    .for_each(|(index, boundary)| match boundary {
                        BoundaryCondition::Neumann(value) =>
                            inc += value / self.index_to_distance_squared(index).sqrt(),
                        BoundaryCondition::Dirichlet(value) =>
                            inc += (value-total_extracellular)
                                / self.index_to_distance_squared(index),
                        BoundaryCondition::Value(value) =>
                            inc += (value-total_extracellular)
                                / self.index_to_distance_squared(index),
                    });

                boundaries.iter()
                    .for_each(|(index, boundary)| match boundary {
                        BoundaryCondition::Neumann(value) =>
                            inc += value
                                / self.index_to_distance_squared(&index).sqrt(),
                        BoundaryCondition::Dirichlet(value) =>
                            inc += (value-total_extracellular)
                                / self.index_to_distance_squared(&index),
                        BoundaryCondition::Value(value) =>
                            inc += (value-total_extracellular)
                                / self.index_to_distance_squared(&index),
                    });
                inc = inc.component_mul(&self.diffusion_constant);

                point_sources.iter()
                    .for_each(|(_, value)| inc += value);

                // Also calculate internal reactions. Here it is very simple only given by
                // degradation and production.
                inc += self.production_rate
                    - self.degradation_rate.component_mul(&total_extracellular);
                Ok(inc)
            }

            fn boundary_condition_to_neighbor_voxel(
                &self,
                _neighbor_index: &[i64; $d]
            ) -> Result<BoundaryCondition<SVector<f64, N>>, IndexError> {
                Ok(BoundaryCondition::Value(self.extracellular_concentrations))
            }
        }

        // Implement the cartesian cuboid
        // Index is an array of size 3 with elements of type usize
        impl<Cel, const N: usize> Domain<Cel, [i64; $d], $voxel_name<N>> for $name
        // Position, Force and Velocity are all Vector$d supplied by the Nalgebra crate
            where Cel: cellular_raza_concepts::Mechanics<
                SVector<f64, $d>,
                SVector<f64, $d>,
                SVector<f64, $d>
            >,
        {
            fn apply_boundary(&self, cell: &mut Cel) -> Result<(),BoundaryError> {
                let mut pos = cell.pos();
                let mut velocity = cell.velocity();

                // For each dimension
                for i in 0..$d {
                    // Check if the particle is below lower edge
                    if pos[i] < self.min[i] {
                        pos[i] = 2.0 * self.min[i] - pos[i];
                        velocity[i] = velocity[i].abs();
                    }
                    // Check if the particle is over the edge
                    if pos[i] > self.max[i] {
                        pos[i] = 2.0 * self.max[i] - pos[i];
                        velocity[i] = - velocity[i].abs();
                    }
                }
                // Set new position and velocity of particle
                cell.set_pos(&pos);
                cell.set_velocity(&velocity);

                // If new position is still out of boundary return error
                for i in 0..$d {
                    if pos[i] < self.min[i] || pos[i] > self.max[i] {
                        return Err(BoundaryError(format!(
                            "Particle is out of domain at position {:?}",
                            pos
                        )));
                    }
                }
                Ok(())
            }

            fn get_voxel_index(&self, cell: &Cel) -> [i64; $d] {
                let p = cell.pos();
                let mut out = [0; $d];

                for i in 0..$d {
                    out[i] = ((p[i] - self.min[0]) / self.voxel_sizes[i]) as i64;
                    out[i] = out[i].min(self.n_vox[i]-1).max(0);
                }
                return out;
            }

            fn get_all_indices(&self) -> Vec<[i64; $d]> {
                [$($k),+].iter()
                    .map(|i| (0..self.n_vox[*i]))
                    .multi_cartesian_product()
                    .map(|ind_v| [$(ind_v[$k]),+])
                    .collect()
            }

            fn get_neighbor_voxel_indices(&self, index: &[i64; $d]) -> Vec<[i64; $d]> {
                // Create the bounds for the following creation of all the voxel indices
                let bounds: [[i64; 2]; $d] = [$(
                    [
                        max(index[$k] as i32 - 1, 0) as i64,
                        min(index[$k]+2, self.n_vox[$k])
                    ]
                ),+];

                // Create voxel indices
                let v: Vec<[i64; $d]> = [$($k),+].iter()      // indices supplied in macro invokation
                    .map(|i| (bounds[*i][0]..bounds[*i][1]))    // ranges from bounds
                    .multi_cartesian_product()                  // all possible combinations
                    .map(|ind_v| [$(ind_v[$k]),+])              // multi_cartesian_product gives us vector elements. We map them to arrays.
                    .filter(|ind| ind!=index)                   // filter the elements such that the current index is not included.
                    .collect();                                 // collect into the correct type

                return v;
            }

            fn generate_contiguous_multi_voxel_regions(&self, n_regions: usize) -> Result<Vec<Vec<([i64; $d], $voxel_name<N>)>>, CalcError> {
                // Get all voxel indices
                let indices: Vec<[i64; $d]> = [$($k),+]
                    .iter()                                     // indices supplied in macro invokation
                    .map(|i| (0..self.n_vox[*i]))               // ranges from self.n_vox
                    .multi_cartesian_product()                  // all possible combinations
                    .map(|ind_v| [$(ind_v[$k]),+])              // multi_cartesian_product gives us vector elements. We map them to arrays.
                    .collect();

                let (n, _m, average_len);
                match get_decomp_res(indices.len(), n_regions) {
                    Some(res) => (n, _m, average_len) = res,
                    None => return Err(CalcError("Could not find a suiting decomposition".to_owned())),
                };

                // Now we drain the indices vector
                let mut index_voxel_combinations: Vec<([i64; $d], $voxel_name<N>)> = indices
                    .into_iter()
                    .map(|ind| {
                        let min = [$(self.min[$k] +    ind[$k]  as f64*self.voxel_sizes[$k]),+];
                        let max = [$(self.min[$k] + (1+ind[$k]) as f64*self.voxel_sizes[$k]),+];
                        let domain_boundaries = (0..$d)
                            .map(|_| (-1_i64..2_i64))
                            .multi_cartesian_product()
                            .map(|v| [$(ind[$k] + v[$k]),+])
                            .filter(|new_index| *new_index != ind)
                            .filter(|new_index| new_index.iter().zip(self.n_vox.iter()).any(|(i1, i2)| *i1<0 || i2<=i1))
                            .map(|new_index| (new_index, BoundaryCondition::Neumann(SVector::<f64, N>::from_element(0.0))))
                            .collect::<Vec<_>>();
                        (ind, $voxel_name::<N>::new(min, max, ind, domain_boundaries))
                    })
                    .collect();

                // TODO optimize this!
                // Currently we are not splitting the voxels apart efficiently
                let mut ind_n: Vec<Vec<_>> = index_voxel_combinations
                    .drain(0..(average_len*n) as usize)
                    .into_iter()
                    .chunks(average_len as usize)
                    .into_iter()
                    .map(|chunk| chunk.collect::<Vec<_>>())
                    .collect();

                let mut ind_m: Vec<Vec<_>> = index_voxel_combinations
                    .drain(..)
                    .into_iter()
                    .chunks((max(average_len-1, 1)) as usize)
                    .into_iter()
                    .map(|chunk| chunk.collect::<Vec<_>>())
                    .collect();

                ind_n.append(&mut ind_m);

                Ok(ind_n)
            }
        }
    }
}

macro_rules! implement_cartesian_cuboid_domain_new {
    (
        $d: literal,
        $domain_name: ident,
        $subdomain_name: ident,
        $voxel_name: ident,
        $float_type: ty,
        $($k: expr),+
    ) => {
        #[derive(Clone, Debug, Deserialize, Serialize)]
        #[cfg_attr(feature = "pyo3", pyclass)]
        #[cfg_attr(feature = "pyo3", pyo3(get_all, set_all))]
        /// Cartesian cuboid in
        #[doc = concat!(" `", stringify!($d), "D`")]
        /// with float type
        #[doc = concat!(" `", stringify!($float_type), "`")]
        pub struct $domain_name {
            /// Lower boundary of domain
            pub min: [$float_type; $d],
            /// Upper boundary of domain
            pub max: [$float_type; $d],
            /// Number of voxels in domain along axes
            pub n_voxels: [i64; $d],
            /// Length of individual voxels in domain
            pub dx_voxels: [$float_type; $d],
            /// Initial seed from which to generate seeds for voxels
            pub rng_seed: u64,
        }

        impl $domain_name {
            fn check_min_max(min: [$float_type; $d], max: [$float_type; $d]) -> Result<(), CalcError> {
                for i in 0..$d {
                    match max[i] > min[i] {
                        false => Err(CalcError(format!("Min {:?} must be smaller than Max {:?} for domain boundaries!", min, max))),
                        true => Ok(()),
                    }?;
                }
                Ok(())
            }

            fn check_positive<F>(interaction_ranges: [F; $d]) -> Result<(), CalcError>
            where
                F: PartialOrd + num::Zero + core::fmt::Debug,
            {
                for i in 0..$d {
                    match interaction_ranges[i] > F::zero() {
                        false => Err(CalcError(format!("Interaction range must be positive and non-negative! Got value {:?}", interaction_ranges[i]))),
                        true => Ok(())
                    }?;
                }
                Ok(())
            }

            /// Construct the domain from given lower/upper boundaries and maximum
            /// length of interaction ranges along axes.
            pub fn from_boundaries_and_interaction_ranges(
                min: [$float_type; $d],
                max: [$float_type; $d],
                interaction_ranges: [$float_type; $d]
            ) -> Result<$domain_name, CalcError>
            {
                Self::check_min_max(min, max)?;
                Self::check_positive(interaction_ranges)?;
                let mut n_voxels = [0; $d];
                let mut dx_voxels = [0.0; $d];
                for i in 0..$d {
                    n_voxels[i] = ((max[i] - min[i]) / interaction_ranges[i] * 0.5).ceil() as i64;
                    dx_voxels[i] = (max[i]-min[i])/n_voxels[i] as $float_type;
                }
                Ok(Self {
                    min,
                    max,
                    n_voxels,
                    dx_voxels,
                    rng_seed: 0,
                })
            }

            /// Construct the domain from given lower/upper boundaries and
            /// number of voxels along axes.
            pub fn from_boundaries_and_n_voxels(
                min: [$float_type; $d],
                max: [$float_type; $d],
                n_vox: [usize; $d]
            ) -> Result<$domain_name, CalcError>
            {
                Self::check_min_max(min, max)?;
                Self::check_positive(n_vox)?;
                let mut dx_voxels = [0.0; $d];
                for i in 0..$d {
                    dx_voxels[i] = (max[i] - min[i]) / n_vox[i] as $float_type;
                }
                Ok(Self {
                    min,
                    max,
                    n_voxels: [$(n_vox[$k] as i64),+],
                    dx_voxels,
                    rng_seed: 0,
                })
            }

            fn get_voxel_index(
                &self,
                position: &nalgebra::SVector<$float_type, $d>,
            ) -> Result<[i64; $d], BoundaryError> {
                let mut percent: nalgebra::SVector<$float_type, $d> = self.max.into();
                percent -= nalgebra::SVector::<$float_type, $d>::from(self.min);
                percent = position.component_div(&percent);
                let vox = [$(
                    (percent[$k] * self.n_voxels[$k] as $float_type).floor() as i64,
                )+];

                // If the returned voxel is not positive and smaller than the maximum
                // number of voxel indices this function needs to return an error.
                if vox
                    .iter()
                    .enumerate()
                    .any(|(i, &p)| p<0 && self.n_voxels[i]<p) {
                        return Err(
                            BoundaryError(format!("Cell with position {:?} could not find index in domain with size min: {:?} max: {:?}", position, self.min, self.max))
                        );
                } else {
                    return Ok(vox);
                }
            }

            fn get_neighbor_voxel_indices(&self, index: &[i64; $d]) -> Vec<[i64; $d]> {
                // Create the bounds for the following creation of all the voxel indices
                let bounds: [[i64; 2]; $d] = [$(
                    [
                        max(index[$k] as i32 - 1, 0) as i64,
                        min(index[$k]+2, self.n_voxels[$k])
                    ]
                ),+];

                // Create voxel indices
                let v: Vec<[i64; $d]> = [$($k),+].iter()      // indices supplied in macro invocation
                    .map(|i| (bounds[*i][0]..bounds[*i][1]))    // ranges from bounds
                    .multi_cartesian_product()                  // all possible combinations
                    .map(|ind_v| [$(ind_v[$k]),+])              // multi_cartesian_product gives us vector elements. We map them to arrays.
                    .filter(|ind| ind!=index)                   // filter the elements such that the current index is not included.
                    .collect();                                 // collect into the correct type

                return v;
            }

            fn get_all_voxel_indices(&self) -> Vec<[i64; $d]> {
                [$($k),+]
                    .iter()                                     // indices supplied in macro invocation
                    .map(|i| (0..self.n_voxels[*i]))            // ranges from self.n_vox
                    .multi_cartesian_product()                  // all possible combinations
                    .map(|ind_v| [$(ind_v[$k]),+])              // multi_cartesian_product gives us vector elements. We map them to arrays.
                    .collect()
            }

        }

        #[doc ="Subdomain of ["]
        #[doc = stringify!($domain_name)]
        #[doc = "]"]
        ///
        /// The subdomain contains voxels
        #[derive(Clone, Debug, Deserialize, Serialize)]
        #[cfg_attr(feature = "pyo3", pyclass)]
        #[cfg_attr(feature = "pyo3", pyo3(get_all, set_all))]
        pub struct $subdomain_name {
            /// All voxels contained in this subdomain
            pub voxels: Vec<$voxel_name>,
            domain_min: [$float_type; $d],
            domain_max: [$float_type; $d],
            domain_n_voxels: [i64; $d],
            domain_voxel_sizes: [$float_type; $d],
        }

        #[derive(Clone, Debug, Deserialize, Serialize)]
        #[cfg_attr(feature = "pyo3", pyclass)]
        #[cfg_attr(feature = "pyo3", pyo3(get_all, set_all))]
        /// Voxel of the [
        #[doc = stringify!($subdomain_name)]
        /// ]
        pub struct $voxel_name {
            /// Lower boundary of the voxel
            pub min: [$float_type; $d],
            /// Upper boundary of the voxel
            pub max: [$float_type; $d],
            /// Index of the voxel
            pub ind: [i64; $d],
        }

        impl<C, I: IntoIterator<Item=C>> cellular_raza_concepts::domain_new::Domain<C, $subdomain_name, I> for $domain_name
        where
            C: cellular_raza_concepts::Mechanics<nalgebra::SVector<$float_type, $d>, nalgebra::SVector<$float_type, $d>, nalgebra::SVector<$float_type, $d>, $float_type>,
        {
            // TODO THINK VERY HARD ABOUT THESE TYPES! THEY MIGHT BE CHOSEN STUPIDLY!
            type SubDomainIndex = usize;
            type VoxelIndex = [i64; $d];

            /// Much more research must be done to effectively write this function.
            /// We should be using more sophisticated functionality based on common known facts for
            /// minimizing surface area and number of neighbors.
            /// For more information also see
            /// - [Wikipedia](https://en.wikipedia.org/wiki/Plateau%27s_laws)
            /// - [Math StackExchange](https://math.stackexchange.com/questions/3488409/dividing-a-square-into-n-equal-size-parts-with-minimal-fence)
            fn decompose(
                self,
                n_subdomains: core::num::NonZeroUsize,
                cells: I,
            ) -> Result<cellular_raza_concepts::domain_new::DecomposedDomain<
                Self::SubDomainIndex,
                $subdomain_name,
                C
            >, DecomposeError> {
                let mut indices = self.get_all_voxel_indices();

                let (n, m, average_len);
                match get_decomp_res(indices.len(), n_subdomains.into()) {
                    Some(res) => (n, m, average_len) = res,
                    None => return Err(
                        DecomposeError::Generic("Could not find a suiting decomposition".to_owned())
                    ),
                };

                // TODO optimize this!
                // Currently we are not splitting the voxels apart efficiently
                // These are subdomains which contain n voxels
                let mut ind_n: Vec<Vec<_>> = indices
                    .drain(0..(average_len*n) as usize)
                    .into_iter()
                    .chunks(average_len as usize)
                    .into_iter()
                    .map(|chunk| chunk.collect::<Vec<_>>())
                    .collect();

                // These are subdomains that contain m indices
                let mut ind_m: Vec<Vec<_>> = indices
                    .drain(..)
                    .into_iter()
                    .chunks((max(average_len-1, 1)) as usize)
                    .into_iter()
                    .map(|chunk| chunk.collect::<Vec<_>>())
                    .collect();

                // Combine them into one Vector
                ind_n.append(&mut ind_m);

                // We construct all Voxels which are grouped in their according subdomains
                // Then we construct the subdomain
                let mut index_subdomain_cells: std::collections::HashMap<
                    Self::SubDomainIndex,
                    (_, Vec<C>)
                > = ind_n
                    .clone()
                    .into_iter()
                    .enumerate()
                    .map(|(i, indices)| {
                        let voxels = indices
                            .into_iter()
                            .map(|ind| {
                                let min = [$(self.min[$k] +    ind[$k]  as $float_type*self.dx_voxels[$k]),+];
                                let max = [$(self.min[$k] + (1+ind[$k]) as $float_type*self.dx_voxels[$k]),+];

                                $voxel_name {
                                    min,
                                    max,
                                    ind,
                                }
                            }).collect::<Vec<_>>();
                            (i as Self::SubDomainIndex, ($subdomain_name {
                                voxels,
                                domain_min: self.min,
                                domain_max: self.max,
                                domain_n_voxels: self.n_voxels,
                                domain_voxel_sizes: self.dx_voxels,
                            }, Vec::<C>::new()))
                        }
                    ).collect();

                // Construct a map from voxel_index to subdomain_index
                let voxel_index_to_subdomain_index = ind_n
                    .clone()
                    .into_iter()
                    .enumerate()
                    .map(|(subdomain_index, voxel_indices)| voxel_indices
                        .into_iter()
                        .map(move |voxel_index| (voxel_index, subdomain_index))
                    )
                    .flatten()
                    .collect::<std::collections::HashMap<Self::VoxelIndex, Self::SubDomainIndex>>();

                // Sort the cells into the correct voxels
                cells
                    .into_iter()
                    .map(|cell| {
                        // Get the voxel index of the cell
                        let voxel_index = self.get_voxel_index(&cell.pos())?;
                        // Now get the subdomain index of the voxel
                        let subdomain_index = voxel_index_to_subdomain_index.get(&voxel_index).ok_or(
                            DecomposeError::IndexError(IndexError(
                                format!(
                                    "Could not cell with position {:?} in domain {:?}",
                                    cell.pos(),
                                    self
                                )
                            ))
                        )?;
                        // Then add the cell to the subdomains cells.
                        index_subdomain_cells.get_mut(&subdomain_index).ok_or(
                            DecomposeError::IndexError(IndexError(
                                format!(
                                    "Could not find subdomain index {:?} internally which should\
                                    have been there.",
                                    subdomain_index
                                )
                            ))
                        )?.1.push(cell);
                        Ok(())

                    }).collect::<Result<Vec<_>, DecomposeError>>()?;

                //
                let index_subdomain_cells: Vec<(Self::SubDomainIndex, _, _)> = index_subdomain_cells
                    .into_iter()
                    .map(|(index, (subdomain, cells))| (index, subdomain, cells))
                    .collect();

                let neighbor_map = ind_n
                    .into_iter()
                    .enumerate()
                    .map(|(subdomain_index, voxel_indices)| {
                        let neighbor_voxels = voxel_indices
                            .into_iter()
                            .map(|voxel_index| self.get_neighbor_voxel_indices(&voxel_index))
                            .flatten();
                        let neighbor_subdomains = neighbor_voxels
                            .map(|neighbor_voxel_index| voxel_index_to_subdomain_index
                                .get(&neighbor_voxel_index)
                                .and_then(|v| Some(v.clone()))
                                .ok_or(
                                    DecomposeError::IndexError(IndexError(format!(
                                        "Could not find neighboring voxel index {:?} internally\
                                        which should have been initialized.",
                                        neighbor_voxel_index))
                                )
                            ))
                            .collect::<Result<Vec<usize>, _>>()
                            .and_then(|neighbors| Ok(neighbors
                                .into_iter()
                                .unique()
                                .filter(|neighbor_index| *neighbor_index!=subdomain_index)
                                .collect::<Vec<_>>()))?;
                        Ok((subdomain_index, neighbor_subdomains))
                    })
                    .collect::<Result<_, DecomposeError>>()?;

                Ok(cellular_raza_concepts::domain_new::DecomposedDomain {
                    n_subdomains: (n+m).try_into().unwrap_or(1.try_into().unwrap()),
                    index_subdomain_cells,
                    neighbor_map,
                    rng_seed: self.rng_seed.clone(),
                })
            }
        }

        impl cellular_raza_concepts::domain_new::SubDomain for $subdomain_name
        // where
        //     C: cellular_raza_concepts::Mechanics<SVector<$float_type, $d>, SVector<$float_type, $d>, SVector<$float_type, $d>, $float_type>,
        {
            type VoxelIndex = [i64; $d];


            fn get_neighbor_voxel_indices(&self, index: &Self::VoxelIndex) -> Vec<Self::VoxelIndex> {
                // Create the bounds for the following creation of all the voxel indices
                let bounds: [[i64; 2]; $d] = [$(
                    [
                        max(index[$k] as i32 - 1, 0) as i64,
                        min(index[$k]+2, self.domain_n_voxels[$k])
                    ]
                ),+];

                // Create voxel indices
                let v: Vec<[i64; $d]> = [$($k),+].iter()      // indices supplied in macro invocation
                    .map(|i| (bounds[*i][0]..bounds[*i][1]))    // ranges from bounds
                    .multi_cartesian_product()                  // all possible combinations
                    .map(|ind_v| [$(ind_v[$k]),+])              // multi_cartesian_product gives us vector elements. We map them to arrays.
                    .filter(|ind| ind!=index)                   // filter the elements such that the current index is not included.
                    .collect();                                 // collect into the correct type

                return v;
            }

            fn get_all_indices(&self) -> Vec<Self::VoxelIndex> {
                self.voxels.iter().map(|vox| vox.ind.clone()).collect()
            }
        }

        impl<C> cellular_raza_concepts::domain_new::SortCells<C> for $subdomain_name
        where
            C: Mechanics<
            SVector<$float_type, $d>,
            SVector<$float_type, $d>,
            SVector<$float_type, $d>,
            $float_type,
        >,
        {
            type VoxelIndex = [i64; $d];

            fn get_voxel_index_of(&self, cell: &C) -> Result<Self::VoxelIndex, BoundaryError> {
                let pos = cell.pos();
                let mut out = [0; $d];

                for i in 0..$d {
                    out[i] = ((pos[i] - self.domain_min[0]) / self.domain_voxel_sizes[i]) as i64;
                    out[i] = out[i].min(self.domain_n_voxels[i]-1).max(0);
                }
                Ok(out)
            }
        }

        impl cellular_raza_concepts::domain_new::SubDomainMechanics<
            SVector<$float_type, $d>,
            SVector<$float_type, $d>,
        > for $subdomain_name {
            fn apply_boundary(
                &self,
                pos: &mut SVector<$float_type, $d>,
                velocity: &mut SVector<$float_type, $d>
            ) -> Result<(), BoundaryError> {
                // For each dimension
                for i in 0..$d {
                    // Check if the particle is below lower edge
                    if pos[i] < self.domain_min[i] {
                        pos[i] = 2.0 * self.domain_min[i] - pos[i];
                        velocity[i] = velocity[i].abs();
                    }
                    // Check if the particle is over the edge
                    if pos[i] > self.domain_max[i] {
                        pos[i] = 2.0 * self.domain_max[i] - pos[i];
                        velocity[i] = - velocity[i].abs();
                    }
                }

                // If new position is still out of boundary return error
                for i in 0..$d {
                    if pos[i] < self.domain_min[i] || pos[i] > self.domain_max[i] {
                        return Err(BoundaryError(
                                format!("Particle is out of domain at position {:?}", pos)
                        ));
                    }
                }
                Ok(())
            }
        }
    }
}

implement_cartesian_cuboid_domain_new!(
    1,
    CartesianCuboid1New,
    CartesianSubDomain1,
    CartesianVoxel1,
    f64,
    0
);
implement_cartesian_cuboid_domain_new!(
    2,
    CartesianCuboid2New,
    CartesianSubDomain2,
    CartesianVoxel2,
    f64,
    0,
    1
);
implement_cartesian_cuboid_domain_new!(
    3,
    CartesianCuboid3New,
    CartesianSubDomain3,
    CartesianVoxel3,
    f64,
    0,
    1,
    2
);

implement_cartesian_cuboid_domain_new!(
    1,
    CartesianCuboid1NewF32,
    CartesianSubDomain1F32,
    CartesianVoxel1F32,
    f32,
    0
);
implement_cartesian_cuboid_domain_new!(
    2,
    CartesianCuboid2NewF32,
    CartesianSubDomain2F32,
    CartesianVoxel2F32,
    f32,
    0,
    1
);
implement_cartesian_cuboid_domain_new!(
    3,
    CartesianCuboid3NewF32,
    CartesianSubDomain3F32,
    CartesianVoxel3F32,
    f32,
    0,
    1,
    2
);

// TODO make them only visible if correspoding feature (eg. fluid_mechanics or gradients) is active
define_and_implement_cartesian_cuboid!(1, CartesianCuboid1, 0);
define_and_implement_cartesian_cuboid!(2, CartesianCuboid2, 0, 1);
define_and_implement_cartesian_cuboid!(3, CartesianCuboid3, 0, 1, 2);
implement_cartesian_cuboid_voxel_fluid_mechanics!(1, CartesianCuboid1, CartesianCuboidVoxel1, 0);
implement_cartesian_cuboid_voxel_fluid_mechanics!(2, CartesianCuboid2, CartesianCuboidVoxel2, 0, 1);
implement_cartesian_cuboid_voxel_fluid_mechanics!(
    3,
    CartesianCuboid3,
    CartesianCuboidVoxel3,
    0,
    1,
    2
);

impl CreatePlottingRoot for CartesianCuboid2 {
    fn create_bitmap_root<'a, T>(
        &self,
        image_size: u32,
        filename: &'a T,
    ) -> Result<
        DrawingArea<BitMapBackend<'a>, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
        DrawingError,
    >
    where
        T: AsRef<std::path::Path> + ?Sized,
    {
        // Calculate the images dimensions by the dimensions of the simulation domain
        let dx = (self.max[0] - self.min[0]).abs();
        let dy = (self.max[1] - self.min[1]).abs();
        let q = dx.min(dy);
        let image_size_x = (image_size as f64 * dx / q).round() as u32;
        let image_size_y = (image_size as f64 * dy / q).round() as u32;

        // Create a domain with the correct size and fill it white.
        use plotters::drawing::IntoDrawingArea;
        let root = BitMapBackend::new(filename, (image_size_x, image_size_y)).into_drawing_area();
        root.fill(&plotters::prelude::full_palette::WHITE).unwrap();

        // Build a chart on the domain such that plotting later will be simplified
        let mut chart = plotters::prelude::ChartBuilder::on(&root)
            // Finally attach a coordinate on the drawing area and make a chart context
            .build_cartesian_2d(self.min[0]..self.max[0], self.min[1]..self.max[1])
            .unwrap();

        let root = chart.plotting_area().clone();

        chart
            .configure_mesh()
            // we do not want to draw any mesh lines automatically
            .disable_mesh()
            .draw()
            .unwrap();

        Ok(root)
    }
}

#[cfg(test)]
mod test {
    use super::get_decomp_res;
    use rayon::prelude::*;

    #[test]
    fn test_get_demomp_res() {
        #[cfg(debug_assertions)]
        let max = 500;
        #[cfg(not(debug_assertions))]
        let max = 5_000;

        (1..max)
            .into_par_iter()
            .map(|n_voxel| {
                #[cfg(debug_assertions)]
                let max_regions = 100;
                #[cfg(not(debug_assertions))]
                let max_regions = 1_000;
                for n_regions in 1..max_regions {
                    match get_decomp_res(n_voxel, n_regions) {
                        Some(res) => {
                            let (n, m, average_len) = res;
                            assert_eq!(n + m, n_regions);
                            assert_eq!(n * average_len + m * (average_len - 1), n_voxel);
                        }
                        None => panic!(
                            "No result for inputs n_voxel: {} n_regions: {}",
                            n_voxel, n_regions
                        ),
                    }
                }
            })
            .collect::<Vec<()>>();
    }
}
