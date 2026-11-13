use crate::{
    AcpiTable,
    sdt::{SdtHeader, Signature},
};
use core::{marker::PhantomPinned, mem, pin::Pin, slice};
use log::warn;

/// The SLIT (System Locality Information Table) provides a matrix of relative distances between
/// all proximity domains. It encodes a list of N*N entries, where each entry is `u8` and the
/// relative distance between proximity domains `(i, j)` is the entry at `i*N+j`. The distance
/// between a proximity domain and itself is normalised to a value of `10`.
#[derive(Debug)]
#[repr(C, packed)]
pub struct Slit {
    pub header: SdtHeader,
    pub num_proximity_domains: u64,
    _pinned: PhantomPinned,
}

unsafe impl AcpiTable for Slit {
    const SIGNATURE: Signature = Signature::SLIT;

    fn header(&self) -> &SdtHeader {
        &self.header
    }
}

impl Slit {
    pub fn matrix(self: Pin<&Self>) -> DistanceMatrix<'_> {
        DistanceMatrix { num_proximity_domains: self.num_proximity_domains, matrix: self.matrix_raw() }
    }

    pub fn matrix_raw(self: Pin<&Self>) -> &[u8] {
        let available = (self.header.length as usize).saturating_sub(mem::size_of::<Slit>());
        let num_proximity_domains = self.num_proximity_domains;
        let num_entries = match num_proximity_domains.checked_mul(num_proximity_domains) {
            Some(num_entries) if num_entries <= available as u64 => num_entries as usize,
            _ => {
                warn!("SLIT too short for given number of proximity domains! Returning empty matrix");
                0
            }
        };

        unsafe {
            let ptr = Pin::into_inner_unchecked(self) as *const Slit as *const u8;
            slice::from_raw_parts(ptr.byte_add(mem::size_of::<Slit>()), num_entries)
        }
    }
}

pub struct DistanceMatrix<'a> {
    pub num_proximity_domains: u64,
    pub matrix: &'a [u8],
}

impl DistanceMatrix<'_> {
    pub fn distance(&self, i: u32, j: u32) -> Option<u8> {
        if (i as u64) < self.num_proximity_domains && (j as u64) < self.num_proximity_domains {
            // The matrix is empty when the table refused its own count, so the cell may be absent.
            let index = (i as u64).checked_mul(self.num_proximity_domains)?.checked_add(j as u64)?;
            self.matrix.get(usize::try_from(index).ok()?).copied()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A two-locality SLIT whose distances differ by direction, so a transposed lookup shows.
    fn table() -> [u8; 44 + 4] {
        let mut bytes = [0u8; 44 + 4];
        bytes[0..4].copy_from_slice(b"SLIT");
        bytes[4..8].copy_from_slice(&(44u32 + 4).to_le_bytes());
        bytes[36..44].copy_from_slice(&2u64.to_le_bytes());
        bytes[44..48].copy_from_slice(&[10, 20, 21, 10]);
        bytes
    }

    #[test]
    fn matrix_covers_every_locality_pair() {
        let bytes = table();
        let slit = unsafe { Pin::new_unchecked(&*(bytes.as_ptr() as *const Slit)) };
        assert_eq!(slit.matrix_raw(), &[10, 20, 21, 10]);
    }

    #[test]
    fn a_count_whose_square_overflows_or_exceeds_the_table_has_no_matrix() {
        for count in [1u64 << 32, u64::MAX, 3] {
            let mut bytes = table();
            bytes[36..44].copy_from_slice(&count.to_le_bytes());
            let slit = unsafe { Pin::new_unchecked(&*(bytes.as_ptr() as *const Slit)) };
            assert!(slit.matrix_raw().is_empty());
            assert_eq!(slit.matrix().distance(0, 0), None);
        }
    }

    #[test]
    fn distance_is_indexed_from_then_to() {
        let bytes = table();
        let slit = unsafe { Pin::new_unchecked(&*(bytes.as_ptr() as *const Slit)) };
        let matrix = slit.matrix();
        assert_eq!(matrix.distance(0, 0), Some(10));
        assert_eq!(matrix.distance(0, 1), Some(20));
        assert_eq!(matrix.distance(1, 0), Some(21));
        assert_eq!(matrix.distance(2, 0), None);
        assert_eq!(matrix.distance(0, 2), None);
    }
}
