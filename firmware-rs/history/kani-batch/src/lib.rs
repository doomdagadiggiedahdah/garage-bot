//! GarageBot timing and decision logic, ported from `garage-bot/main.py`
//! (MicroPython on ESP32) so it can be model-checked with Kani.
//!
//! Everything here is pure: tick values, wall-clock seconds and network
//! outcomes are inputs. See README.md for what is and is not modelled.

pub mod close_timeout;
pub mod poll;
pub mod stale;
pub mod ticks;
pub mod uptime;

#[cfg(kani)]
mod proofs;
