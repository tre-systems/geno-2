use app_web::input::{MultiTouchState, TouchGestureKind, MAX_TOUCH_POINTS};

/// Build a state with the given pointers, in `(id, [x, y])` form.
fn mt_with(pointers: &[(i32, [f32; 2])]) -> MultiTouchState {
    let mut mt = MultiTouchState::default();
    for &(id, pos) in pointers {
        mt.pointers.insert(id, pos);
    }
    mt
}

// ────────────────── TouchGestureKind ──────────────────

#[test]
fn gesture_kind_default_is_none() {
    assert_eq!(TouchGestureKind::default(), TouchGestureKind::None);
}

#[test]
fn gesture_kinds_are_distinct() {
    let kinds = [TouchGestureKind::None, TouchGestureKind::PerformanceSurface];
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
    assert_eq!(mt.motion_px, 0.0);
    assert_eq!(mt.last_ripple_motion, 0.0);
}

#[test]
fn pointer_add_remove() {
    let mut mt = mt_with(&[(1, [100.0, 200.0])]);
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
    assert!(MultiTouchState::default().two_finger_metrics().is_none());
}

#[test]
fn two_finger_metrics_none_when_one_pointer() {
    let mt = mt_with(&[(1, [100.0, 200.0])]);
    assert!(mt.two_finger_metrics().is_none());
}

#[test]
fn two_finger_metrics_returns_distance_and_angle() {
    let mt = mt_with(&[(1, [0.0, 0.0]), (2, [300.0, 400.0])]);

    let (dist, angle) = mt.two_finger_metrics().unwrap();
    assert!((dist - 500.0).abs() < 0.1, "3-4-5 triangle hypotenuse");
    // Pointers are sorted by id, so the angle runs id1(0,0) → id2(300,400).
    let expected = 400.0_f32.atan2(300.0);
    assert!((angle - expected).abs() < 0.01);
}

#[test]
fn two_finger_metrics_min_distance_is_one() {
    let mt = mt_with(&[(1, [100.0, 100.0]), (2, [100.0, 100.0])]);

    let (dist, _) = mt.two_finger_metrics().unwrap();
    assert!(dist >= 1.0, "coincident pointers clamp distance to 1.0");
}

#[test]
fn two_finger_metrics_horizontal() {
    let mt = mt_with(&[(1, [0.0, 50.0]), (2, [200.0, 50.0])]);

    let (dist, angle) = mt.two_finger_metrics().unwrap();
    assert!((dist - 200.0).abs() < 0.1);
    assert!(
        angle.abs() < 0.01,
        "horizontal id1→id2 is ~0 rad, got {angle}"
    );
}

#[test]
fn two_finger_metrics_vertical() {
    let mt = mt_with(&[(1, [50.0, 0.0]), (2, [50.0, 200.0])]);

    let (dist, angle) = mt.two_finger_metrics().unwrap();
    assert!((dist - 200.0).abs() < 0.1);
    assert!(
        (angle - std::f32::consts::FRAC_PI_2).abs() < 0.01,
        "vertical id1→id2 is ~PI/2 rad, got {angle}"
    );
}

// ────────────────── midpoint_uv ──────────────────

#[test]
fn midpoint_uv_none_when_insufficient_pointers() {
    let empty = MultiTouchState::default();
    assert!(empty.midpoint_uv(800.0, 600.0).is_none());

    let one = mt_with(&[(1, [100.0, 200.0])]);
    assert!(one.midpoint_uv(800.0, 600.0).is_none());
}

#[test]
fn midpoint_uv_computes_center() {
    let mt = mt_with(&[(1, [200.0, 150.0]), (2, [600.0, 450.0])]);

    // Midpoint (400, 300) on an 800×600 canvas → (0.5, 0.5).
    let uv = mt.midpoint_uv(800.0, 600.0).unwrap();
    assert!((uv[0] - 0.5).abs() < 0.01);
    assert!((uv[1] - 0.5).abs() < 0.01);
}

#[test]
fn midpoint_uv_clamps_to_unit_range() {
    let mt = mt_with(&[(1, [-100.0, -50.0]), (2, [100.0, 50.0])]);

    let uv = mt.midpoint_uv(800.0, 600.0).unwrap();
    assert!(uv[0] >= 0.0 && uv[0] <= 1.0);
    assert!(uv[1] >= 0.0 && uv[1] <= 1.0);
}

// ────────────────── centroid_px ──────────────────

#[test]
fn centroid_px_none_when_empty() {
    assert!(MultiTouchState::default().centroid_px().is_none());
}

#[test]
fn centroid_px_single_pointer() {
    let mt = mt_with(&[(1, [120.0, 240.0])]);

    let c = mt.centroid_px().unwrap();
    assert!((c[0] - 120.0).abs() < 0.01);
    assert!((c[1] - 240.0).abs() < 0.01);
}

#[test]
fn centroid_px_two_pointers() {
    let mt = mt_with(&[(1, [100.0, 200.0]), (2, [300.0, 400.0])]);

    let c = mt.centroid_px().unwrap();
    assert!((c[0] - 200.0).abs() < 0.01);
    assert!((c[1] - 300.0).abs() < 0.01);
}

#[test]
fn centroid_px_three_pointers() {
    let mt = mt_with(&[(1, [0.0, 0.0]), (2, [300.0, 0.0]), (3, [0.0, 300.0])]);

    let c = mt.centroid_px().unwrap();
    assert!((c[0] - 100.0).abs() < 0.01);
    assert!((c[1] - 100.0).abs() < 0.01);
}

#[test]
fn centroid_px_five_pointers() {
    let mt = mt_with(&[
        (1, [100.0, 100.0]),
        (2, [200.0, 100.0]),
        (3, [300.0, 100.0]),
        (4, [100.0, 300.0]),
        (5, [300.0, 300.0]),
    ]);

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
    let mt = mt_with(&[(1, [400.0, 300.0])]);

    let uv = mt.centroid_uv(800.0, 600.0).unwrap();
    assert!((uv[0] - 0.5).abs() < 0.01);
    assert!((uv[1] - 0.5).abs() < 0.01);
}

#[test]
fn centroid_uv_clamps_to_unit_range() {
    let mt = mt_with(&[(1, [1600.0, 1200.0])]);

    let uv = mt.centroid_uv(800.0, 600.0).unwrap();
    assert_eq!(uv[0], 1.0);
    assert_eq!(uv[1], 1.0);
}

// ────────────────── reset_gesture ──────────────────

#[test]
fn reset_gesture_clears_state_but_keeps_pointers() {
    let mut mt = mt_with(&[(1, [100.0, 200.0]), (2, [300.0, 400.0])]);
    mt.gesture_kind = TouchGestureKind::PerformanceSurface;
    mt.peak_pointer_count = 2;
    mt.initial_distance = 250.0;
    mt.initial_angle = 1.5;
    mt.initial_bpm = 120.0;
    mt.initial_detune = 50.0;
    mt.initial_centroid = [200.0, 300.0];
    mt.motion_px = 123.0;
    mt.last_ripple_motion = 88.0;
    mt.current_centroid = Some([200.0, 300.0]);

    mt.reset_gesture();

    assert_eq!(mt.pointers.len(), 2, "pointers survive a reset");
    assert_eq!(mt.gesture_kind, TouchGestureKind::None);
    assert_eq!(mt.peak_pointer_count, 0);
    assert_eq!(mt.initial_distance, 0.0);
    assert_eq!(mt.initial_angle, 0.0);
    assert_eq!(mt.initial_bpm, 0.0);
    assert_eq!(mt.initial_detune, 0.0);
    assert_eq!(mt.initial_centroid, [0.0, 0.0]);
    assert!(mt.current_centroid.is_none());
    assert_eq!(mt.motion_px, 0.0);
    assert_eq!(mt.last_ripple_motion, 0.0);
}

// ────────────────── current_centroid ──────────────────

#[test]
fn current_centroid_default_is_none() {
    assert!(MultiTouchState::default().current_centroid.is_none());
}

#[test]
fn current_centroid_tracks_pointer_movement() {
    let mut mt = mt_with(&[
        (1, [100.0, 200.0]),
        (2, [300.0, 200.0]),
        (3, [200.0, 400.0]),
    ]);
    mt.gesture_kind = TouchGestureKind::PerformanceSurface;

    mt.current_centroid = mt.centroid_px();
    let c = mt.current_centroid.unwrap();
    assert!((c[0] - 200.0).abs() < 0.01);
    assert!((c[1] - 266.67).abs() < 0.5);

    // Slide every pointer +100px in X; the centroid follows.
    mt.pointers.insert(1, [200.0, 200.0]);
    mt.pointers.insert(2, [400.0, 200.0]);
    mt.pointers.insert(3, [300.0, 400.0]);
    mt.current_centroid = mt.centroid_px();
    let c = mt.current_centroid.unwrap();
    assert!((c[0] - 300.0).abs() < 0.01);
    assert!((c[1] - 266.67).abs() < 0.5);
}

#[test]
fn current_centroid_preferred_over_fallback() {
    let mut mt = mt_with(&[
        (1, [100.0, 200.0]),
        (2, [300.0, 200.0]),
        (3, [200.0, 400.0]),
    ]);
    mt.gesture_kind = TouchGestureKind::PerformanceSurface;
    mt.current_centroid = mt.centroid_px();

    let fallback = [999.0, 999.0];
    let final_pos = mt.current_centroid.unwrap_or(fallback);
    assert!((final_pos[0] - 200.0).abs() < 0.01);
    assert!((final_pos[1] - 266.67).abs() < 0.5);

    mt.reset_gesture();
    assert_eq!(mt.current_centroid.unwrap_or(fallback), fallback);
}

// ────────────────── gesture lifecycle simulation ──────────────────

#[test]
fn simulate_two_finger_gesture_lifecycle() {
    let mut mt = mt_with(&[(1, [100.0, 300.0])]);
    assert_eq!(mt.pointers.len(), 1);
    assert!(
        mt.two_finger_metrics().is_none(),
        "one finger has no metric"
    );

    // Second finger down → start the continuous performance surface.
    mt.pointers.insert(2, [500.0, 300.0]);
    mt.peak_pointer_count = 2;
    mt.gesture_kind = TouchGestureKind::PerformanceSurface;

    let (dist, _angle) = mt.two_finger_metrics().unwrap();
    mt.initial_distance = dist;
    assert!((dist - 400.0).abs() < 0.1);

    // Spread inward: the distance ratio halves.
    mt.pointers.insert(1, [200.0, 300.0]);
    mt.pointers.insert(2, [400.0, 300.0]);
    let (new_dist, _) = mt.two_finger_metrics().unwrap();
    let ratio = new_dist / mt.initial_distance;
    assert!((ratio - 0.5).abs() < 0.01, "spread ratio should be 0.5");

    // Both fingers lift, then the gesture resets.
    mt.pointers.remove(&2);
    assert_eq!(mt.pointers.len(), 1);
    mt.pointers.remove(&1);
    assert!(mt.pointers.is_empty());
    mt.reset_gesture();
    assert_eq!(mt.gesture_kind, TouchGestureKind::None);
}

#[test]
fn simulate_three_finger_horizontal_motion() {
    let mut mt = mt_with(&[
        (1, [100.0, 300.0]),
        (2, [200.0, 300.0]),
        (3, [300.0, 300.0]),
    ]);
    mt.gesture_kind = TouchGestureKind::PerformanceSurface;
    mt.peak_pointer_count = 3;

    mt.initial_centroid = mt.centroid_px().unwrap();
    assert!((mt.initial_centroid[0] - 200.0).abs() < 0.01);
    assert!((mt.initial_centroid[1] - 300.0).abs() < 0.01);

    // Move right: every finger moves +100px in X.
    mt.pointers.insert(1, [200.0, 300.0]);
    mt.pointers.insert(2, [300.0, 300.0]);
    mt.pointers.insert(3, [400.0, 300.0]);

    let final_centroid = mt.centroid_px().unwrap();
    let dx = final_centroid[0] - mt.initial_centroid[0];
    let dy = final_centroid[1] - mt.initial_centroid[1];
    assert!(dx.abs() > dy.abs(), "motion is horizontal");
    assert!(dx > 0.0, "should be rightward");
    assert!((dx - 100.0).abs() < 0.01);
}

#[test]
fn simulate_three_finger_vertical_motion() {
    let mut mt = mt_with(&[
        (1, [300.0, 100.0]),
        (2, [300.0, 200.0]),
        (3, [300.0, 300.0]),
    ]);
    mt.gesture_kind = TouchGestureKind::PerformanceSurface;
    mt.peak_pointer_count = 3;
    mt.initial_centroid = mt.centroid_px().unwrap();

    // Move down: every finger moves +80px in Y.
    mt.pointers.insert(1, [300.0, 180.0]);
    mt.pointers.insert(2, [300.0, 280.0]);
    mt.pointers.insert(3, [300.0, 380.0]);

    let final_centroid = mt.centroid_px().unwrap();
    let dx = final_centroid[0] - mt.initial_centroid[0];
    let dy = final_centroid[1] - mt.initial_centroid[1];
    assert!(dy.abs() > dx.abs(), "motion is vertical");
    assert!(dy > 0.0, "should be downward");
}

#[test]
fn simulate_gesture_upgrade_two_to_three_fingers() {
    let mut mt = mt_with(&[(1, [100.0, 200.0]), (2, [300.0, 200.0])]);
    mt.gesture_kind = TouchGestureKind::PerformanceSurface;
    mt.peak_pointer_count = 2;

    // Third finger arrives and stays on the same continuous surface.
    mt.pointers.insert(3, [200.0, 400.0]);
    mt.gesture_kind = TouchGestureKind::PerformanceSurface;
    mt.peak_pointer_count = 3;
    mt.initial_centroid = mt.centroid_px().unwrap();

    assert_eq!(mt.gesture_kind, TouchGestureKind::PerformanceSurface);
    assert_eq!(mt.peak_pointer_count, 3);
    assert!(mt.centroid_px().is_some());
}

#[test]
fn touch_points_uv_are_sorted_and_normalized() {
    let mt = mt_with(&[(20, [800.0, 600.0]), (10, [0.0, 0.0]), (15, [400.0, 300.0])]);

    let (points, count) = mt.touch_points_uv(800.0, 600.0);

    assert_eq!(count, 3);
    assert_eq!(points[0], [0.0, 0.0, 1.0, 0.0]);
    assert_eq!(points[1], [0.5, 0.5, 1.0, 1.0]);
    assert_eq!(points[2], [1.0, 1.0, 1.0, 2.0]);
}

#[test]
fn touch_points_uv_caps_at_shader_limit() {
    let mut mt = MultiTouchState::default();
    for i in 1..=8 {
        mt.pointers.insert(i, [i as f32 * 10.0, 300.0]);
    }

    let (points, count) = mt.touch_points_uv(800.0, 600.0);

    assert_eq!(MAX_TOUCH_POINTS, 5);
    assert_eq!(count, MAX_TOUCH_POINTS);
    assert_eq!(points[4][3], 4.0);
}

// ────────────────── edge cases ──────────────────

#[test]
fn centroid_with_negative_coordinates() {
    let mt = mt_with(&[(1, [-100.0, -200.0]), (2, [100.0, 200.0])]);

    let c = mt.centroid_px().unwrap();
    assert!((c[0] - 0.0).abs() < 0.01);
    assert!((c[1] - 0.0).abs() < 0.01);

    // A centroid left of the canvas clamps to the UV origin.
    let uv = mt.centroid_uv(800.0, 600.0).unwrap();
    assert_eq!(uv[0], 0.0);
    assert_eq!(uv[1], 0.0);
}

#[test]
fn two_finger_rotation_wrapping() {
    // The angle runs id1 → id2 across the full circle.
    let mut mt = mt_with(&[(1, [400.0, 300.0]), (2, [500.0, 300.0])]);
    let (_, angle_right) = mt.two_finger_metrics().unwrap();
    assert!(angle_right.abs() < 0.01, "pointing right should be ~0 rad");

    mt.pointers.insert(2, [300.0, 300.0]);
    let (_, angle_left) = mt.two_finger_metrics().unwrap();
    assert!(
        (angle_left.abs() - std::f32::consts::PI).abs() < 0.01,
        "pointing left should be ~PI rad"
    );
}

#[test]
fn midpoint_uv_at_canvas_corners() {
    let (w, h) = (800.0, 600.0);

    let mut mt = mt_with(&[(1, [0.0, 0.0]), (2, [0.0, 0.0])]);
    let uv = mt.midpoint_uv(w, h).unwrap();
    assert_eq!(uv[0], 0.0, "top-left → UV origin");
    assert_eq!(uv[1], 0.0);

    mt.pointers.insert(1, [w, h]);
    mt.pointers.insert(2, [w, h]);
    let uv = mt.midpoint_uv(w, h).unwrap();
    assert_eq!(uv[0], 1.0, "bottom-right → UV (1, 1)");
    assert_eq!(uv[1], 1.0);
}
