use crate::visualization::{VisualizationState, build_visualization_frames};

#[test]
fn builds_visualization_frames_from_alarm_bits() {
    let frames = build_visualization_frames("6402").unwrap();

    assert_eq!(frames[0], [VisualizationState::Off, VisualizationState::Off, VisualizationState::Off, VisualizationState::Off]);
    assert_eq!(frames[1], [VisualizationState::On, VisualizationState::Off, VisualizationState::Off, VisualizationState::On]);
    assert_eq!(frames[2], [VisualizationState::On, VisualizationState::On, VisualizationState::Off, VisualizationState::Off]);
}

#[test]
fn rejects_invalid_visualization_payload_length() {
    assert!(build_visualization_frames("12").is_none());
}
