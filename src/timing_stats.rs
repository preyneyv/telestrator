use std::{collections::HashMap, time::Instant};

const TIMING_ACCUMULATOR_SIZE: usize = 120;

pub struct TimingStats {
    last_frame: Option<Instant>,
    label: String,
    accumulator: Vec<u32>,

    trackers: HashMap<String, Instant>,
    additional_accumulators: HashMap<String, Vec<u32>>,
    labelled_values: HashMap<String, (Vec<u32>, String)>,
}

impl TimingStats {
    pub fn new(label: String) -> Self {
        Self {
            label,
            last_frame: None,
            accumulator: Vec::with_capacity(TIMING_ACCUMULATOR_SIZE),
            trackers: HashMap::new(),
            additional_accumulators: HashMap::new(),
            labelled_values: HashMap::new(),
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

            self.additional_accumulators.iter().for_each(|(k, v)| {
                let sum: u32 = v.iter().sum();
                let average = (sum as f32) / (v.len() as f32);
                println!("   - {k}\t{:.2}μs", average);
            });
            self.labelled_values.iter().for_each(|(k, (v, u))| {
                let sum: u32 = v.iter().sum();
                let average = (sum as f32) / (v.len() as f32);
                println!("   - {k}\t{:.2}{u}", average);
            });

            self.additional_accumulators.clear();
            self.trackers.clear();
            self.accumulator.clear();
            self.labelled_values.clear();
        }

        self.last_frame = Some(Instant::now());
    }

    pub fn start(&mut self, label: &str) {
        if self.trackers.contains_key(label) {
            eprintln!(
                "stats ({}) ignoring start {}, existing timer running",
                self.label, label
            );
            return;
        }

        self.trackers.insert(label.to_string(), Instant::now());
    }

    pub fn end(&mut self, label: &str) {
        if !self.trackers.contains_key(label) {
            eprintln!(
                "stats({}) ignoring start {}, non existent timer",
                self.label, label
            );
            return;
        }

        let duration: u32 = Instant::now()
            .duration_since(self.trackers.remove(label).unwrap().clone())
            .as_micros()
            .try_into()
            .expect("oops");
        match self.additional_accumulators.get_mut(label) {
            Some(acc) => {
                acc.push(duration);
            }
            None => {
                self.additional_accumulators
                    .insert(label.to_string(), vec![duration]);
            }
        };
    }
    pub fn track(&mut self, label: &str, value: u32, unit: &str) {
        match self.labelled_values.get_mut(label) {
            Some((vec, old_unit)) => {
                old_unit.replace_range(0..old_unit.len(), unit);
                vec.push(value)
            }
            None => {
                self.labelled_values
                    .insert(label.into(), (vec![value], unit.into()));
            }
        }
    }
}
