use secs_jos::Limits;

use crate::MdatError;

/// Cumulative allocation budget for data copied out of a parsed MDAT stream.
///
/// It starts at `Limits::max_file_bytes`. Adaptation and the IR and
/// source-info extraction that follows charge the same counter before each
/// copy, so repeated references to one large array or string cannot amplify
/// a small file into an unbounded allocation.
#[derive(Debug)]
pub struct AllocationBudget {
    remaining: usize,
}

impl AllocationBudget {
    pub fn new(limits: &Limits) -> Self {
        Self {
            remaining: limits.max_file_bytes,
        }
    }

    pub fn remaining(&self) -> usize {
        self.remaining
    }

    pub(crate) fn charge(&mut self, bytes: usize) -> Result<(), MdatError> {
        self.remaining = self
            .remaining
            .checked_sub(bytes)
            .ok_or_else(|| MdatError::Schema("adapted data exceeds allocation limit".into()))?;
        Ok(())
    }
}
