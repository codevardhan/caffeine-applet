//! Unit tests for `src/window.rs`.
//!
//! Loaded by the `#[path]` declaration at the bottom of that file, so
//! `use super::*` gives access to private items like `CaffeineDuration`,
//! `acquire_inhibit`, and the private fields of `CaffeineApplet`.
//!
//! Run with: cargo test
//! Ignored (logind-dependent) tests: cargo test -- --include-ignored

use super::*;
use std::fs::File;
static LID_INHIBIT_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A stand-in inhibit fd that doesn't touch D-Bus/logind at all.
/// Lets us test `update()` state transitions for an "already active"
/// applet without a real system bus / logind session.
fn dummy_fd() -> OwnedFd {
    File::open("/dev/null")
        .expect("opening /dev/null should always succeed")
        .into()
}

/// Drive one message through `update` and discard the `Task` it returns.
///
/// Nothing here runs a COSMIC runtime, so there is no one to hand the `Task`
/// to. These tests assert on the state `update` leaves behind, never on the
/// work it schedules.
fn dispatch(app: &mut CaffeineApplet, message: Message) {
    drop(cosmic::Application::update(app, message));
}

/// An instant one second in the past, for deadlines that have already expired.
///
/// `checked_sub` rather than `-`: `Instant` has no guaranteed epoch, so on a
/// machine booted moments ago the subtraction can underflow.
fn a_second_ago() -> Instant {
    Instant::now()
        .checked_sub(Duration::from_secs(1))
        .expect("monotonic clock should be at least a second past its epoch")
}

//  CaffeineDuration::label ─

#[test]
fn label_minutes15() {
    assert_eq!(CaffeineDuration::Minutes(15).label(), "15 minutes");
}

#[test]
fn label_minutes30() {
    assert_eq!(CaffeineDuration::Minutes(30).label(), "30 minutes");
}

#[test]
fn label_hour1() {
    assert_eq!(CaffeineDuration::Minutes(60).label(), "1 hour");
}

/// The singular/plural split lives in the `.ftl`, not in Rust. These cover
/// both Fluent branches for each unit, and assert the absence of the
/// directionality isolation marks Fluent inserts around placeables by default.
#[test]
fn label_picks_fluent_plural_forms_without_isolation_marks() {
    assert_eq!(CaffeineDuration::Minutes(1).label(), "1 minute");
    assert_eq!(CaffeineDuration::Minutes(2).label(), "2 minutes");
    assert_eq!(CaffeineDuration::Minutes(120).label(), "2 hours");
    for label in [
        CaffeineDuration::Minutes(1).label(),
        CaffeineDuration::Minutes(120).label(),
    ] {
        assert!(
            !label.contains('\u{2068}') && !label.contains('\u{2069}'),
            "{label:?} still carries Fluent isolation marks"
        );
    }
}

#[test]
fn label_indefinite() {
    assert_eq!(CaffeineDuration::Indefinite.label(), "Indefinitely");
}

#[test]
fn labels_are_unique() {
    let mut labels: Vec<String> = CaffeineApplet::default()
        .choices()
        .iter()
        .map(|c| c.label())
        .collect();
    let original_len = labels.len();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(labels.len(), original_len, "duplicate label found");
}

//  CaffeineDuration::duration ─

#[test]
fn duration_minutes15_is_15_minutes() {
    assert_eq!(
        CaffeineDuration::Minutes(15).duration(),
        Some(Duration::from_secs(15 * 60))
    );
}

#[test]
fn duration_minutes30_is_30_minutes() {
    assert_eq!(
        CaffeineDuration::Minutes(30).duration(),
        Some(Duration::from_secs(30 * 60))
    );
}

#[test]
fn duration_hour1_is_60_minutes() {
    assert_eq!(
        CaffeineDuration::Minutes(60).duration(),
        Some(Duration::from_secs(60 * 60))
    );
}

#[test]
fn duration_indefinite_is_none() {
    assert_eq!(CaffeineDuration::Indefinite.duration(), None);
}

#[test]
fn only_indefinite_has_no_duration() {
    for choice in CaffeineApplet::default().choices() {
        let has_duration = choice.duration().is_some();
        assert_eq!(
            has_duration,
            choice != CaffeineDuration::Indefinite,
            "{choice:?} duration()-presence disagrees with Indefinite check"
        );
    }
}

#[test]
fn timed_durations_increase_with_choice_order() {
    let m15 = CaffeineDuration::Minutes(15).duration().unwrap();
    let m30 = CaffeineDuration::Minutes(30).duration().unwrap();
    let h1 = CaffeineDuration::Minutes(60).duration().unwrap();
    assert!(m15 < m30, "15 minutes should be shorter than 30 minutes");
    assert!(m30 < h1, "30 minutes should be shorter than 1 hour");
}

//  CaffeineApplet::default().choices() ─

#[test]
fn all_contains_four_unique_variants() {
    let all = CaffeineApplet::default().choices();
    assert_eq!(all.len(), 4);
    for (i, a) in all.iter().enumerate() {
        for (j, b) in all.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "ALL contains a duplicate variant");
            }
        }
    }
}

#[test]
fn all_contains_every_expected_variant() {
    let all = CaffeineApplet::default().choices();
    assert!(all.contains(&CaffeineDuration::Minutes(15)));
    assert!(all.contains(&CaffeineDuration::Minutes(30)));
    assert!(all.contains(&CaffeineDuration::Minutes(60)));
    assert!(all.contains(&CaffeineDuration::Indefinite));
}

//  CaffeineApplet::default ─

#[test]
fn default_applet_has_no_active_session() {
    let app = CaffeineApplet::default();
    assert!(app.popup.is_none());
    assert!(app.inhibit_fd.is_none());
    assert!(app.screensaver_inhibit.is_none());
    assert!(app.inhibit_error.is_none());
    assert!(!app.is_active());
    assert_eq!(app.active_choice, None);
    assert!(app.deadline.is_none());
}

//  update(): Message::PopupClosed ─

#[test]
fn popup_closed_clears_matching_id() {
    let id = Id::unique();
    let mut app = CaffeineApplet {
        popup: Some(id),
        ..Default::default()
    };

    dispatch(&mut app, Message::PopupClosed(id));

    assert!(app.popup.is_none());
}

#[test]
fn popup_closed_ignores_mismatched_id() {
    let open_id = Id::unique();
    let other_id = Id::unique();
    let mut app = CaffeineApplet {
        popup: Some(open_id),
        ..Default::default()
    };

    dispatch(&mut app, Message::PopupClosed(other_id));

    assert_eq!(app.popup, Some(open_id));
}

#[test]
fn popup_closed_on_already_closed_popup_is_noop() {
    let mut app = CaffeineApplet::default();
    let id = Id::unique();

    dispatch(&mut app, Message::PopupClosed(id));

    assert!(app.popup.is_none());
}

//  update(): Message::Disable ─

#[test]
fn disable_clears_active_session() {
    let mut app = CaffeineApplet {
        inhibit_fd: Some(dummy_fd()),
        active_choice: Some(CaffeineDuration::Minutes(60)),
        deadline: Some(Instant::now() + Duration::from_secs(60)),
        ..Default::default()
    };

    dispatch(&mut app, Message::Disable);

    assert!(app.inhibit_fd.is_none());
    assert!(app.screensaver_inhibit.is_none());
    assert!(!app.is_active());
    assert_eq!(app.active_choice, None);
    assert!(app.deadline.is_none());
}

#[test]
fn disable_closes_open_popup() {
    let mut app = CaffeineApplet {
        inhibit_fd: Some(dummy_fd()),
        popup: Some(Id::unique()),
        ..Default::default()
    };

    dispatch(&mut app, Message::Disable);

    assert!(app.popup.is_none());
}

#[test]
fn disable_when_already_inactive_is_noop() {
    let mut app = CaffeineApplet::default();

    dispatch(&mut app, Message::Disable);

    assert!(app.inhibit_fd.is_none());
    assert_eq!(app.active_choice, None);
    assert!(app.deadline.is_none());
}

//  update(): Message::Enable, already-active fd (no D-Bus needed) ─

#[test]
fn enable_when_already_active_updates_choice_without_reacquiring() {
    let mut app = CaffeineApplet {
        inhibit_fd: Some(dummy_fd()),
        active_choice: Some(CaffeineDuration::Minutes(15)),
        deadline: Some(Instant::now() + Duration::from_secs(15 * 60)),
        ..Default::default()
    };

    dispatch(&mut app, Message::Enable(CaffeineDuration::Minutes(60)));

    assert!(
        app.inhibit_fd.is_some(),
        "existing fd should be kept, not dropped"
    );
    assert_eq!(app.active_choice, Some(CaffeineDuration::Minutes(60)));
    assert!(app.deadline.is_some());
}

#[test]
fn enable_indefinite_when_already_active_clears_deadline() {
    let mut app = CaffeineApplet {
        inhibit_fd: Some(dummy_fd()),
        active_choice: Some(CaffeineDuration::Minutes(15)),
        deadline: Some(Instant::now() + Duration::from_secs(15 * 60)),
        ..Default::default()
    };

    dispatch(&mut app, Message::Enable(CaffeineDuration::Indefinite));

    assert_eq!(app.active_choice, Some(CaffeineDuration::Indefinite));
    assert!(app.deadline.is_none());
}

#[test]
fn enable_closes_popup_when_already_active() {
    let mut app = CaffeineApplet {
        inhibit_fd: Some(dummy_fd()),
        popup: Some(Id::unique()),
        ..Default::default()
    };

    dispatch(&mut app, Message::Enable(CaffeineDuration::Minutes(30)));

    assert!(app.popup.is_none());
}

//  update(): Message::Tick ─

#[test]
fn tick_before_deadline_leaves_session_active() {
    let mut app = CaffeineApplet {
        inhibit_fd: Some(dummy_fd()),
        active_choice: Some(CaffeineDuration::Minutes(15)),
        deadline: Some(Instant::now() + Duration::from_secs(60)),
        ..Default::default()
    };

    dispatch(&mut app, Message::Tick);

    assert!(app.inhibit_fd.is_some());
    assert_eq!(app.active_choice, Some(CaffeineDuration::Minutes(15)));
    assert!(app.deadline.is_some());
}

#[test]
fn tick_after_deadline_clears_session() {
    let mut app = CaffeineApplet {
        inhibit_fd: Some(dummy_fd()),
        active_choice: Some(CaffeineDuration::Minutes(15)),
        // Deadline already in the past.
        deadline: Some(a_second_ago()),
        ..Default::default()
    };

    dispatch(&mut app, Message::Tick);

    assert!(app.inhibit_fd.is_none());
    assert!(app.screensaver_inhibit.is_none());
    assert!(!app.is_active());
    assert_eq!(app.active_choice, None);
    assert!(app.deadline.is_none());
}

#[test]
fn tick_with_no_deadline_is_noop() {
    let mut app = CaffeineApplet::default();

    dispatch(&mut app, Message::Tick);

    assert!(app.inhibit_fd.is_none());
    assert_eq!(app.active_choice, None);
    assert!(app.deadline.is_none());
}

//  Message ─

#[test]
fn message_variants_are_debug_printable() {
    // Sanity check only: make sure Debug is derivable/doesn't panic
    // for the variants that are trivial to construct in a unit test.
    assert_eq!(format!("{:?}", Message::Disable), "Disable");
    assert_eq!(format!("{:?}", Message::Tick), "Tick");
    assert!(
        format!("{:?}", Message::Enable(CaffeineDuration::Minutes(60))).contains("Minutes(60)")
    );
}

//  acquire_inhibit (logind required) ─

/// Requires a running `org.freedesktop.login1` on the system bus.
/// Run with: cargo test -- --include-ignored
#[test]
#[ignore = "requires org.freedesktop.login1 on the system bus"]
fn acquire_inhibit_returns_valid_fd() {
    let fd = acquire_inhibit().expect("should acquire inhibit fd");
    drop(fd);
}

#[test]
#[ignore = "requires org.freedesktop.login1 on the system bus"]
fn inhibit_fd_is_open_while_held_and_released_on_drop() {
    let fd = acquire_inhibit().expect("acquire");
    assert!(fd.try_clone().is_ok(), "fd should be open while held");
    drop(fd);
    // Kernel guarantees the inhibit lock is released when the fd is closed.
}

#[test]
#[ignore = "requires org.freedesktop.login1 (system bus); also touches the session bus"]
fn enable_from_scratch_acquires_inhibit_and_sets_deadline() {
    let mut app = CaffeineApplet::default();

    dispatch(&mut app, Message::Enable(CaffeineDuration::Minutes(15)));

    assert!(app.inhibit_fd.is_some());
    assert_eq!(app.active_choice, Some(CaffeineDuration::Minutes(15)));
    assert!(app.deadline.is_some());
}

//  Screensaver inhibit ─
//
// On COSMIC, cosmic-idle drives screen-off, auto-lock and auto-suspend, and it
// consults `org.freedesktop.ScreenSaver` rather than logind. These cover the
// policy around that second inhibitor; the ones needing a live bus are ignored.

#[test]
fn screensaver_failure_degrades_instead_of_aborting_the_session() {
    let mut app = CaffeineApplet {
        // Pretend logind succeeded, as `enable` guarantees before it gets here.
        inhibit_fd: Some(dummy_fd()),
        ..Default::default()
    };

    app.finish_screensaver_inhibit(Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "no such name org.freedesktop.ScreenSaver",
    )
    .into()));

    assert!(app.screensaver_inhibit.is_none());
    assert!(app
        .inhibit_error
        .as_deref()
        .expect("degraded session should warn")
        .contains("may still blank"));
    assert!(app.is_active(), "a logind-only session is still a session");
}

#[test]
fn disable_clears_a_degraded_session_error() {
    let mut app = CaffeineApplet {
        inhibit_fd: Some(dummy_fd()),
        inhibit_error: Some("stale".into()),
        ..Default::default()
    };

    dispatch(&mut app, Message::Disable);

    assert!(app.inhibit_error.is_none());
}

#[test]
fn timer_expiry_clears_a_degraded_session_error() {
    let mut app = CaffeineApplet {
        inhibit_fd: Some(dummy_fd()),
        inhibit_error: Some("stale".into()),
        deadline: Some(a_second_ago()),
        ..Default::default()
    };

    dispatch(&mut app, Message::Tick);

    assert!(app.inhibit_error.is_none());
    assert!(app.screensaver_inhibit.is_none());
}

#[test]
fn is_active_tracks_either_main_inhibitor_but_not_the_lid_one() {
    let mut app = CaffeineApplet::default();
    assert!(!app.is_active());

    app.lid_inhibit_fd = Some(dummy_fd());
    assert!(
        !app.is_active(),
        "closed-lid mode is not a Caffeine session"
    );

    app.inhibit_fd = Some(dummy_fd());
    assert!(app.is_active());
}

#[test]
#[ignore = "requires an org.freedesktop.ScreenSaver owner (cosmic-idle) on the session bus"]
fn acquire_screensaver_inhibit_returns_a_cookie_and_releases_on_drop() {
    let inhibit = acquire_screensaver_inhibit().expect("should acquire a screensaver inhibit");
    assert!(SCREENSAVER_PATHS.contains(&inhibit.path));
    assert!(
        !inhibit.conn.is_closed(),
        "connection must outlive the cookie"
    );
    // `UnInhibit` is best effort, and closing the connection is the backstop:
    // cosmic-idle drops every inhibitor owned by a vanished unique bus name.
    drop(inhibit);
}

#[test]
#[ignore = "requires org.freedesktop.login1 (system bus) and cosmic-idle (session bus)"]
fn enable_acquires_both_inhibitors_and_disable_releases_both() {
    let mut app = CaffeineApplet::default();

    assert!(app.enable(CaffeineDuration::Minutes(15)));
    assert!(app.inhibit_fd.is_some());
    assert!(app.screensaver_inhibit.is_some());
    assert!(app.inhibit_error.is_none(), "{:?}", app.inhibit_error);

    app.disable();
    assert!(app.inhibit_fd.is_none());
    assert!(app.screensaver_inhibit.is_none());
}

#[test]
#[ignore = "requires org.freedesktop.login1 (system bus) and cosmic-idle (session bus)"]
fn timer_expiry_releases_the_screensaver_inhibit_too() {
    let mut app = CaffeineApplet::default();
    assert!(app.enable(CaffeineDuration::Minutes(15)));
    app.deadline = Some(a_second_ago());

    dispatch(&mut app, Message::Tick);

    assert!(app.screensaver_inhibit.is_none());
}

// Closed-lid mode is independent of ordinary Caffeine timers.

#[test]
fn disabling_lid_mode_releases_its_fd_without_changing_timer() {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut app = CaffeineApplet {
        lid_inhibit_fd: Some(dummy_fd()),
        inhibit_fd: Some(dummy_fd()),
        active_choice: Some(CaffeineDuration::Minutes(30)),
        deadline: Some(deadline),
        ..Default::default()
    };

    dispatch(&mut app, Message::ToggleLidInhibit(false));

    assert!(app.lid_inhibit_fd.is_none());
    assert!(app.inhibit_fd.is_some());
    assert_eq!(app.active_choice, Some(CaffeineDuration::Minutes(30)));
    assert_eq!(app.deadline, Some(deadline));
}

#[test]
fn timer_expiry_does_not_release_lid_inhibitor() {
    let mut app = CaffeineApplet {
        lid_inhibit_fd: Some(dummy_fd()),
        inhibit_fd: Some(dummy_fd()),
        active_choice: Some(CaffeineDuration::Minutes(15)),
        deadline: Some(a_second_ago()),
        ..Default::default()
    };

    dispatch(&mut app, Message::Tick);

    assert!(app.inhibit_fd.is_none());
    assert!(app.lid_inhibit_fd.is_some());
}

#[test]
fn failed_lid_request_stays_off_and_successful_retry_clears_error() {
    let mut app = CaffeineApplet::default();
    app.finish_lid_inhibit(Err(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "Lid inhibition denied",
    )
    .into()));
    assert!(app.lid_inhibit_fd.is_none());
    assert!(app.lid_inhibit_error.as_deref().unwrap().contains("denied"));

    app.finish_lid_inhibit(Ok(dummy_fd()));
    assert!(app.lid_inhibit_fd.is_some());
    assert!(app.lid_inhibit_error.is_none());
}

#[test]
#[ignore = "requires org.freedesktop.login1 on the system bus"]
fn lid_inhibitor_is_registered_and_released_on_drop() {
    // Declared before the guard is taken: an item after a statement reads as
    // though it runs there, when it is really hoisted to the whole scope.
    fn registered() -> bool {
        let conn = Connection::system().expect("system bus");
        let reply = conn
            .call_method(
                Some("org.freedesktop.login1"),
                "/org/freedesktop/login1",
                Some("org.freedesktop.login1.Manager"),
                "ListInhibitors",
                &(),
            )
            .expect("ListInhibitors");
        let rows: Vec<(String, String, String, String, u32, u32)> =
            reply.body().deserialize().expect("inhibitor rows");
        rows.iter().any(|(what, who, why, mode, _, pid)| {
            what.split(':').any(|part| part == "handle-lid-switch")
                && what.split(':').any(|part| part == "sleep")
                && who == "Caffeine Applet"
                && why == "Keep awake with lid closed"
                && mode == "block"
                && *pid == std::process::id()
        })
    }

    let _guard = LID_INHIBIT_TEST_LOCK.lock().unwrap();
    let fd = acquire_lid_inhibit().expect("acquire lid inhibitor");
    assert!(registered(), "must hold both sleep and lid-handling locks");
    drop(fd);
    for _ in 0..100 {
        if !registered() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("inhibitor remained registered after its fd was dropped");
}

#[test]
fn countdown_uses_deadline_and_rounds_up_without_drifting() {
    let start = Instant::now();
    let deadline = start + Duration::from_secs(90 * 60);
    assert_eq!(countdown(deadline, start), "90 min");
    assert_eq!(
        countdown(deadline, start + Duration::from_secs(30 * 60 + 1)),
        "60 min"
    );
    assert_eq!(
        countdown(
            deadline,
            deadline
                .checked_sub(Duration::from_millis(1))
                .expect("deadline is 90 minutes past start")
        ),
        "1 min"
    );
    assert_eq!(countdown(deadline, deadline), "0 min");
    assert_eq!(
        countdown(deadline, deadline + Duration::from_secs(1)),
        "0 min"
    );
}

#[test]
fn selected_row_has_live_remaining_time_and_indefinite_has_no_countdown() {
    let mut app = CaffeineApplet::default();
    let now = Instant::now();
    app.active_choice = Some(CaffeineDuration::Minutes(25));
    app.deadline = Some(now + Duration::from_secs(20 * 60 + 5));
    assert_eq!(
        app.choice_label(CaffeineDuration::Minutes(25), now),
        "25 minutes - 21 min left"
    );
    assert_eq!(
        app.choice_label(CaffeineDuration::Minutes(15), now),
        "15 minutes"
    );
    app.active_choice = Some(CaffeineDuration::Indefinite);
    app.deadline = None;
    assert_eq!(
        app.choice_label(CaffeineDuration::Indefinite, now),
        "Indefinitely - active"
    );
}

#[test]
fn reloading_presets_preserves_current_session_and_keeps_it_visible() {
    let test = crate::config::tests::TestConfig::new();
    Settings {
        duration_minutes: vec![5, 90],
        dangerous_enable_closed_lid_control: false,
        ..Settings::default()
    }
    .save(&test.config)
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut app = CaffeineApplet {
        config: Some(test.config.clone()),
        inhibit_fd: Some(dummy_fd()),
        active_choice: Some(CaffeineDuration::Minutes(25)),
        deadline: Some(deadline),
        ..Default::default()
    };
    app.reload_settings();
    assert_eq!(app.deadline, Some(deadline));
    assert!(app.inhibit_fd.is_some());
    assert_eq!(
        app.choices(),
        vec![
            CaffeineDuration::Minutes(25),
            CaffeineDuration::Minutes(5),
            CaffeineDuration::Minutes(90),
            CaffeineDuration::Indefinite
        ]
    );
}

#[test]
fn disabled_feature_rejects_enable_messages_without_acquiring_an_inhibitor() {
    let mut app = CaffeineApplet::default();
    dispatch(&mut app, Message::ToggleLidInhibit(true));
    assert!(app.lid_inhibit_fd.is_none());
    assert!(!app.settings.dangerous_enable_closed_lid_control);
}

#[test]
fn invalid_config_reload_keeps_last_good_values_and_running_timer() {
    let test = crate::config::tests::TestConfig::new();
    std::fs::write(&test.path, "broken").unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut app = CaffeineApplet {
        config: Some(test.config.clone()),
        inhibit_fd: Some(dummy_fd()),
        deadline: Some(deadline),
        ..Default::default()
    };
    app.settings.duration_minutes = vec![25, 90];
    app.reload_settings();
    assert_eq!(app.settings.duration_minutes, vec![25, 90]);
    assert_eq!(app.deadline, Some(deadline));
    assert!(app.inhibit_fd.is_some());
    assert!(app.config_error.is_some());
}

#[test]
#[ignore = "requires org.freedesktop.login1 on the system bus"]
fn feature_opt_in_never_auto_activates_and_opt_out_releases_the_inhibitor() {
    let _guard = LID_INHIBIT_TEST_LOCK.lock().unwrap();
    let test = crate::config::tests::TestConfig::new();
    let mut settings = Settings {
        dangerous_enable_closed_lid_control: true,
        ..Settings::default()
    };
    settings.save(&test.config).unwrap();
    let mut app = CaffeineApplet {
        config: Some(test.config.clone()),
        ..Default::default()
    };
    app.reload_settings();
    assert!(app.settings.dangerous_enable_closed_lid_control);
    assert!(
        app.lid_inhibit_fd.is_none(),
        "opting in only reveals the control"
    );
    dispatch(&mut app, Message::ToggleLidInhibit(true));
    assert!(app.lid_inhibit_fd.is_some());
    assert_eq!(
        Settings::load(&test.config).unwrap(),
        settings,
        "session switch must not persist activation"
    );
    settings.dangerous_enable_closed_lid_control = false;
    settings.save(&test.config).unwrap();
    app.reload_settings();
    assert!(app.lid_inhibit_fd.is_none());
}

#[test]
fn large_configured_durations_do_not_overflow_minute_conversion() {
    let choice = CaffeineDuration::Minutes(u32::MAX);
    assert_eq!(
        choice.duration().unwrap().as_secs(),
        u64::from(u32::MAX) * 60
    );
    let mut app = CaffeineApplet {
        inhibit_fd: Some(dummy_fd()),
        ..Default::default()
    };
    assert!(app.enable(choice));
    assert!(app.deadline.unwrap() > Instant::now());
}

#[test]
fn default_duration_is_indefinite_until_configured_and_need_not_be_a_preset() {
    let mut app = CaffeineApplet::default();
    assert_eq!(app.default_choice(), CaffeineDuration::Indefinite);
    app.settings.default_duration_minutes = Some(25);
    assert_eq!(app.default_choice(), CaffeineDuration::Minutes(25));
    assert!(!app.settings.duration_minutes.contains(&25));
}

#[test]
fn explicit_menu_selection_ignores_configured_default() {
    let mut app = CaffeineApplet::default();
    app.settings.default_duration_minutes = Some(45);
    app.inhibit_fd = Some(dummy_fd());
    dispatch(&mut app, Message::Enable(CaffeineDuration::Minutes(15)));
    assert_eq!(app.active_choice, Some(CaffeineDuration::Minutes(15)));
}

#[test]
#[ignore = "requires org.freedesktop.login1 (system bus); also touches the session bus"]
fn left_click_loads_configured_default_and_next_click_ends_session() {
    let test = crate::config::tests::TestConfig::new();
    let settings = Settings {
        default_duration_minutes: Some(25),
        ..Settings::default()
    };
    settings.save(&test.config).unwrap();
    let mut app = CaffeineApplet {
        config: Some(test.config.clone()),
        ..Default::default()
    };
    let before = Instant::now();
    dispatch(&mut app, Message::ToggleInhibit);
    assert_eq!(app.active_choice, Some(CaffeineDuration::Minutes(25)));
    assert!(app.inhibit_fd.is_some());
    let deadline = app.deadline.unwrap();
    assert!(deadline >= before + Duration::from_secs(25 * 60));
    assert!(deadline <= Instant::now() + Duration::from_secs(25 * 60));
    assert!(app.choices().contains(&CaffeineDuration::Minutes(25)));
    dispatch(&mut app, Message::ToggleInhibit);
    assert!(app.inhibit_fd.is_none());
    assert!(app.screensaver_inhibit.is_none());
    assert!(app.deadline.is_none());
}
