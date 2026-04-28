use crate::alarms_handler::{AlarmStack, AlarmTracker};
use crate::constants::{ALARMS_CHANNELS_AMOUNT, ALARMS_MESSAGE_STRING_LENGTH, ALARMS_STACK_DEPTH};

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum VisualizationState {
    On,
    Off,
}

pub type VisualizationFrame = [VisualizationState; ALARMS_CHANNELS_AMOUNT];

pub fn build_visualization_frames(
    alarm_str: &str,
) -> Option<[VisualizationFrame; ALARMS_STACK_DEPTH]> {
    if alarm_str.len() != ALARMS_MESSAGE_STRING_LENGTH {
        return None;
    }

    let mut alarm_chars = ['\0'; ALARMS_MESSAGE_STRING_LENGTH];
    for (i, c) in alarm_str.chars().enumerate() {
        alarm_chars[i] = c;
    }

    let mut temp_stack = AlarmStack::new();
    temp_stack.import_bits(alarm_chars);
    let matrix = temp_stack.get_stack_view();

    let mut frames = [[VisualizationState::Off; ALARMS_CHANNELS_AMOUNT]; ALARMS_STACK_DEPTH];
    for (row_idx, row) in matrix.iter().enumerate() {
        for (col_idx, &active) in row.iter().enumerate() {
            frames[row_idx][col_idx] = if active {
                VisualizationState::On
            } else {
                VisualizationState::Off
            };
        }
    }

    Some(frames)
}
