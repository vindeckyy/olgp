//! Small deterministic recognizer used by `olgpctl bench`.

/// Semantic op used by the benchmark.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Op {
    Launch,
    Sync,
    Other,
}

/// Recognizes `LAUNCH -> LAUNCH` batches.
#[derive(Debug, Default)]
pub struct BatchRecognizer {
    held: bool,
    hits: u64,
}

impl BatchRecognizer {
    /// Note one operation.
    pub fn note(&mut self, op: Op) {
        if self.held && op == Op::Launch {
            self.hits += 1;
            self.held = false;
            return;
        }
        self.held = op == Op::Launch;
    }

    /// Number of recognized batches.
    #[must_use]
    pub fn hits(&self) -> u64 {
        self.hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_launch_pairs() {
        let mut recognizer = BatchRecognizer::default();
        recognizer.note(Op::Launch);
        recognizer.note(Op::Launch);
        assert_eq!(recognizer.hits(), 1);
    }
}
