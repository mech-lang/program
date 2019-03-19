// # Mech Program

// ## Prelude
#![feature(extern_prelude)]

extern crate core;

#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate serde;

extern crate time;

extern crate mech_core;
extern crate mech_syntax;
use mech_core::{Core, Change, Transaction, Interner};
use mech_core::Value;
use mech_core::{TableIndex, Hasher};
use mech_core::{Block, Constraint};
use mech_core::{Function, Comparator};

// ## Watchers

pub trait Watcher {
  fn get_name(& self) -> String;
  fn get_columns(& self) -> usize;
  fn set_name(&mut self, &str);
  fn on_change(&mut self, store: &mut Interner, diff: Transaction);
}

// ## Modules

pub mod program;

// ## Exported Modules

pub use self::program::{ProgramRunner, RunLoop, RunLoopMessage};