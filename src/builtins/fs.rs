use std::sync::{Arc, Mutex};

use hvmc::{
  ast, dispatch_dyn_net,
  host::Host,
  run::{LabSet, Port, Trg, Wire},
  stdlib::{ArcDef, HostedDef},
};
use term_to_net::term_to_compat_net;

use crate::{
  builtins::util::{AsDefFunction, FunctionLike, FunctionLikeHosted},
  net::net_to_hvmc::net_to_hvmc,
  readback_hvmc,
  term::{
    term_to_net::{self, Labels},
    AdtEncoding, Book, Term,
  },
};

#[derive(Clone)]
struct ReadbackData {
  book: Arc<Book>,
  host: Arc<Mutex<Host>>,
  labels: Arc<Labels>,
  adt_encoding: AdtEncoding,
}

const VICIOUS_CIRCLE_MSG: &str = "Found vicious circle";
const FILENAME_NOT_VALID_MSG: &str = "Filename is not valid string.";
const CONTENTS_NOT_VALID_MSG: &str = "Content is not valid string.";
const FS_ERROR_MSG: &str = "Filesystem error: ";
const INVALID_UTF8_MSG: &str = "UTF-8 error: ";

/// Adds the filesystem definitions (`HVM.store` and `HVM.load`) to the book
pub(crate) fn add_fs_defs(
  book: Arc<Book>,
  host: Arc<Mutex<Host>>,
  labels: Arc<Labels>,
  adt_encoding: AdtEncoding,
) {
  #[derive(Clone)]
  struct Fs0 {
    readback_data: ReadbackData,
    save: bool,
  }
  impl AsDefFunction for Fs0 {
    fn call<M: hvmc::run::Mode>(&self, net: &mut hvmc::run::Net<M>, input: Wire, output: Wire) {
      let (wire, port) = net.create_wire();
      let slf = self.clone();
      let readback_node = hvmc::stdlib::readback(slf.readback_data.host.clone(), port, move |net, tree| {
        dispatch_dyn_net!(net => {
          net.link_wire_port(wire, Port::ERA);
          let (term, _errs) = readback_hvmc(&ast::Net { root: tree,redexes: vec![]} , &slf.readback_data.book, &slf.readback_data.labels, false, slf.readback_data.adt_encoding);
          let filename = if let Term::Str { ref val } = term {
            Some(val.to_string())
          } else {
            None
          };
          net.link_wire_port(output, ArcDef::new_arc_port(LabSet::ALL, FunctionLike(Fs1 { readback_data: slf.readback_data, save: slf.save, filename  })));
        })
      });
      net.link_wire_port(input, readback_node);
    }
  }

  #[derive(Clone)]
  struct Fs1 {
    readback_data: ReadbackData,
    save: bool,
    filename: Option<String>,
  }
  impl AsDefFunction for Fs1 {
    fn call<M: hvmc::run::Mode>(&self, net: &mut hvmc::run::Net<M>, input: Wire, output: Wire) {
      if self.save {
        let (wire, port) = net.create_wire();
        let slf = self.clone();
        let mut labels = (*self.readback_data.labels).clone();
        let host = self.readback_data.host.clone();
        let readback_node = hvmc::stdlib::readback(
          self.readback_data.host.clone(),
          port,
          move |net, tree| {
            dispatch_dyn_net!(net => {
              let (term, _errs) = readback_hvmc(&ast::Net { root: tree,redexes: vec![]} , &slf.readback_data.book, &slf.readback_data.labels, false, slf.readback_data.adt_encoding);
              let contents = if let Term::Str { ref val } = term {
                Some(val.to_string())
              } else {
                None
              };
              // Save file
              let result = match (slf.filename, contents) {
                (None, _) => {
                  Term::encode_err(Term::encode_str(FILENAME_NOT_VALID_MSG))
                },
                (_, None) => {
                  Term::encode_err(Term::encode_str(CONTENTS_NOT_VALID_MSG))
                },
                (Some(filename), Some(contents)) => {
                  match std::fs::write(filename, contents) {
                    Ok(_) => Term::encode_ok(Term::Era),
                    Err(e) => Term::encode_err(Term::encode_str(&format!("{FS_ERROR_MSG}{e}"))),
                  }
                },
              };
              let result = term_to_compat_net(&result, &mut labels);
              match net_to_hvmc(&result) {
                  Ok(result) => {
                    // Return λx (x result)
                    let app = net.create_node(hvmc::run::Tag::Ctr, 0);
                    let lam = net.create_node(hvmc::run::Tag::Ctr, 0);
                    host.lock().unwrap().encode_net(net, Trg::port(app.p1), &result);
                    net.link_wire_port(wire, Port::ERA);
                    net.link_port_port(app.p0, lam.p1);
                    net.link_port_port(app.p2, lam.p2);

                    net.link_wire_port(output, lam.p0);
                  },
                  Err(_) => {
                    // If this happens, we can't even report an error to
                    // the hvm program, so simply print an error, and plug in an ERA
                    // The other option would be panicking.
                    eprintln!("{VICIOUS_CIRCLE_MSG}");
                    net.link_wire_port(output, Port::ERA);
                  },
              }
            })
          },
        );
        net.link_wire_port(input, readback_node);
      } else {
        let app = net.create_node(hvmc::run::Tag::Ctr, 0);
        let result = self
          .filename
          .as_ref()
          .ok_or(FILENAME_NOT_VALID_MSG.to_owned())
          .and_then(|filename| std::fs::read(filename).map_err(|e| format!("{FS_ERROR_MSG}{e}")))
          .and_then(|x| {
            std::str::from_utf8(&x).map(|x| x.to_string()).map_err(|e| format!("{INVALID_UTF8_MSG}{e}"))
          });
        let result = match result {
          Ok(s) => Term::encode_ok(Term::encode_str(&s)),
          Err(s) => Term::encode_err(Term::encode_str(&s)),
        };
        let mut labels = (*self.readback_data.labels).clone();
        let result = term_to_compat_net(&result, &mut labels);
        if let Ok(result) = net_to_hvmc(&result) {
          self.readback_data.host.lock().unwrap().encode_net(net, Trg::port(app.p1), &result);
        } else {
          eprintln!("{VICIOUS_CIRCLE_MSG}");
          net.link_port_port(Port::ERA, app.p1);
        }
        net.link_wire_port(output, app.p2);
        net.link_wire_port(input, app.p0);
      }
    }
  }
  let readback_data = ReadbackData { book, host: host.clone(), labels, adt_encoding };
  host.lock().unwrap().insert_def("HVM.store", unsafe {
    HostedDef::new_hosted(
      LabSet::ALL,
      FunctionLikeHosted(Fs0 { readback_data: readback_data.clone(), save: true }),
    )
  });
  host.lock().unwrap().insert_def("HVM.load", unsafe {
    HostedDef::new_hosted(
      LabSet::ALL,
      FunctionLikeHosted(Fs0 { readback_data: readback_data.clone(), save: false }),
    )
  });
}
