// SPDX-License-Identifier: GPL-3.0-only

//! Middle-click autoscroll, the way Windows does it.
//!
//! The middle press is held back from the client. Released inside the dead zone it is
//! delivered as an ordinary click, so paste, close-tab and open-in-new-tab keep working.
//! Moved past the dead zone the window under the press scrolls, at a speed that grows
//! with the distance from the press, until the button is released; the client never sees
//! the press. The pointer moves freely and the client keeps pointer focus, as under an
//! implicit grab, and gets motion but no button.

use std::time::{Duration, Instant};

use calloop::{
    RegistrationToken,
    timer::{TimeoutAction, Timer},
};
use cosmic_comp_config::AutoscrollConfig;
use smithay::{
    backend::input::{Axis, AxisSource, ButtonState, InputTime},
    input::{
        Seat, SeatHandler,
        pointer::{
            AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent,
            GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent,
            GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent,
            GrabStartData as PointerGrabStartData, MotionEvent, PointerGrab, PointerInnerHandle,
            RelativeMotionEvent,
        },
    },
    utils::{Logical, Point, Serial},
};

use crate::{
    backend::render::cursor::{CursorState, PanDirection},
    shell::{SeatExt, focus::target::PointerFocusTarget},
    state::State,
};

pub const BTN_MIDDLE: u32 = 0x112;

/// Ticks between axis events while scrolling. Output refresh would be the ideal, but the
/// values are per elapsed millisecond, so any steady tick reads the same.
const TICK: Duration = Duration::from_millis(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Pressed, inside the dead zone: a click if released here
    Pending,
    /// Past the dead zone: scrolling until release
    Scrolling,
}

pub struct AutoscrollGrab {
    start_data: PointerGrabStartData<State>,
    seat: Seat<State>,
    config: AutoscrollConfig,
    press_serial: Serial,
    press_time: InputTime,
    phase: Phase,
    timer: Option<RegistrationToken>,
    direction: PanDirection,
}

impl AutoscrollGrab {
    pub fn new(
        seat: &Seat<State>,
        focus: Option<(PointerFocusTarget, Point<f64, Logical>)>,
        location: Point<f64, Logical>,
        serial: Serial,
        time: InputTime,
        config: AutoscrollConfig,
    ) -> AutoscrollGrab {
        AutoscrollGrab {
            start_data: PointerGrabStartData {
                focus,
                button: BTN_MIDDLE,
                location,
            },
            seat: seat.clone(),
            config,
            press_serial: serial,
            press_time: time,
            phase: Phase::Pending,
            timer: None,
            direction: PanDirection::All,
        }
    }

    /// The signed speed on each axis for a pointer at `offset` from the press, in logical
    /// pixels per millisecond.
    fn velocity(config: &AutoscrollConfig, offset: Point<f64, Logical>) -> (f64, f64) {
        let along = |distance: f64| {
            if distance.abs() <= config.dead_zone {
                0.0
            } else {
                distance.signum() * config.multiplier * distance.abs().powf(config.exponent)
            }
        };
        (along(offset.x), along(offset.y))
    }

    fn direction(config: &AutoscrollConfig, offset: Point<f64, Logical>) -> PanDirection {
        let (vx, vy) = Self::velocity(config, offset);
        match (vx.partial_cmp(&0.0), vy.partial_cmp(&0.0)) {
            (Some(std::cmp::Ordering::Equal), Some(std::cmp::Ordering::Less)) => {
                PanDirection::North
            }
            (Some(std::cmp::Ordering::Greater), Some(std::cmp::Ordering::Less)) => {
                PanDirection::NorthEast
            }
            (Some(std::cmp::Ordering::Greater), Some(std::cmp::Ordering::Equal)) => {
                PanDirection::East
            }
            (Some(std::cmp::Ordering::Greater), Some(std::cmp::Ordering::Greater)) => {
                PanDirection::SouthEast
            }
            (Some(std::cmp::Ordering::Equal), Some(std::cmp::Ordering::Greater)) => {
                PanDirection::South
            }
            (Some(std::cmp::Ordering::Less), Some(std::cmp::Ordering::Greater)) => {
                PanDirection::SouthWest
            }
            (Some(std::cmp::Ordering::Less), Some(std::cmp::Ordering::Equal)) => PanDirection::West,
            (Some(std::cmp::Ordering::Less), Some(std::cmp::Ordering::Less)) => {
                PanDirection::NorthWest
            }
            _ => PanDirection::All,
        }
    }

    fn set_cursor(&mut self, state: &mut State, direction: PanDirection) {
        if self.phase == Phase::Scrolling && self.direction == direction {
            return;
        }
        self.direction = direction;
        if let Some(cursor_state) = self.seat.user_data().get::<CursorState>() {
            cursor_state.lock().unwrap().set_shape(direction);
        }
        let output = self.seat.active_output();
        state.backend.schedule_render(&output);
    }

    fn start_scrolling(&mut self, state: &mut State) {
        self.phase = Phase::Scrolling;

        let seat = self.seat.clone();
        let anchor = self.start_data.location;
        let config = self.config.clone();
        let mut last = Instant::now();
        let token = state
            .common
            .event_loop_handle
            .insert_source(Timer::from_duration(TICK), move |now, _, state| {
                let elapsed = now.saturating_duration_since(last).as_secs_f64() * 1000.0;
                last = now;

                let pointer = seat.get_pointer().unwrap();
                let offset = pointer.current_location() - anchor;
                let (vx, vy) = Self::velocity(&config, offset);
                let (dx, dy) = (vx * elapsed, vy * elapsed);
                if dx != 0.0 || dy != 0.0 {
                    let mut frame = AxisFrame::new(InputTime::now()).source(AxisSource::Continuous);
                    if dx != 0.0 {
                        frame = frame.value(Axis::Horizontal, dx);
                    }
                    if dy != 0.0 {
                        frame = frame.value(Axis::Vertical, dy);
                    }
                    pointer.axis(state, frame);
                    pointer.frame(state);
                }
                TimeoutAction::ToDuration(TICK)
            })
            .ok();
        self.timer = token;
    }

    fn stop(&mut self, state: &mut State) {
        if let Some(token) = self.timer.take() {
            state.common.event_loop_handle.remove(token);
        }
        if let Some(cursor_state) = self.seat.user_data().get::<CursorState>() {
            let mut cursor_state = cursor_state.lock().unwrap();
            if matches!(
                cursor_state.shape(),
                Some(crate::backend::render::cursor::CursorShape::Pan(_))
            ) {
                cursor_state.unset_shape();
            }
        }
        let output = self.seat.active_output();
        state.backend.schedule_render(&output);
    }

    /// Hand the client the press it never got, so the release that follows pairs up.
    fn deliver_press(&self, state: &mut State, handle: &mut PointerInnerHandle<'_, State>) {
        let press = ButtonEvent {
            button: BTN_MIDDLE,
            state: ButtonState::Pressed,
            serial: self.press_serial,
            time: self.press_time,
        };
        state.common.xwayland_notify_pointer_button_event(
            press.button,
            press.state,
            press.serial,
            press.time,
        );
        handle.button(state, &press);
    }
}

impl PointerGrab<State> for AutoscrollGrab {
    fn motion(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Option<(<State as SeatHandler>::PointerFocus, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        // Focus stays where the press was, as it would under an implicit grab.
        handle.motion(state, self.start_data.focus.clone(), event);

        let offset = event.location - self.start_data.location;
        match self.phase {
            Phase::Pending => {
                let (vx, vy) = Self::velocity(&self.config, offset);
                if vx != 0.0 || vy != 0.0 {
                    self.set_cursor(state, Self::direction(&self.config, offset));
                    self.start_scrolling(state);
                }
            }
            Phase::Scrolling => {
                self.set_cursor(state, Self::direction(&self.config, offset));
            }
        }
    }

    fn relative_motion(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Option<(<State as SeatHandler>::PointerFocus, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(state, self.start_data.focus.clone(), event);
    }

    fn button(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &ButtonEvent,
    ) {
        let ours = event.button == BTN_MIDDLE;
        match (self.phase, event.state) {
            // A click: the press it was holding back, then this release.
            (Phase::Pending, ButtonState::Released) if ours => {
                self.deliver_press(state, handle);
                handle.button(state, event);
                handle.unset_grab(self, state, event.serial, event.time, true);
            }
            // Another button joined in: not an autoscroll, let the client sort it out.
            (Phase::Pending, ButtonState::Pressed) => {
                self.deliver_press(state, handle);
                handle.button(state, event);
                handle.unset_grab(self, state, event.serial, event.time, true);
            }
            // Scrolling ends on our release, or any other press; the client never sees ours.
            (Phase::Scrolling, ButtonState::Released) if ours => {
                self.stop(state);
                handle.unset_grab(self, state, event.serial, event.time, true);
            }
            (Phase::Scrolling, ButtonState::Pressed) => {
                self.stop(state);
                handle.button(state, event);
                handle.unset_grab(self, state, event.serial, event.time, true);
            }
            _ => handle.button(state, event),
        }
    }

    fn axis(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        details: AxisFrame,
    ) {
        handle.axis(state, details);
    }

    fn frame(&mut self, state: &mut State, handle: &mut PointerInnerHandle<'_, State>) {
        handle.frame(state);
    }

    fn gesture_swipe_begin(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(state, event);
    }

    fn gesture_swipe_update(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(state, event);
    }

    fn gesture_swipe_end(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(state, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(state, event);
    }

    fn gesture_pinch_update(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(state, event);
    }

    fn gesture_pinch_end(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(state, event);
    }

    fn gesture_hold_begin(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(state, event);
    }

    fn gesture_hold_end(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(state, event);
    }

    fn start_data(&self) -> &PointerGrabStartData<State> {
        &self.start_data
    }

    fn unset(&mut self, state: &mut State) {
        self.stop(state);
    }
}
