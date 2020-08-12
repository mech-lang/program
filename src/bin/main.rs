extern crate mech_program;
extern crate mech_utilities;
extern crate mech_core;
extern crate crossbeam_channel;
extern crate mech_syntax;
use mech_program::{ProgramRunner, RunLoop, ClientMessage};
use mech_utilities::{RunLoopMessage, MechCode};
use mech_core::{Core, Value, Index, Table};
use mech_syntax::compiler::Compiler;

use hashbrown::HashMap;

use libloading::Library;

use crossbeam_channel::Sender;
use crossbeam_channel::Receiver;

use std::rc::Rc;

fn main() {
  let mut runner = ProgramRunner::new("test", 100000);
  let running = runner.run();
  running.send(RunLoopMessage::Code((0,MechCode::String(r#"
block
  #test = math/sin(angle: 90)"#.to_string()))));
  running.send(RunLoopMessage::PrintCore(Some(0)));
  //running.send(RunLoopMessage::PrintRuntime);
  //running.send(RunLoopMessage::Stop);
  loop{
    loop {
      match running.receive() {
        Ok(ClientMessage::String(string)) => println!("{}", string),
        Ok(ClientMessage::StepDone) => {
          break;
        }
        message => (),
      }
    }
    //running.send(RunLoopMessage::PrintCore(Some(0)));
  }
}