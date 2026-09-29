//! Dense row-major `f32` matrix. `m[r]` is row `r` as a slice, so `m[r][c]`
//! reads like the Java `float[][]` it replaces while storing one contiguous
//! buffer.

use std::ops::{Index, IndexMut};

#[derive(Debug, Clone, Default)]
pub(crate) struct Matrix {
    data: Vec<f32>,
    cols: usize,
}

impl Matrix {
    pub(crate) fn new(rows: usize, cols: usize) -> Self {
        Self {
            data: vec![0.0; rows * cols],
            cols,
        }
    }

    #[inline]
    pub(crate) fn get(&self, r: usize, c: usize) -> f32 {
        self.data[r * self.cols + c]
    }

    #[inline]
    pub(crate) fn set(&mut self, r: usize, c: usize, v: f32) {
        self.data[r * self.cols + c] = v;
    }

    #[inline]
    pub(crate) fn cols(&self) -> usize {
        self.cols
    }

    #[inline]
    pub(crate) fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.data
    }

    pub(crate) fn rows_mut(&mut self) -> std::slice::ChunksExactMut<'_, f32> {
        self.data.chunks_exact_mut(self.cols.max(1))
    }
}

impl Index<usize> for Matrix {
    type Output = [f32];
    #[inline]
    fn index(&self, r: usize) -> &[f32] {
        &self.data[r * self.cols..(r + 1) * self.cols]
    }
}

impl IndexMut<usize> for Matrix {
    #[inline]
    fn index_mut(&mut self, r: usize) -> &mut [f32] {
        &mut self.data[r * self.cols..(r + 1) * self.cols]
    }
}
