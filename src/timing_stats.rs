use std::time::Instant;

const TIMING_ACCUMULATOR_SIZE: usize = 120;

pub struct TimingStats {
    last_frame: Option<Instant>,
    label: String,
    accumulator: Vec<u32>,
}

impl TimingStats {
    pub fn new(label: String) -> Self {
        Self {
            label,
            last_frame: None,
            accumulator: Vec::with_capacity(TIMING_ACCUMULATOR_SIZE),
        }
    }
    pub fn tick(&mut self) {
        if let Some(last_frame) = self.last_frame {
            self.accumulator
                .push(last_frame.elapsed().as_micros().try_into().expect("oop"));
        }

        if self.accumulator.len() == TIMING_ACCUMULATOR_SIZE {
            let sum: u32 = self.accumulator.iter().sum();
            let average = sum / (TIMING_ACCUMULATOR_SIZE as u32);
            println!(
                " stats ({}):\t{}μs\t{:.2} fps",
                self.label,
                average,
                1_000_000f32 / (average as f32)
            );
            self.accumulator.clear();
        }

        self.last_frame = Some(Instant::now());
    }
}
