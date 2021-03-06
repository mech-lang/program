use mech_core::{Core, humanize, Register, Transaction, Change, Error, ErrorType};
use mech_core::{Block, BlockState};
use mech_core::{Table, TableId, TableIndex};
use mech_core::hash_string;
use mech_syntax::compiler::Compiler;
use mech_utilities::{maximize_block, RunLoopMessage, MechSocket, MechCode, Machine, MachineRegistrar, MachineDeclaration, SocketMessage};

use std::thread::{self, JoinHandle};
use std::sync::Arc;
use hashbrown::{HashSet, HashMap};
use crossbeam_channel::Sender;
use crossbeam_channel::Receiver;
use colored::*;

use super::program::Program;
use super::persister::Persister;

use std::net::{SocketAddr, UdpSocket};
extern crate websocket;
use websocket::OwnedMessage;

use std::io;
use std::time::Instant;
use std::sync::Mutex;

extern crate miniz_oxide;

use miniz_oxide::inflate::decompress_to_vec;
use miniz_oxide::deflate::compress_to_vec;

// ## Run Loop

// Client messages are sent to the client from the run loop

/*pub enum MechChannel {
  Crossbeam(crossbeam_channel::Sender<ClientMessage>),
  UdpSocket(UdpSocket),
}

impl MechChannel {

  pub fn send(&mut self, message: ClientMessage) { 
    match &self {
      MechChannel::Crossbeam(sender) => {
        sender.send(message);
      }
      MechChannel::UdpSocket(socket) => {
        let msg: Vec<u8> = bincode::serialize(&message).unwrap();
        socket.send(&msg);
      }
    }
  }
}*/

#[derive(Debug, Clone)]
pub enum ClientMessage {
  Stop,
  Pause,
  Resume,
  Clear,
  Exit(i32),
  Time(usize),
  NewBlocks(usize),
  Table(Option<Table>),
  Transaction(Transaction),
  String(String),
  //Block(Block),
  StepDone,
  Done,
  Ready,
}

pub struct RunLoop {
  pub name: String,
  pub socket_address: Option<String>,
  thread: JoinHandle<()>,
  pub outgoing: Sender<RunLoopMessage>,
  pub incoming: Receiver<ClientMessage>,
}

impl RunLoop {

  pub fn wait(self) {
    self.thread.join().unwrap();
  }

  pub fn close(&self) {
    match self.outgoing.send(RunLoopMessage::Stop) {
      Ok(..) => (),
      Err(..) => (),
    }
  }

  pub fn send(&self, msg: RunLoopMessage) -> Result<(),&str> {
    match self.outgoing.send(msg) {
      Ok(_) => Ok(()),
      Err(_) => Err("Failed to send message"),
    }
  }

  pub fn receive(&self) -> Result<ClientMessage,&str> {
    match self.incoming.recv() {
      Ok(message) => Ok(message),
      Err(_) => Err("Failed to send message"),
    }
  }

  pub fn is_empty(&self) -> bool {
    self.incoming.is_empty()
  }

  pub fn channel(&self) -> Sender<RunLoopMessage> {
    self.outgoing.clone()
  }

}

// ## Program Runner

pub struct ProgramRunner {
  pub name: String,
  pub socket: Option<Arc<UdpSocket>>,
  //pub persistence_channel: Option<Sender<PersisterMessage>>,
}

impl ProgramRunner {

  pub fn new(name:&str, capacity: usize) -> ProgramRunner {
    // Start a new program
    //let mut program = Program::new(name, capacity);

    // Start a persister
    /*
    let persist_name = format!("{}.mdb", name);
    let mut persister = Persister::new(&persist_name);
    persister.load(&persist_name);
    let changes = persister.get_changes();

    // Load database
    ////println!("{} Applying {} stored changes...", BrightCyan.paint(format!("[{}]", name)), changes.len());    
    for change in changes {
      program.mech.process_transaction(&Transaction::from_change(change));
    }*/
    
    let socket = match UdpSocket::bind("127.0.0.1:0") {
      Ok(socket) => Some(Arc::new(socket)),
      _ => None,
    };

    ProgramRunner {
      name: name.to_owned(),
      socket,
      //program,
      // TODO Use the persistence file specified by the user
      //persistence_channel: Some(persister.get_channel()),
      //persistence_channel: None,
    }
  }

  /*pub fn load_program(&mut self, input: String) -> Result<(),Box<std::error::Error>> {
    self.program.compile_program(input);
    Ok(())
  }

  pub fn load_core(&mut self, mut core: Core) {
    core.id = (self.program.cores.len() + 1) as u64;
    for input_register in &core.input {
      let input = self.program.input_map.entry(input_register.clone()).or_insert(HashSet::new());
      input.insert(core.id);
    }

    let table = core.get_table("#data".to_string()).unwrap();
    self.program.mech.remote_tables.push(table.clone());

    self.program.cores.insert(core.id, core);
  }*/

  pub fn add_persist_channel(&mut self, persister:&mut Persister) {
    //self.persistence_channel = Some(persister.get_channel());
  }

  pub fn run(self) -> RunLoop {
    //let name = self.name;
    //let outgoing = self.program.outgoing.clone();
    let (outgoing, program_incoming) = crossbeam_channel::unbounded();
    let runloop_outgoing = outgoing.clone();
    let (client_outgoing, incoming) = crossbeam_channel::unbounded();
    //let mut program = self.program;
    //let persistence_channel = self.persistence_channel;

    let name = format!("{}", &self.name.clone());
    let socket_address = match self.socket {
      Some(ref socket) => Some(socket.local_addr().unwrap().to_string()),
      None => None,
    };

    // Start start a channel receiving thread    
    let thread = thread::Builder::new().name(name.clone()).spawn(move || {

      let mut program = Program::new("new program", 100, 1000, outgoing.clone(), program_incoming);

      let program_channel_udpsocket = program.outgoing.clone();
      let program_channel_udpsocket = program.outgoing.clone();


      match &self.socket {
        Some(ref socket) => {
          let socket_receiver = socket.clone();
          // Start a socket receiving thread
          let thread = thread::Builder::new().name("remote core listener".to_string()).spawn(move || {
            let mut compressed_message = [0; 16_383];
            loop {
              match socket_receiver.recv_from(&mut compressed_message) {
                Ok((amt, src)) => {
                  let serialized_message = decompress_to_vec(&compressed_message).expect("Failed to decompress!");
                  let message: Result<SocketMessage, bincode::Error> = bincode::deserialize(&serialized_message);
                  match message {
                    Ok(SocketMessage::RemoteCoreConnect(remote_core_address)) => {
                      program_channel_udpsocket.send(RunLoopMessage::RemoteCoreConnect(MechSocket::UdpSocket(remote_core_address)));
                    }
                    Ok(SocketMessage::RemoteCoreDisconnect(remote_core_address)) => {
                      program_channel_udpsocket.send(RunLoopMessage::RemoteCoreDisconnect(remote_core_address));
                    }
                    Ok(SocketMessage::Listening(register)) => {
                      program_channel_udpsocket.send(RunLoopMessage::Listening((hash_string(&src.to_string()), register)));
                    }
                    Ok(SocketMessage::Ping) => {
                      println!("Got a ping from: {:?}", src);
                      let message = bincode::serialize(&SocketMessage::Pong).unwrap();
                      let compressed_message = compress_to_vec(&message,6);
                      socket_receiver.send_to(&compressed_message, src);
                    }
                    Ok(SocketMessage::Pong) => {
                      println!("Got a pong from: {:?}", src);
                    }
                    Ok(SocketMessage::Transaction(txn)) => {
                      program_channel_udpsocket.send(RunLoopMessage::Transaction(txn));
                    }
                    Ok(x) => println!("Unhandled Message {:?}", x),
                    Err(error) => println!("{:?}", error),
                  }
                }
                Err(error) => {

                }
              }
            }
          }).unwrap();
        }
        None => (),
      }

      //program.download_dependencies(Some(client_outgoing.clone()));

      // Step cores
      program.mech.step();
      for core in program.cores.values_mut() {
        core.step();
      }

      // Send the ready to the client to indicate that the program is initialized
      client_outgoing.send(ClientMessage::Ready);
      let mut paused = false;
      'runloop: loop {
        match (program.incoming.recv(), paused) {
          (Ok(RunLoopMessage::Transaction(txn)), false) => {
            // Process the transaction and calculate how long it took. 
            let start_ns = time::precise_time_ns();
            program.mech.process_transaction(&txn);   
            // Trigger any machines that are now ready due to the transaction
            program.trigger_machines();  
            // For all changed registers, inform all listeners of changes
            let mut set = HashSet::new();
            for changed_register in &program.mech.runtime.aggregate_changed_this_round {
              if set.contains(&changed_register.table_id) {
                continue;
              }
              set.insert(changed_register.table_id.clone());
              match (program.listeners.get(&changed_register),program.mech.get_table(*changed_register.table_id.unwrap())) {
                (Some(listeners),Some(table)) => {
                  let mut data: Vec<u8> = vec![0; (table.data.len()+1)*8+1];
                  data[0] = 0x42;
                  for i in 0..8 {
                    data[1+i] = (table.id >> (i * 8)) as u8
                  }
                  for i in 0..table.data.len() {
                    let val = table.data[i];
                    for j in 0..8 {
                      let shifted = (val >> (j * 8)) as u8;
                      data[j+i*8+9] = shifted;
                    }
                  }
                  let compressed_message = compress_to_vec(&data,6);
                  /*let change = Change::Table{table_id: table.id, data: table.data};
                  let txn = Transaction{changes: vec![change]};
                  let message = bincode::serialize(&SocketMessage::Transaction(txn)).unwrap();
                  let compressed_message = compress_to_vec(&message,6);*/
                  // Send the transaction to each listener
                  for core_id in listeners {
                    match (&self.socket,program.remote_cores.get_mut(&core_id)) {
                      (Some(ref socket),Some(MechSocket::UdpSocket(remote_core_address))) => {
                        let len = socket.send_to(&compressed_message, remote_core_address.clone()).unwrap();
                      }
                      (_,Some(MechSocket::WebSocketSender(websocket))) => {
                        match websocket.send_message(&OwnedMessage::Binary(compressed_message.clone())) {
                          Err(_) => {
                            program.outgoing.send(RunLoopMessage::RemoteCoreDisconnect(*core_id));
                          }
                          _ => (),
                        };
                      }
                      _ => (),
                    }
                  }
                }
                _ => (),
              }
            }
            let end_ns = time::precise_time_ns();
            let time = (end_ns - start_ns) as f64;
            client_outgoing.send(ClientMessage::String(format!("Txn took {:0.4?} ms", time / 1_000_000.0)));
            client_outgoing.send(ClientMessage::StepDone);
          },
          (Ok(RunLoopMessage::Listening((core_id, register))), _) => {
            match program.mech.runtime.output.contains(&register) {
              // We produce a table for which they're listening
              true => {
                // Mark down that this register has a listener for future updates
                let mut listeners = program.listeners.entry(register.clone()).or_insert(HashSet::new()); 
                listeners.insert(core_id);
                // Send over the table we have now
                match program.mech.get_table(*register.table_id.unwrap()) {
                  Some(table) => {
                    // Decompose the table into changes for a transaction
                    let mut changes = vec![];
                    changes.push(Change::NewTable{table_id: table.id, rows: table.rows, columns: table.columns});
                    for ((_,column_ix), column_alias) in table.store.column_index_to_alias.iter() {
                      changes.push(Change::SetColumnAlias{table_id: table.id, column_ix: *column_ix, column_alias: *column_alias});
                    } 
                    let mut values = vec![];
                    for i in 1..=table.rows {
                      for j in 1..=table.columns {
                        let (value, _) = table.get_unchecked(i,j);
                        values.push((TableIndex::Index(i), TableIndex::Index(j), value));
                      }
                    }
                    changes.push(Change::Set{table_id: table.id, values});
                    let txn = Transaction{changes};
                    let message = bincode::serialize(&SocketMessage::Transaction(txn)).unwrap();
                    let compressed_message = compress_to_vec(&message,6);
                    // Send the transaction to the remote core
                    match (&self.socket,program.remote_cores.get_mut(&core_id)) {
                      (Some(ref socket),Some(MechSocket::UdpSocket(remote_core_address))) => {
                        let len = socket.send_to(&compressed_message, remote_core_address.clone()).unwrap();
                      }
                      (_,Some(MechSocket::WebSocketSender(websocket))) => {
                        websocket.send_message(&OwnedMessage::Binary(compressed_message)).unwrap();
                      }
                      _ => (),
                    }                    
                  }
                  None => (),
                }
              }, 
              false => (),
            }
          },
          (Ok(RunLoopMessage::RemoteCoreDisconnect(remote_core_id)), _) => {
            match &self.socket {
              Some(ref socket) => {
                let socket_address = hash_string(&socket.local_addr().unwrap().to_string());
                if remote_core_id != socket_address {
                  match program.remote_cores.get(&remote_core_id) {
                    None => {

                    } 
                    Some(_) => {
                      client_outgoing.send(ClientMessage::String(format!("Remote core disconnected: {}", humanize(&remote_core_id))));
                      program.remote_cores.remove(&remote_core_id);
                      for (core_id, core_address) in &program.remote_cores {
                        match core_address {
                          MechSocket::UdpSocket(core_address) => {
                            let message = bincode::serialize(&SocketMessage::RemoteCoreDisconnect(remote_core_id)).unwrap();
                            let compressed_message = compress_to_vec(&message,6);
                            let len = socket.send_to(&compressed_message, core_address.clone()).unwrap();
                          }
                          MechSocket::WebSocket(_) => {
                            // TODO
                          }
                          _ => (),
                        }
                      }
                    }
                  }
                }
              }
              None => (),
            }          
          }
          (Ok(RunLoopMessage::RemoteCoreConnect(MechSocket::UdpSocket(remote_core_address))), _) => {
            match &self.socket {
              Some(ref socket) => {
                let socket_address = socket.local_addr().unwrap().to_string();
                if remote_core_address != socket_address {
                  match program.remote_cores.get(&hash_string(&remote_core_address)) {
                    None => {
                      // We've got a new remote core. Let's ask it what it needs from us
                      // and tell it about all the other cores in our network.
                      program.remote_cores.insert(hash_string(&remote_core_address),MechSocket::UdpSocket(remote_core_address.clone()));
                      client_outgoing.send(ClientMessage::String(format!("Remote core connected: {}", humanize(&hash_string(&remote_core_address)))));
                      let message = bincode::serialize(&SocketMessage::RemoteCoreConnect(socket_address.clone())).unwrap();
                      let compressed_message = compress_to_vec(&message,6);                    
                      let len = socket.send_to(&compressed_message, remote_core_address.clone()).unwrap();
                      for (core_id, core_address) in &program.remote_cores {
                        match core_address {
                          MechSocket::UdpSocket(core_address) => {
                            let message = bincode::serialize(&SocketMessage::RemoteCoreConnect(core_address.to_string())).unwrap();
                            let compressed_message = compress_to_vec(&message,6);                    
                            let len = socket.send_to(&compressed_message, remote_core_address.clone()).unwrap();
                          }
                          MechSocket::WebSocket(_) => {
                            // TODO
                          }
                          _ => (),
                        }
                      }
                    } 
                    Some(_) => {
                      for register in &program.mech.runtime.needed_registers {
                        let message = bincode::serialize(&SocketMessage::Listening(*register)).unwrap();
                        let compressed_message = compress_to_vec(&message,6);                    
                        let len = socket.send_to(&compressed_message, remote_core_address.clone()).unwrap();
                      }
                    }
                  }
                }
              }
              None => (),
            }
          } 
          (Ok(RunLoopMessage::RemoteCoreConnect(MechSocket::WebSocket(ws_stream))), _) => {
            let remote_core_address = ws_stream.peer_addr().unwrap();
            let remote_core_id = hash_string(&remote_core_address.to_string());
            let (mut ws_incoming, mut ws_outgoing) = ws_stream.split().unwrap();
            // Tell the remote websocket what this core is listening for
            for register in &program.mech.runtime.needed_registers {
              let message = bincode::serialize(&SocketMessage::Listening(*register)).unwrap();
              let compressed_message = compress_to_vec(&message,6);
              ws_outgoing.send_message(&OwnedMessage::Binary(compressed_message)).unwrap();
            }
            // Store the websocket sender
            program.remote_cores.insert(remote_core_id, MechSocket::WebSocketSender(ws_outgoing));
            let program_channel_websocket = program.outgoing.clone();
            client_outgoing.send(ClientMessage::String(format!("Remote core connected: {}", humanize(&hash_string(&remote_core_address.to_string())))));
            thread::spawn(move || {
              for message in ws_incoming.incoming_messages() {
                let message = message.unwrap();
                match message {
                  OwnedMessage::Close(_) => {
                    return;
                  }
                  OwnedMessage::Binary(msg) => {
                    let message: Result<SocketMessage, bincode::Error> = bincode::deserialize(&msg);
                    match message {
                      Ok(SocketMessage::Listening(register)) => {
                        program_channel_websocket.send(RunLoopMessage::Listening((remote_core_id, register)));
                      }
                      Ok(SocketMessage::Transaction(txn)) => {
                        program_channel_websocket.send(RunLoopMessage::Transaction(txn));
                      },
                      x => {println!("Unhandled Message: {:?}", x);},
                    }
                  }
                  _ => (),
                }
              }  
            });
          }
          (Ok(RunLoopMessage::String((string,color))), _) => {
            let r: u8 = (color >> 16) as u8;
            let g: u8 = (color >> 8) as u8;
            let b: u8 = color as u8;
            let colored_string = format!("{}", string.truecolor(r,g,b));
            client_outgoing.send(ClientMessage::String(colored_string));
          } 
          (Ok(RunLoopMessage::Exit(exit_code)), _) => {
            client_outgoing.send(ClientMessage::Exit(exit_code));
          } 
          (Ok(RunLoopMessage::Code(code)), _) => {
            // Load the program
            match code {
              MechCode::String(code) => {
                let mut compiler = Compiler::new(); 
                let programs = compiler.compile_string(code);
                for p in programs {
                  program.mech.register_blocks(p.blocks); 
                }
              },
              MechCode::MiniBlocks(miniblocks) => {
                let mut blocks: Vec<Block> = Vec::new();
                for miniblock in miniblocks {
                  blocks.push(maximize_block(&miniblock));
                }
                program.mech.register_blocks(blocks);
              }
              MechCode::MiniPrograms(miniprograms) => {
                for p in miniprograms {
                  let mut blocks: Vec<Block> = Vec::new();
                  for miniblock in p.blocks {
                    blocks.push(maximize_block(&miniblock));
                  }
                  program.mech.register_blocks(blocks);
                }
              }
            }
            // Start the program
            program.trigger_machines();
            program.download_dependencies(Some(client_outgoing.clone()));
            // React to errors
            for error in &program.mech.runtime.errors {
              program.errors.insert(error.clone());
            }
            if program.errors.len() > 0 {
              let error_string = format_errors(&program);
              client_outgoing.send(ClientMessage::String(error_string));
              client_outgoing.send(ClientMessage::Exit(1));
            }
            client_outgoing.send(ClientMessage::StepDone);
          }
          (Ok(RunLoopMessage::EchoCode(code)), _) => {
            
            // Reset #ans
            program.mech.clear_table(hash_string("ans"));

            // Compile and run code
            let mut compiler = Compiler::new();
            compiler.compile_string(code);
            program.mech.register_blocks(compiler.blocks);
            program.download_dependencies(Some(client_outgoing.clone()));

            // Get the result
            let echo_table = program.mech.get_table(hash_string("ans"));
            //program.listeners.insert(Register{table_id: TableId::Global(hash_string("ans")), row: TableIndex::All, column: TableIndex::All }); 

            // Send it
            //client_outgoing.send(ClientMessage::Table(echo_table));
            client_outgoing.send(ClientMessage::StepDone);
          } 
          (Ok(RunLoopMessage::Clear), _) => {
            program.clear();
            client_outgoing.send(ClientMessage::Clear);
          },
          (Ok(RunLoopMessage::PrintCore(core_id)), _) => {
            match core_id {
              None => client_outgoing.send(ClientMessage::String(format!("There are {:?} cores running.", program.cores.len() + 1))),
              Some(0) => client_outgoing.send(ClientMessage::String(format!("{:?}", program.mech))),
              Some(core_id) => client_outgoing.send(ClientMessage::String(format!("{:?}", program.cores.get(&core_id)))),
            };
          },
          (Ok(RunLoopMessage::PrintRuntime), _) => {
            //println!("{:?}", program.mech.runtime);
            client_outgoing.send(ClientMessage::String(format!("{:?}",program.mech.runtime)));
          },
          (Ok(RunLoopMessage::Blocks(miniblocks)), _) => {
            let mut blocks: Vec<Block> = Vec::new();
            for miniblock in miniblocks {
              let mut block = Block::new(100);
              for tfms in miniblock.transformations {
                block.register_transformations(tfms);
              }
              blocks.push(block);
            }
            program.mech.register_blocks(blocks);
            program.mech.step();
            client_outgoing.send(ClientMessage::StepDone);
          }
          (Ok(RunLoopMessage::Stop), _) => { 
            client_outgoing.send(ClientMessage::Stop);
            break 'runloop;
          },
          (Ok(RunLoopMessage::GetTable(table_id)), _) => { 
            let table_msg = ClientMessage::Table(program.mech.get_table(table_id));
            client_outgoing.send(table_msg);
          },
          (Ok(RunLoopMessage::Pause), false) => { 
            paused = true;
            client_outgoing.send(ClientMessage::Pause);
          },
          (Ok(RunLoopMessage::Resume), true) => {
            paused = false;
            //program.mech.resume();
            client_outgoing.send(ClientMessage::Resume);
          },
          (Ok(RunLoopMessage::StepBack), _) => {
            if !paused {
              paused = true;
            }
            //program.mech.step_back_one();
            //client_outgoing.send(ClientMessage::Time(program.mech.offset));
          }
          (Ok(RunLoopMessage::StepForward), true) => {
            //program.mech.step_forward_one();
            //client_outgoing.send(ClientMessage::Time(program.mech.offset));
          } 
          (Err(_), _) => {
            break 'runloop
          },
          x => println!("qq{:?}", x),
        }
        //println!("{:?}", program.mech);
        client_outgoing.send(ClientMessage::Done);
      }
      /*if let Some(channel) = persistence_channel {
        channel.send(PersisterMessage::Stop);
      }*/
    }).unwrap();

    RunLoop { name, socket_address, thread, outgoing: runloop_outgoing, incoming }
  }

  /*pub fn colored_name(&self) -> term_painter::Painted<String> {
    BrightCyan.paint(format!("[{}]", &self.name))
  }*/

}


fn format_errors(program: &Program) -> String {
  let mut formatted_errors = "".to_string();
  if program.errors.len() > 0 {
    let plural = if program.errors.len() == 1 {
      ""
    } else {
      "s"
    };
    let error_notice = format!("Found {} Error{}:\n", &program.errors.len(), plural);
    formatted_errors = format!("{}\n{}\n\n", formatted_errors, error_notice.bright_red());
    for error in &program.errors {
      let block = &program.mech.runtime.blocks.get(&error.block_id).unwrap();
      formatted_errors = format!("{}{} {} {} {}\n\n", formatted_errors, "--".truecolor(246,192,78), "Block".truecolor(246,192,78), block.name, "--------------------------------------------".truecolor(246,192,78));
      match &error.error_type {
        ErrorType::DuplicateAlias(alias_id) => {
          let alias = &program.mech.get_string(&alias_id).unwrap();
          formatted_errors = format!("{} Local table {:?} defined more than once.\n",formatted_errors, alias);
        },
        ErrorType::MissingFunction(function_id) => {
          let missing_function = &program.mech.get_string(&function_id).unwrap();
          formatted_errors = format!("{} Missing function: {}()\n",formatted_errors, missing_function);
        },
        ErrorType::UnsatisfiedTransformation(missing_ids) => {
          formatted_errors = format!("{} Missing:",formatted_errors);
          for id in missing_ids {
            let missing_string = &program.mech.get_string(&id).unwrap();
            formatted_errors = format!("{} {}",formatted_errors,missing_string);
          }
        },
        _ => formatted_errors = format!("{}{:?}\n", formatted_errors, error),
      }
      formatted_errors = format!("{}\n", formatted_errors);
      formatted_errors = format!("{} {} {}\n",formatted_errors, ">".bright_red(), error.step_text);
      formatted_errors = format!("{}\n{}",formatted_errors, "------------------------------------------------------\n\n".truecolor(246,192,78));
    }
  }
  formatted_errors
}