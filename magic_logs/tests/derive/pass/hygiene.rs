//! The derive's code names nothing a host may shadow, and reaches the crate
//! by another path when told.

#![no_implicit_prelude]

mod platform {
    pub use ::magic_logs as logs;
}

#[derive(::magic_logs::MagicLog)]
#[log(name = "events", schema = "logs", crate = "crate::platform::logs")]
pub struct Event {
    #[log(id)]
    pub id: i64,
    #[log(time, sql = "timestamptz")]
    pub at: i64,
    #[log(level)]
    pub level: ::std::string::String,
}

// Names the derive's code must not mean.
struct Option;
struct Some;
struct None;

fn main() {
    let _ = (Option, Some, None);
    let _ = <Event as ::magic_logs::MagicLog>::SPEC;
}
