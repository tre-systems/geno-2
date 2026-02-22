use app_web::input::{MultiTouchState, TouchGestureKind};

// ────────────────── TouchGestureKind ──────────────────

#[test]
fn gesture_kind_default_is_none() {
    assert_eq!(TouchGestureKind::default(), TouchGestureKind::None);
}

#[test]
fn gesture_kinds_are_distinct() {
    let kinds = [
        TouchGestureKind::None,
        TouchGestureKind::TwoFingerPinchRotate,
        TouchGestureKind::ThreeFingerSwipe,
        TouchGestureKind::FourFingerTap,
        TouchGestureKind::FiveFingerTap,
    ];
    for (i, a) in kinds.iter().enumerate() {
        for (j, b) in kinds.iter().enumerate() {
            if i == j {
                assert_eq!(a, b);
            } else {
                assert_ne!(a, b);
            }
        }
    }
}

// ────────────────── MultiTouchState basics ──────────────────

#[test]
fn multitouch_state_default_is_empty() {
    let mt = MultiTouchState::default();
    assert!(mt.pointers.is_empty());
    assert_eq!(mt.gesture_kind, TouchGestureKind::None);
    assert_eq!(mt.peak_pointer_count, 0);
    assert!(!mt.gesture_committed);
}

#[test]
fn pointer_add_remove() {
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [100.0, 200.0]);
    assert_eq!(mt.pointers.len(), 1);

    mt.pointers.insert(2, [300.0, 400.0]);
    assert_eq!(mt.pointers.len(), 2);

    mt.pointers.remove(&1);
    assert_eq!(mt.pointers.len(), 1);
    assert!(mt.pointers.contains_key(&2));
}

// ────────────────── two_finger_metrics ──────────────────

#[test]
fn two_finger_metrics_none_when_empty() {
    let mt = MultiTouchState::default();
    assert!(mt.two_finger_metrics().is_none());
}

#[test]
fn two_finger_metrics_none_when_one_pointer() {
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [100.0, 200.0]);
    assert!(mt.two_finger_metrics().is_none());
}

#[test]
fn two_finger_metrics_returns_distance_and_angle() {
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [0.0, 0.0]);
    mt.pointers.insert(2, [300.0, 400.0]);

    let (dist, angle) = mt.two_finger_metrics().unwrap();
    // Distance should be 500 (3-4-5 triangle)
    assert!((dist - 500.0).abs() < 0.1);
    // Angle should be atan2(400, 300) ≈ 0.927 rad or atan2(-400, -300) (differs by PI)
    // due to HashMap non-deterministic iteration order
    let expected = 400.0_f32.atan2(300.0);
    let diff = (angle - expected).abs();
    assert!(diff < 0.01 || (diff - std::f32::consts::PI).abs() < 0.01);
}

#[test]
fn two_finger_metrics_min_distance_is_one() {
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [100.0, 100.0]);
    mt.pointers.insert(2, [100.0, 100.0]); // Same position

    let (dist, _) = mt.two_finger_metrics().unwrap();
    assert!(dist >= 1.0, "minimum distance should be clamped to 1.0");
}

#[test]
fn two_finger_metrics_horizontal() {
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [0.0, 50.0]);
    mt.pointers.insert(2, [200.0, 50.0]);

    let (dist, angle) = mt.two_finger_metrics().unwrap();
    assert!((dist - 200.0).abs() < 0.1);
    // Horizontal: angle is 0 or ±PI depending on HashMap iteration order
    assert!(
        angle.abs() < 0.01 || (angle.abs() - std::f32::consts::PI).abs() < 0.01,
        "expected horizontal angle (0 or ±PI), got {}",
        angle
    );
}

#[test]
fn two_finger_metrics_vertical() {
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [50.0, 0.0]);
    mt.pointers.insert(2, [50.0, 200.0]);

    let (dist, angle) = mt.two_finger_metrics().unwrap();
    assert!((dist - 200.0).abs() < 0.1);
    // Vertical: angle is ±PI/2 depending on HashMap iteration order
    assert!(
        (angle.abs() - std::f32::consts::FRAC_PI_2).abs() < 0.01,
        "expected vertical angle (±PI/2), got {}",
        angle
    );
}

// ────────────────── midpoint_uv ──────────────────

#[test]
fn midpoint_uv_none_when_insufficient_pointers() {
    let mt = MultiTouchState::default();
    assert!(mt.midpoint_uv(800.0, 600.0).is_none());

    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [100.0, 200.0]);
    assert!(mt.midpoint_uv(800.0, 600.0).is_none());
}

#[test]
fn midpoint_uv_computes_center() {
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [200.0, 150.0]);
    mt.pointers.insert(2, [600.0, 450.0]);

    let uv = mt.midpoint_uv(800.0, 600.0).unwrap();
    // Midpoint = (400, 300), UV = (0.5, 0.5)
    assert!((uv[0] - 0.5).abs() < 0.01);
    assert!((uv[1] - 0.5).abs() < 0.01);
}

#[test]
fn midpoint_uv_clamps_to_unit_range() {
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [-100.0, -50.0]);
    mt.pointers.insert(2, [100.0, 50.0]);

    let uv = mt.midpoint_uv(800.0, 600.0).unwrap();
    assert!(uv[0] >= 0.0 && uv[0] <= 1.0);
    assert!(uv[1] >= 0.0 && uv[1] <= 1.0);
}

// ────────────────── centroid_px ──────────────────

#[test]
fn centroid_px_none_when_empty() {
    let mt = MultiTouchState::default();
    assert!(mt.centroid_px().is_none());
}

#[test]
fn centroid_px_single_pointer() {
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [120.0, 240.0]);

    let c = mt.centroid_px().unwrap();
    assert!((c[0] - 120.0).abs() < 0.01);
    assert!((c[1] - 240.0).abs() < 0.01);
}

#[test]
fn centroid_px_two_pointers() {
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [100.0, 200.0]);
    mt.pointers.insert(2, [300.0, 400.0]);

    let c = mt.centroid_px().unwrap();
    assert!((c[0] - 200.0).abs() < 0.01);
    assert!((c[1] - 300.0).abs() < 0.01);
}

#[test]
fn centroid_px_three_pointers() {
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [0.0, 0.0]);
    mt.pointers.insert(2, [300.0, 0.0]);
    mt.pointers.insert(3, [0.0, 300.0]);

    let c = mt.centroid_px().unwrap();
    assert!((c[0] - 100.0).abs() < 0.01);
    assert!((c[1] - 100.0).abs() < 0.01);
}

#[test]
fn centroid_px_five_pointers() {
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [100.0, 100.0]);
    mt.pointers.insert(2, [200.0, 100.0]);
    mt.pointers.insert(3, [300.0, 100.0]);
    mt.pointers.insert(4, [100.0, 300.0]);
    mt.pointers.insert(5, [300.0, 300.0]);

    let c = mt.centroid_px().unwrap();
    assert!((c[0] - 200.0).abs() < 0.01);
    assert!((c[1] - 180.0).abs() < 0.01);
}

// ────────────────── centroid_uv ──────────────────

#[test]
fn centroid_uv_none_when_empty() {
    let mt = MultiTouchState::default();
    assert!(mt.centroid_uv(800.0, 600.0).is_none());
}

#[test]
fn centroid_uv_computes_normalized_center() {
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [400.0, 300.0]);

    let uv = mt.centroid_uv(800.0, 600.0).unwrap();
    assert!((uv[0] - 0.5).abs() < 0.01);
    assert!((uv[1] - 0.5).abs() < 0.01);
}

#[test]
fn centroid_uv_clamps_to_unit_range() {
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [1600.0, 1200.0]);

    let uv = mt.centroid_uv(800.0, 600.0).unwrap();
    assert_eq!(uv[0], 1.0);
    assert_eq!(uv[1], 1.0);
}

// ────────────────── reset_gesture ──────────────────

#[test]
fn reset_gesture_clears_state_but_keeps_pointers() {
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [100.0, 200.0]);
    mt.pointers.insert(2, [300.0, 400.0]);
    mt.gesture_kind = TouchGestureKind::TwoFingerPinchRotate;
    mt.peak_pointer_count = 2;
    mt.initial_distance = 250.0;
    mt.initial_angle = 1.5;
    mt.initial_bpm = 120.0;
    mt.initial_detune = 50.0;
    mt.initial_centroid = [200.0, 300.0];
    mt.gesture_committed = true;

    mt.reset_gesture();

    // Pointers should remain
    assert_eq!(mt.pointers.len(), 2);

    // Everything else should be reset
    assert_eq!(mt.gesture_kind, TouchGestureKind::None);
    assert_eq!(mt.peak_pointer_count, 0);
    assert_eq!(mt.initial_distance, 0.0);
    assert_eq!(mt.initial_angle, 0.0);
    assert_eq!(mt.initial_bpm, 0.0);
    assert_eq!(mt.initial_detune, 0.0);
    assert_eq!(mt.initial_centroid, [0.0, 0.0]);
    assert!(!mt.gesture_committed);
}

// ────────────────── gesture lifecycle simulation ──────────────────

#[test]
fn simulate_two_finger_gesture_lifecycle() {
    let mut mt = MultiTouchState::default();

    // Finger 1 down
    mt.pointers.insert(1, [100.0, 300.0]);
    assert_eq!(mt.pointers.len(), 1);
    assert!(mt.two_finger_metrics().is_none());

    // Finger 2 down → start gesture
    mt.pointers.insert(2, [500.0, 300.0]);
    mt.peak_pointer_count = 2;
    mt.gesture_kind = TouchGestureKind::TwoFingerPinchRotate;

    let (dist, _angle) = mt.two_finger_metrics().unwrap();
    mt.initial_distance = dist;
    assert!((dist - 400.0).abs() < 0.1);

    // Simulate pinch (fingers move closer)
    mt.pointers.insert(1, [200.0, 300.0]);
    mt.pointers.insert(2, [400.0, 300.0]);
    let (new_dist, _) = mt.two_finger_metrics().unwrap();
    let ratio = new_dist / mt.initial_distance;
    assert!(ratio < 1.0, "pinch should yield ratio < 1.0");
    assert!((ratio - 0.5).abs() < 0.01);

    // Finger 2 up
    mt.pointers.remove(&2);
    assert_eq!(mt.pointers.len(), 1);

    // Finger 1 up
    mt.pointers.remove(&1);
    assert!(mt.pointers.is_empty());
    mt.reset_gesture();
    assert_eq!(mt.gesture_kind, TouchGestureKind::None);
}

#[test]
fn simulate_three_finger_horizontal_swipe() {
    let mut mt = MultiTouchState::default();

    // Three fingers down at same Y
    mt.pointers.insert(1, [100.0, 300.0]);
    mt.pointers.insert(2, [200.0, 300.0]);
    mt.pointers.insert(3, [300.0, 300.0]);
    mt.gesture_kind = TouchGestureKind::ThreeFingerSwipe;
    mt.peak_pointer_count = 3;

    let centroid = mt.centroid_px().unwrap();
    mt.initial_centroid = centroid;
    assert!((centroid[0] - 200.0).abs() < 0.01);
    assert!((centroid[1] - 300.0).abs() < 0.01);

    // Swipe right: all fingers move +100px in X
    mt.pointers.insert(1, [200.0, 300.0]);
    mt.pointers.insert(2, [300.0, 300.0]);
    mt.pointers.insert(3, [400.0, 300.0]);

    let final_centroid = mt.centroid_px().unwrap();
    let dx = final_centroid[0] - mt.initial_centroid[0];
    let dy = final_centroid[1] - mt.initial_centroid[1];

    // Should be a horizontal swipe
    assert!(dx.abs() > dy.abs());
    assert!(dx > 0.0, "should be rightward");
    assert!((dx - 100.0).abs() < 0.01);
}

#[test]
fn simulate_three_finger_vertical_swipe() {
    let mut mt = MultiTouchState::default();

    // Three fingers down at same X
    mt.pointers.insert(1, [300.0, 100.0]);
    mt.pointers.insert(2, [300.0, 200.0]);
    mt.pointers.insert(3, [300.0, 300.0]);
    mt.gesture_kind = TouchGestureKind::ThreeFingerSwipe;
    mt.peak_pointer_count = 3;
    mt.initial_centroid = mt.centroid_px().unwrap();

    // Swipe down: all fingers move +80px in Y
    mt.pointers.insert(1, [300.0, 180.0]);
    mt.pointers.insert(2, [300.0, 280.0]);
    mt.pointers.insert(3, [300.0, 380.0]);

    let final_centroid = mt.centroid_px().unwrap();
    let dx = final_centroid[0] - mt.initial_centroid[0];
    let dy = final_centroid[1] - mt.initial_centroid[1];

    assert!(dy.abs() > dx.abs());
    assert!(dy > 0.0, "should be downward");
}

#[test]
fn simulate_gesture_upgrade_two_to_three_fingers() {
    let mut mt = MultiTouchState::default();

    // Start with 2 fingers
    mt.pointers.insert(1, [100.0, 200.0]);
    mt.pointers.insert(2, [300.0, 200.0]);
    mt.gesture_kind = TouchGestureKind::TwoFingerPinchRotate;
    mt.peak_pointer_count = 2;

    // Third finger arrives → upgrade
    mt.pointers.insert(3, [200.0, 400.0]);
    mt.gesture_kind = TouchGestureKind::ThreeFingerSwipe;
    mt.peak_pointer_count = 3;
    mt.initial_centroid = mt.centroid_px().unwrap();

    assert_eq!(mt.gesture_kind, TouchGestureKind::ThreeFingerSwipe);
    assert_eq!(mt.peak_pointer_count, 3);
    assert!(mt.centroid_px().is_some());
}

#[test]
fn four_finger_gesture_commits_once() {
    let mut mt = MultiTouchState::default();

    mt.pointers.insert(1, [100.0, 100.0]);
    mt.pointers.insert(2, [200.0, 100.0]);
    mt.pointers.insert(3, [300.0, 100.0]);
    mt.pointers.insert(4, [400.0, 100.0]);

    assert!(!mt.gesture_committed);
    mt.gesture_kind = TouchGestureKind::FourFingerTap;
    mt.gesture_committed = true;

    // Adding a 5th finger should not re-commit if already committed
    mt.pointers.insert(5, [500.0, 100.0]);
    assert!(mt.gesture_committed); // Still committed from the 4-finger action
}

#[test]
fn five_finger_gesture_lifecycle() {
    let mut mt = MultiTouchState::default();

    for i in 1..=5 {
        mt.pointers.insert(i, [i as f32 * 100.0, 300.0]);
    }
    mt.peak_pointer_count = 5;
    mt.gesture_kind = TouchGestureKind::FiveFingerTap;
    mt.gesture_committed = true;

    assert_eq!(mt.pointers.len(), 5);
    assert_eq!(mt.gesture_kind, TouchGestureKind::FiveFingerTap);

    // All fingers lift
    for i in 1..=5 {
        mt.pointers.remove(&i);
    }
    assert!(mt.pointers.is_empty());
    mt.reset_gesture();
    assert_eq!(mt.gesture_kind, TouchGestureKind::None);
    assert!(!mt.gesture_committed);
}

// ────────────────── edge cases ──────────────────

#[test]
fn centroid_with_negative_coordinates() {
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [-100.0, -200.0]);
    mt.pointers.insert(2, [100.0, 200.0]);

    let c = mt.centroid_px().unwrap();
    assert!((c[0] - 0.0).abs() < 0.01);
    assert!((c[1] - 0.0).abs() < 0.01);

    // UV should clamp
    let uv = mt.centroid_uv(800.0, 600.0).unwrap();
    assert_eq!(uv[0], 0.0);
    assert_eq!(uv[1], 0.0);
}

#[test]
fn two_finger_rotation_wrapping() {
    // Test that angle computation handles the full circle
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [400.0, 300.0]); // Center
    mt.pointers.insert(2, [500.0, 300.0]); // Right

    let (_, angle_right) = mt.two_finger_metrics().unwrap();
    assert!(angle_right.abs() < 0.01, "pointing right should be ~0 rad");

    mt.pointers.insert(2, [300.0, 300.0]); // Left
    let (_, angle_left) = mt.two_finger_metrics().unwrap();
    assert!(
        (angle_left.abs() - std::f32::consts::PI).abs() < 0.01,
        "pointing left should be ~±PI rad"
    );
}

#[test]
fn midpoint_uv_at_canvas_corners() {
    let w = 800.0;
    let h = 600.0;

    // Top-left corner
    let mut mt = MultiTouchState::default();
    mt.pointers.insert(1, [0.0, 0.0]);
    mt.pointers.insert(2, [0.0, 0.0]);
    let uv = mt.midpoint_uv(w, h).unwrap();
    assert_eq!(uv[0], 0.0);
    assert_eq!(uv[1], 0.0);

    // Bottom-right corner
    mt.pointers.insert(1, [w, h]);
    mt.pointers.insert(2, [w, h]);
    let uv = mt.midpoint_uv(w, h).unwrap();
    assert_eq!(uv[0], 1.0);
    assert_eq!(uv[1], 1.0);
}
